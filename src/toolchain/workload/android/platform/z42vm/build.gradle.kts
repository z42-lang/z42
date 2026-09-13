// Android library module — packages the Kotlin facade + JNI bridge +
// per-ABI Rust .so files into a single `.aar`.
//
// Spec: docs/spec/archive/2026-05-12-add-platform-android/

plugins {
    id("com.android.library")
}

android {
    namespace = "io.z42.vm"
    compileSdk = 37
    // Pin the NDK used by the CMake JNI build to the one cargo-ndk uses
    // (versions.toml [build.android.ndk]); AGP would otherwise pick its own default.
    ndkVersion = "30.0.16248370"

    defaultConfig {
        minSdk = 23

        consumerProguardFiles("consumer-rules.pro")

        // Instrumented tests use AndroidJUnit4 + androidx.test.runner.
        // See src/androidTest/java/io/z42/vm/Z42VMInstrumentedTest.kt.
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"

        externalNativeBuild {
            cmake {
                cppFlags("")
                arguments("-DANDROID_STL=c++_static")
            }
        }

        ndk {
            // Mirror the cargo-ndk targets driven by build.sh (32-bit ABI 已退场；
            // 见 memory project_supported_platforms 与 versions.toml [platform.android].abis)。
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    // Source sets use AGP's default layout (no `sourceSets {}` needed):
    //   src/main/jniLibs — cargo-ndk drops libz42_platform_android.so per ABI here
    //                      (the build runs cargo ndk before ./gradlew).
    //   src/main/assets  — stdlib zpkg files copied in by the build.

    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
            version = "3.22.1"
        }
    }

    packaging {
        // Ship the cargo-ndk-built .so alongside libz42vm_jni.so. CMake
        // pulls it in as IMPORTED but Gradle still needs to copy it
        // into the AAR.
        jniLibs.useLegacyPackaging = false
    }
}

dependencies {
    // Pure Kotlin facade — no runtime AndroidX needed for v0.1.
    implementation("androidx.annotation:annotation:1.10.0")

    // Instrumented test deps — drive Z42VMInstrumentedTest.kt against
    // the Pixel 6 API 37 emulator (AVD z42_pixel6_api37). Spec:
    //   docs/spec/archive/2026-05-12-add-android-tests/specs/android-tests/spec.md
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
    androidTestImplementation("androidx.test:runner:1.7.0")
    androidTestImplementation("androidx.test:core:1.7.0")
}
