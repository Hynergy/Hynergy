plugins {
    java
    id("com.azuredoom.hytale-tools")
}

group = property("group").toString()

val electricalProject = rootProject.project(":electrical")

tasks.named<Jar>("jar") {
    archiveBaseName.set(project.property("mod_name").toString())
    archiveVersion.set(project.property("version").toString())
    duplicatesStrategy = DuplicatesStrategy.EXCLUDE

    from(electricalProject.tasks.named<Jar>("jar").map { zipTree(it.archiveFile) })
}

java {
    toolchain.languageVersion.set(JavaLanguageVersion.of(property("java_version").toString().toInt()))
}

hytaleTools {
    javaVersion = property("java_version").toString().toInt()
    hytaleVersion = property("hytale_version").toString()
    manifestServerVersion = property("manifestServerVersion").toString()
    manifestGroup = property("manifest_group").toString()
    modId = "core"
    modDescription = property("mod_description").toString()
    modUrl = property("mod_url").toString()
    mainClass = "dev.hynergy.core.HynergyPlugin"
    modCredits = property("mod_author").toString()
    manifestDependencies = property("manifest_dependencies").toString()
    manifestOptionalDependencies = property("manifest_opt_dependencies").toString()
    curseforgeId = property("curseforge_id").toString()
    disabledByDefault = property("disabled_by_default").toString().toBoolean()
    includesPack = property("includes_pack").toString().toBoolean()
    injectServerJavadocsIntoSources = property("inject_server_javadocs_into_sources").toString().toBoolean()
    generateAssetsBinary = property("generate_assets_binary").toString().toBoolean()
    patchline = property("patchline").toString()
}

repositories {
    mavenCentral()
}

dependencies {
    implementation(project(":electrical"))
    implementation("org.jspecify:jspecify:1.0.0")
    implementation("it.unimi.dsi:fastutil:8.5.19")
    compileOnly("org.projectlombok:lombok:1.18.48")
    annotationProcessor("org.projectlombok:lombok:1.18.48")
}
