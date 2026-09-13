// Root project — only declares Android Gradle Plugin + Kotlin classpath
// so the `z42vm` subproject can apply them.
//
// Spec: docs/spec/archive/2026-05-12-add-platform-android/

// AGP 9 compiles Kotlin itself (built-in Kotlin) — the `org.jetbrains.kotlin.android`
// plugin must NOT be applied anymore. AGP only depends on KGP 2.2.10 as a floor; this
// classpath entry is the documented way to build with a newer Kotlin.
buildscript {
    repositories {
        google()
        mavenCentral()
    }
    dependencies {
        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:2.4.20")
    }
}

plugins {
    id("com.android.library") version "9.4.0" apply false
}
