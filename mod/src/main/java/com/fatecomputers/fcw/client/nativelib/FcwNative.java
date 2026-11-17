package com.fatecomputers.fcw.client.nativelib;

/**
 * JNI bindings of the client native library {@code fcw_client} (QUIC client to the VM daemon,
 * screen decoding, input, audio). Signatures are fixed by docs/CONTRACT.md §5.
 * <p>
 * Only call these after {@link NativeLoader#isLoaded()} returned true.
 */
public final class FcwNative {
    private FcwNative() {
    }

    /** Returns a session right away; the connection is established in the background. */
    public static native long connect(String host, int port, String certSha256Hex);

    /** 0 connecting, 1 ready, 2 error, 3 closed. */
    public static native int sessionState(long session);

    public static native String sessionError(long session);

    public static native void close(long session);

    /** Presents a token issued by the daemon; the result arrives as an "attach" event. */
    public static native void attach(long session, String token);

    /** Subscribes to a screen; 0 = error. */
    public static native long openView(long session, String vmId, String screenId, int maxFps);

    public static native void closeView(long view);

    /** out7: width, height, kind (1 graphical, 2 text), seq, cursorVisible, cursorX, cursorY. */
    public static native boolean viewInfo(long view, int[] out7);

    /**
     * Copies changed areas into a direct RGBA buffer of width*height*4 bytes. Writes x,y,w,h
     * quadruples into rects; returns their count, -1 = everything changed, 0 = nothing changed.
     */
    public static native int copyDirty(long view, java.nio.ByteBuffer dst, int[] rects);

    public static native void sendKey(long session, String vmId, int glfwKey, int glfwScancode, boolean down);

    public static native void sendText(long session, String vmId, String text);

    /** Sent as a datagram. */
    public static native void sendPointer(long session, String vmId, int x, int y);

    public static native void sendButtons(long session, String vmId, int mask, int x, int y);

    public static native void sendWheel(long session, String vmId, int dx, int dy, int x, int y);

    public static native void releaseAll(long session, String vmId);

    /** JSON string or null: attach, error, vm_state, screen_changed. */
    public static native String pollEvent(long session);

    /** Opus decoded to PCM 48 kHz stereo s16; returns the number of samples written. */
    public static native int readAudio(long session, String vmId, short[] pcm);
}
