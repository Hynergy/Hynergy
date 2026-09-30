plugins {
    java
}

java {
    toolchain {
        languageVersion.set(
            providers.gradleProperty("java_version")
                .map { JavaLanguageVersion.of(it.toInt()) }
        )
    }
}

tasks.withType<JavaCompile>().configureEach {
    options.encoding = "UTF-8"
}

tasks.withType<Test>().configureEach {
    useJUnitPlatform()
}
