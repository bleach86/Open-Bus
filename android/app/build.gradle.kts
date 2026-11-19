// The app: SDL's Java side and the game screen; the emulator's libraries,
// libcpu_emu.so and libSDL2.so, from src/main/jniLibs, as
// tools/build-android puts them there.
plugins {
    id("com.android.application")
}

android {
    namespace = "app.openbus"
    compileSdk = 35

    defaultConfig {
        applicationId = "app.openbus"
        // Android 7.0, as the libraries are built for.
        minSdk = 24
        targetSdk = 35
        versionCode = 1
        versionName = "0.1"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    packaging {
        // The libraries as they are, loaded from the APK, not unpacked.
        jniLibs.useLegacyPackaging = false
    }
}
