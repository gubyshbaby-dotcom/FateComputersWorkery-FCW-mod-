package com.fatecomputers.fcw.server.daemon;

import com.fatecomputers.fcw.FcwMod;
import com.fatecomputers.fcw.proto.ControlProto.Envelope;

import java.io.Closeable;
import java.io.EOFException;
import java.io.IOException;
import java.net.InetSocketAddress;
import java.net.StandardProtocolFamily;
import java.net.StandardSocketOptions;
import java.net.UnixDomainSocketAddress;
import java.nio.ByteBuffer;
import java.nio.channels.SocketChannel;
import java.nio.file.Path;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.function.Consumer;

/**
 * Control channel to the daemon (docs/CONTRACT.md §3): a Unix stream socket, every frame is a
 * big-endian u32 length followed by an {@link Envelope}. One reader and one writer thread; the
 * server thread only touches the queues.
 */
final class ControlConnection implements Closeable {
    private static final int MAX_FRAME = 64 * 1024 * 1024;
    private static final ByteBuffer POISON = ByteBuffer.allocate(0);

    private final SocketChannel channel;
    private final BlockingQueue<ByteBuffer> outbound = new LinkedBlockingQueue<>();
    private final Consumer<Envelope> onMessage;
    private final Consumer<String> onClosed;
    private final Thread reader;
    private final Thread writer;
    private volatile boolean closed;

    private ControlConnection(SocketChannel channel, Consumer<Envelope> onMessage, Consumer<String> onClosed) {
        this.channel = channel;
        this.onMessage = onMessage;
        this.onClosed = onClosed;
        this.reader = new Thread(this::readLoop, "fcw-control-reader");
        this.writer = new Thread(this::writeLoop, "fcw-control-writer");
        reader.setDaemon(true);
        writer.setDaemon(true);
    }

    static ControlConnection open(Path socket, Consumer<Envelope> onMessage, Consumer<String> onClosed) throws IOException {
        SocketChannel ch = SocketChannel.open(StandardProtocolFamily.UNIX);
        try {
            ch.connect(UnixDomainSocketAddress.of(socket));
        } catch (IOException e) {
            ch.close();
            throw e;
        }
        ControlConnection c = new ControlConnection(ch, onMessage, onClosed);
        c.reader.start();
        c.writer.start();
        return c;
    }

    /** TCP variant for a daemon running as a service on another host (same frames). */
    static ControlConnection openTcp(InetSocketAddress address, int timeoutMs, Consumer<Envelope> onMessage, Consumer<String> onClosed) throws IOException {
        SocketChannel ch = SocketChannel.open();
        try {
            ch.setOption(StandardSocketOptions.TCP_NODELAY, true);
            ch.setOption(StandardSocketOptions.SO_KEEPALIVE, true);
            ch.socket().connect(address, timeoutMs);
        } catch (IOException e) {
            ch.close();
            throw e;
        }
        ControlConnection c = new ControlConnection(ch, onMessage, onClosed);
        c.reader.start();
        c.writer.start();
        return c;
    }

    void send(Envelope envelope) {
        if (closed) {
            return;
        }
        byte[] body = envelope.toByteArray();
        ByteBuffer frame = ByteBuffer.allocate(4 + body.length);
        frame.putInt(body.length).put(body).flip();
        outbound.add(frame);
    }

    boolean isClosed() {
        return closed;
    }

    private void readLoop() {
        ByteBuffer header = ByteBuffer.allocate(4);
        String reason = "connection closed";
        try {
            while (!closed) {
                header.clear();
                readFully(header);
                int len = header.flip().getInt();
                if (len < 0 || len > MAX_FRAME) {
                    throw new IOException("bad frame length " + len);
                }
                ByteBuffer body = ByteBuffer.allocate(len);
                readFully(body);
                Envelope env;
                try {
                    env = Envelope.parseFrom(body.flip());
                } catch (IOException e) {
                    FcwMod.LOGGER.warn("Unreadable message from the VM daemon: {}", e.toString());
                    continue;
                }
                onMessage.accept(env);
            }
        } catch (EOFException e) {
            reason = "daemon closed the control socket";
        } catch (IOException e) {
            if (!closed) {
                reason = e.toString();
            }
        } catch (RuntimeException e) {
            FcwMod.LOGGER.error("VM daemon reader failed", e);
            reason = e.toString();
        }
        boolean wasClosed = closed;
        close();
        if (!wasClosed) {
            onClosed.accept(reason);
        }
    }

    private void readFully(ByteBuffer buf) throws IOException {
        while (buf.hasRemaining()) {
            if (channel.read(buf) < 0) {
                throw new EOFException();
            }
        }
    }

    private void writeLoop() {
        try {
            while (!closed) {
                ByteBuffer frame = outbound.take();
                if (frame == POISON) {
                    break;
                }
                while (frame.hasRemaining()) {
                    channel.write(frame);
                }
            }
        } catch (InterruptedException ignored) {
            Thread.currentThread().interrupt();
        } catch (IOException e) {
            if (!closed) {
                FcwMod.LOGGER.warn("Writing to the VM daemon failed: {}", e.toString());
                close();
            }
        }
    }

    /** Waits (bounded) until queued frames are written, used before Shutdown. */
    void flush(long timeoutMs) {
        long deadline = System.currentTimeMillis() + timeoutMs;
        while (!outbound.isEmpty() && !closed && System.currentTimeMillis() < deadline) {
            Thread.onSpinWait();
            try {
                Thread.sleep(5);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                return;
            }
        }
    }

    @Override
    public void close() {
        if (closed) {
            return;
        }
        closed = true;
        outbound.clear();
        outbound.add(POISON);
        try {
            channel.close();
        } catch (IOException ignored) {
        }
    }
}
