package com.fatecomputers.fcw.util;

import java.io.IOException;
import java.io.InputStream;
import java.nio.file.AtomicMoveNotSupportedException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.HexFormat;
import java.util.Optional;

/**
 * Extracts native files bundled in the mod jar ({@code /natives/<os>-<arch>/<name>}) into
 * {@code <gameDir>/fcw/natives/<sha256>/<name>}. The content hash in the path means a new mod
 * version never overwrites a file that a running process may still have open.
 */
public final class NativeFiles {
    private NativeFiles() {
    }

    public static String resourcePath(String fileName) {
        return "/natives/" + Platform.id() + "/" + fileName;
    }

    /** True if the jar carries this native file for the current platform. */
    public static boolean isBundled(String fileName) {
        return NativeFiles.class.getResource(resourcePath(fileName)) != null;
    }

    /**
     * @return the extracted file, or empty if the jar has no such file for this platform
     */
    public static Optional<Path> extract(String fileName, Path nativesRoot, boolean executable) throws IOException {
        byte[] data;
        try (InputStream in = NativeFiles.class.getResourceAsStream(resourcePath(fileName))) {
            if (in == null) {
                return Optional.empty();
            }
            data = in.readAllBytes();
        }
        String hash = sha256(data);
        Path dir = nativesRoot.resolve(hash);
        Path target = dir.resolve(fileName);
        if (!Files.isRegularFile(target) || Files.size(target) != data.length) {
            Files.createDirectories(dir);
            Path tmp = Files.createTempFile(dir, fileName, ".tmp");
            Files.write(tmp, data);
            try {
                Files.move(tmp, target, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
            } catch (AtomicMoveNotSupportedException e) {
                Files.move(tmp, target, StandardCopyOption.REPLACE_EXISTING);
            }
        }
        if (executable && !Platform.isWindows()) {
            //noinspection ResultOfMethodCallIgnored
            target.toFile().setExecutable(true, false);
        }
        return Optional.of(target);
    }

    public static String sha256(byte[] data) {
        try {
            return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(data));
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException(e);
        }
    }
}
