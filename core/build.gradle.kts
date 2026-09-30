plugins {
    id("hynergy.java-conventions")
    id("com.azuredoom.hytale-tools")
}

val electricalProject = project(":electrical")

tasks.named<Jar>("jar") {
    archiveBaseName.set(providers.gradleProperty("mod_name"))
    duplicatesStrategy = DuplicatesStrategy.EXCLUDE

    from(electricalProject.tasks.named<Jar>("jar").map { zipTree(it.archiveFile) })
}

hytaleTools {
    modId.set("core")
    mainClass.set("dev.hynergy.core.HynergyPlugin")

    modDescription.set(providers.gradleProperty("mod_description"))
    modUrl.set(providers.gradleProperty("mod_url"))
    modCredits.set(providers.gradleProperty("mod_author"))

    manifestDependencies.set(
        providers.gradleProperty("manifest_dependencies")
    )

    includesPack.set(
        providers.gradleProperty("includes_pack").map(String::toBoolean)
    )
}

dependencies {
    implementation(project(":electrical"))
    compileOnly(libs.jspecify)
}
