package dev.hynergy.electrical.internal;

import java.io.IOException;
import java.io.InputStream;
import java.lang.foreign.SymbolLookup;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.Locale;

final class NativeLibrary {
    static SymbolLookup load() {
        String os = System.getProperty("os.name").toLowerCase(Locale.ROOT);
        String arch = System.getProperty("os.arch").toLowerCase(Locale.ROOT);

        String platform = switch (arch) {
            case "amd64", "x86_64" -> "x86_64";
            case "aarch64", "arm64" -> "aarch64";
            default -> throw new IllegalStateException(
                    "Unsupported architecture: " + System.getProperty("os.arch")
            );
        };

        String fileName;
        String osName;

        if (os.contains("win")) {
            osName = "windows";
            fileName = "hynergy_electrical.dll";
        } else if (os.contains("mac")) {
            osName = "macos";
            fileName = "libhynergy_electrical.dylib";
        } else if (os.contains("linux")) {
            osName = "linux";
            fileName = "libhynergy_electrical.so";
        } else {
            throw new IllegalStateException(
                    "Unsupported operating system: " + System.getProperty("os.name")
            );
        }

        String resourcePath =
                "/natives/" + osName + "-" + platform + "/" + fileName;

        try (InputStream input = NativeLibrary.class.getResourceAsStream(resourcePath)) {
            if (input == null) {
                throw new IllegalStateException(
                        "Native electrical library resource not found: " + resourcePath
                );
            }

            Path directory = Files.createTempDirectory("hynergy-electrical-");
            Path library = directory.resolve(fileName);

            Files.copy(input, library, StandardCopyOption.REPLACE_EXISTING);

            library.toFile().deleteOnExit();
            directory.toFile().deleteOnExit();

            System.load(library.toAbsolutePath().toString());

            return SymbolLookup.loaderLookup();
        } catch (IOException exception) {
            throw new IllegalStateException(
                    "Failed to extract native electrical library: " + resourcePath,
                    exception
            );
        }
    }
}
