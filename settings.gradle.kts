pluginManagement {
    includeBuild("build-logic")

    plugins {
        id("com.azuredoom.hytale-workspace") version "1.0.51"
        id("com.azuredoom.hytale-tools") version "1.0.51"
    }

    repositories {
        gradlePluginPortal()
        mavenCentral()
        maven {
            name = "AzureDoom Maven"
            url = uri("https://maven.azuredoom.com/mods")
        }
    }
}

plugins {
    id("org.gradle.toolchains.foojay-resolver-convention") version "1.0.0"
}

dependencyResolutionManagement {
    repositories {
        mavenCentral()
    }
}

rootProject.name = "Hynergy"
include("core", "electrical")
