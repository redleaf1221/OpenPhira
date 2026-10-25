plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "org.flos.phira"
    // The reference APK is compileSdk 34 / targetSdk 33; the androidx versions below require 34.
    compileSdk = 34

    defaultConfig {
        applicationId = "org.flos.phira"
        minSdk = 22
        targetSdk = 33
        versionCode = 39
        versionName = "0.8.2"

        ndk {
            // The Rust engine only ships a 64-bit ARM build; emulator/x86 is out of scope.
            abiFilters += "arm64-v8a"
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            isShrinkResources = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    androidResources {
        // The `res/` assets are already a zstd stream inside the cipher, so deflating them a
        // second time only costs boot time.
        noCompress += listOf("char", "res", "ogg", "ttf")
    }

    packaging {
        // The engine `dlopen`s nothing else, but it does expect an extracted `libphira.so`
        // (it reads its own path for diagnostics).
        jniLibs.useLegacyPackaging = true
    }

    // The Rust build writes straight into this directory; see build.ps1.
    sourceSets["main"].jniLibs.srcDir("src/main/jniLibs")
}

dependencies {
    implementation("androidx.core:core-ktx:1.12.0")
    implementation("androidx.appcompat:appcompat:1.6.1")
    implementation("androidx.activity:activity-ktx:1.8.2")
    implementation("com.google.android.material:material:1.11.0")
}
