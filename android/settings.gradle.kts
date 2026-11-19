// Open Bus for Android: its app, the emulator's game screen, SDL's activity
// running the emulator's own library (tools/build-android builds it).
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}
rootProject.name = "Open Bus"
include(":app")
