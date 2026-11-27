# Gradle builds the JNI bridge against the prebuilt Android Runtime.
#
# Gradle passes Z42_RUNTIME_INCLUDE_DIR and the NDK sets ANDROID_ABI.
# The Kotlin facade loads libz42.so before libz42vm_jni.so.

cmake_minimum_required(VERSION 3.22.1)

project(z42vm_jni LANGUAGES C)

add_library(
    z42vm_jni SHARED
    src/main/cpp/z42vm_jni.c
)

file(TO_CMAKE_PATH "${Z42_RUNTIME_INCLUDE_DIR}" Z42_RUNTIME_INCLUDE_DIR)
if(NOT Z42_RUNTIME_INCLUDE_DIR OR NOT EXISTS "${Z42_RUNTIME_INCLUDE_DIR}/z42.h")
    message(FATAL_ERROR "z42 Runtime headers missing at ${Z42_RUNTIME_INCLUDE_DIR}")
endif()
target_include_directories(z42vm_jni PRIVATE "${Z42_RUNTIME_INCLUDE_DIR}")

set(Z42_RUST_LIB
    "${CMAKE_CURRENT_SOURCE_DIR}/src/main/jniLibs/${ANDROID_ABI}/libz42.so"
)

# Imported target — Gradle's `packagingOptions` ensures the .so ships
# alongside libz42vm_jni.so in the final AAR.
# IMPORTED_NO_SONAME=TRUE makes CMake record only the file basename
# (libz42.so) in libz42vm_jni.so's DT_NEEDED entry,
# instead of the absolute build-time IMPORTED_LOCATION. Without this,
# device-side dlopen looks for the macOS build path → UnsatisfiedLinkError.
# With it, dlopen resolves via the apk's lib/<abi>/ at runtime.
add_library(z42 SHARED IMPORTED)
set_target_properties(
    z42 PROPERTIES
    IMPORTED_LOCATION  "${Z42_RUST_LIB}"
    IMPORTED_NO_SONAME TRUE
)

target_link_libraries(
    z42vm_jni
    z42
    log    # so we can __android_log_print from C if needed
)
