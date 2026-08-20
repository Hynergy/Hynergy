package dev.hynergy.mna;

import java.io.IOException;
import java.io.InputStream;
import java.io.UncheckedIOException;
import java.lang.foreign.Arena;
import java.lang.foreign.SymbolLookup;
import java.nio.file.AtomicMoveNotSupportedException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.HexFormat;
import java.util.Locale;

/**
 * Locates, extracts, and opens the Hynergy MNA native library.
 */
public final class NativeLibrary {

    private static final String RESOURCE_ROOT = "natives";

    private static final String LIBRARY_NAME = "hynergy_mna";

    private static final class Holder {
        private static final SymbolLookup LOOKUP = open();
    }

    private NativeLibrary() {
    }

    public static SymbolLookup lookup() {
        return Holder.LOOKUP;
    }

    private static SymbolLookup open() {
        Path library = extract();
        try {
            return SymbolLookup.libraryLookup(library, Arena.global());
        } catch (IllegalArgumentException e) {
            throw new IllegalStateException(
                    "failed to open the native library at " + library
                            + ". The file exists but the operating system refused to load it, "
                            + "which usually means it was built for a different platform.",
                    e);
        }
    }

    private static Path extract() {
        String resource = RESOURCE_ROOT + "/" + platform() + "/" + fileName();
        byte[] bytes = read(resource);

        Path directory = Path.of(
                System.getProperty("java.io.tmpdir"),
                "hynergy-mna-" + digest(bytes));
        Path target = directory.resolve(fileName());

        if (Files.isRegularFile(target)) {
            return target;
        }

        try {
            Files.createDirectories(directory);

            Path pending = Files.createTempFile(directory, "pending-", ".tmp");
            try {
                Files.write(pending, bytes);
                move(pending, target);
            } finally {
                Files.deleteIfExists(pending);
            }
        } catch (IOException e) {
            throw new UncheckedIOException(
                    "failed to extract the native library to " + target, e);
        }

        return target;
    }

    private static void move(Path pending, Path target) throws IOException {
        try {
            Files.move(pending, target,
                    StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
        } catch (AtomicMoveNotSupportedException e) {
            Files.move(pending, target, StandardCopyOption.REPLACE_EXISTING);
        }
    }

    private static byte[] read(String resource) {
        ClassLoader loader = NativeLibrary.class.getClassLoader();
        try (InputStream in = loader.getResourceAsStream(resource)) {
            if (in == null) {
                throw new IllegalStateException(
                        "the native library is not on the classpath at '" + resource + "'. "
                                + "Check that the Gradle 'stageNative' task ran and that this "
                                + "platform is one the build produces a library for.");
            }
            return in.readAllBytes();
        } catch (IOException e) {
            throw new UncheckedIOException("failed to read '" + resource + "'", e);
        }
    }

    private static String digest(byte[] bytes) {
        try {
            byte[] hash = MessageDigest.getInstance("SHA-256").digest(bytes);
            return HexFormat.of().formatHex(hash, 0, 8);
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException("SHA-256 is required but unavailable", e);
        }
    }

    static String platform() {
        return operatingSystem() + "-" + architecture();
    }

    private static String operatingSystem() {
        String name = System.getProperty("os.name").toLowerCase(Locale.ROOT);
        if (name.contains("win")) {
            return "windows";
        }
        if (name.contains("mac")) {
            return "macos";
        }
        if (name.contains("linux")) {
            return "linux";
        }
        throw new IllegalStateException(
                "unsupported operating system: " + System.getProperty("os.name"));
    }

    private static String architecture() {
        String arch = System.getProperty("os.arch").toLowerCase(Locale.ROOT);
        return switch (arch) {
            case "amd64", "x86_64" -> "x86_64";
            case "aarch64", "arm64" -> "aarch64";
            default -> throw new IllegalStateException("unsupported architecture: " + arch);
        };
    }

    private static String fileName() {
        return switch (operatingSystem()) {
            case "windows" -> LIBRARY_NAME + ".dll";
            case "macos" -> "lib" + LIBRARY_NAME + ".dylib";
            default -> "lib" + LIBRARY_NAME + ".so";
        };
    }
}
