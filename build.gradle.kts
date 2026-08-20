plugins {
    id("com.azuredoom.hytale-workspace") version "1.+"
}

subprojects {
    tasks.register("prepareKotlinBuildScriptModel") {
        description = "Prepares the Kotlin build script model for this project."
        dependsOn(rootProject.tasks.named("prepareKotlinBuildScriptModel"))
    }
}

hytaleWorkspace {
    modProjects = listOf(":core")
    hostProject = ":core"

    manifestGroup = property("manifest_group").toString()
    hytaleVersion = property("hytale_version").toString()
    patchline = property("patchline").toString()
}
