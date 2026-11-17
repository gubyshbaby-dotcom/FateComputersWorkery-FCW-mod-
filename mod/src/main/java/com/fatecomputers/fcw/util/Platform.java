package com.fatecomputers.fcw.util;

import java.util.Locale;

/**
 * Host platform naming used for native files in the jar: {@code /natives/<os>-<arch>/...}
 * (docs/CONTRACT.md §6).
 */
public final class Platform {
    public static final String OS;
    public static final String ARCH;

    static {
        String os = System.getProperty("os.name", "").toLowerCase(Locale.ROOT);
        String arch = System.getProperty("os.arch", "").toLowerCase(Locale.ROOT);
        if (os.contains("win")) {
            OS = "windows";
        } else if (os.contains("mac") || os.contains("darwin")) {
            OS = "macos";
        } else if (os.contains("linux")) {
            OS = "linux";
        } else {
            OS = os.replace(' ', '_');
        }
        ARCH = switch (arch) {
            case "amd64", "x86_64", "x64" -> "x86_64";
            case "aarch64", "arm64" -> "aarch64";
            default -> arch;
        };
    }

    private Platform() {
    }

    public static String id() {
        return OS + "-" + ARCH;
    }

    public static boolean isWindows() {
        return "windows".equals(OS);
    }

    public static String daemonFileName() {
        return isWindows() ? "fcw-daemon.exe" : "fcw-daemon";
    }

    public static String clientLibraryFileName() {
        return switch (OS) {
            case "windows" -> "fcw_client.dll";
            case "macos" -> "libfcw_client.dylib";
            default -> "libfcw_client.so";
        };
    }
}
