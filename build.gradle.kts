plugins {
    id("com.azuredoom.hytale-workspace")
}

hytaleWorkspace {
    modProjects.set(listOf(":core"))
    hostProject.set(":core")

    manifestGroup.set(providers.gradleProperty("group"))
    hytaleVersion.set(providers.gradleProperty("hytale_version"))
    patchline.set(providers.gradleProperty("patchline"))
}
