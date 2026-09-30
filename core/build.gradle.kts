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
    modId = "core"
    mainClass = "dev.hynergy.core.HynergyPlugin"

    modDescription = property("mod_description").toString()
    modUrl = property("mod_url").toString()
    modCredits = property("mod_author").toString()

    manifestDependencies =
        property("manifest_dependencies").toString()

    includesPack = true
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
