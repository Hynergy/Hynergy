import java.util.*

plugins {
    java
}

group = "dev.hynergy"
version = "0.0.1-dev"

repositories {
    mavenCentral()
}

dependencies {
    testImplementation(platform("org.junit:junit-bom:6.0.0"))
    testImplementation("org.junit.jupiter:junit-jupiter")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
}

tasks.test {
    useJUnitPlatform()

    // FFM downcalls are restricted methods. Without this the JVM prints a warning on every run;
    // the flag makes the intent explicit instead of leaving noise in the test output.
    jvmArgs("--enable-native-access=ALL-UNNAMED")
}

val cargoPackage = "hynergy-mna-ffi"
val nativeLibraryName = "hynergy_mna"

val nativeOs: String = run {
    val osName = System.getProperty("os.name")
        .lowercase(Locale.ROOT)

    when {
        osName.contains("win") -> "windows"
        osName.contains("mac") -> "macos"
        osName.contains("linux") -> "linux"

        else -> error(
            "Unsupported operating system: ${System.getProperty("os.name")}"
        )
    }
}

val nativeArch: String = when (
    System.getProperty("os.arch").lowercase(Locale.ROOT)
) {
    "amd64",
    "x86_64" -> "x86_64"

    "aarch64",
    "arm64" -> "aarch64"

    else -> error(
        "Unsupported architecture: ${System.getProperty("os.arch")}"
    )
}

val nativeFileName: String = when (nativeOs) {
    "windows" -> "$nativeLibraryName.dll"
    "linux" -> "lib$nativeLibraryName.so"
    "macos" -> "lib$nativeLibraryName.dylib"

    else -> error("Unsupported operating system: $nativeOs")
}

val nativePlatform = "$nativeOs-$nativeArch"

val cargoTargetDir: File = layout.buildDirectory
    .dir("cargo-target")
    .get()
    .asFile

val generatedNativeResources: File = layout.buildDirectory
    .dir("generated/native-resources")
    .get()
    .asFile

val cargoOutputLibrary = File(
    cargoTargetDir,
    "release/$nativeFileName"
)

val cargoBuildNative = tasks.register<Exec>("cargoBuildNative") {
    group = "build"
    description = "Builds the Rust MNA native library for the current platform"

    workingDir(file("native"))

    environment(
        "CARGO_TARGET_DIR",
        cargoTargetDir.absolutePath
    )

    commandLine(
        "cargo",
        "build",
        "--release",
        "--package",
        cargoPackage
    )

    inputs.files(
        fileTree("native") {
            include("**/*.rs")
            include("**/Cargo.toml")
            include("Cargo.lock")
        }
    )

    outputs.file(cargoOutputLibrary)
}

val stageNative = tasks.register<Copy>("stageNative") {
    group = "build"
    description = "Stages the Rust native library for inclusion in the JAR"

    dependsOn(cargoBuildNative)

    from(cargoOutputLibrary)

    into(
        File(
            generatedNativeResources,
            "natives/$nativePlatform"
        )
    )
}

sourceSets {
    named("main") {
        resources.srcDir(generatedNativeResources)
    }
}

tasks.withType<ProcessResources>().configureEach {
    dependsOn(stageNative)
}

tasks.test {
    dependsOn(stageNative)
}