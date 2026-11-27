# CMake build for the JNI bridge.
#
# Inputs:
#   z42vm_jni.c — declares the Java_io_z42_vm_Z42VM_native* exports.
#   ../jniLibs/<abi>/libz42.so — built by build.sh
#     via `cargo ndk`. Linked dynamically (loaded by Z42VM companion
#     `System.loadLibrary("z42")` before z42vm_jni).
#
# Output: libz42vm_jni.so under jniLibs/<abi>/.
#
# Spec: 变更 add-platform-android/

cmake_minimum_required(VERSION 3.22.1)

project(z42vm_jni LANGUAGES C)

add_library(
    z42vm_jni SHARED
    z42vm_jni.c
)

get_filename_component(Z42_WORKLOAD_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../../.." ABSOLUTE)
if(NOT DEFINED Z42_RUNTIME_INCLUDE_DIR)
    if(EXISTS "${Z42_WORKLOAD_DIR}/runtime-include.txt")
        file(READ "${Z42_WORKLOAD_DIR}/runtime-include.txt" Z42_RUNTIME_INCLUDE_DIR)
    else()
        set(Z42_RUNTIME_INCLUDE_DIR "${Z42_WORKLOAD_DIR}/../../../../runtime/include")
    endif()
endif()
file(TO_CMAKE_PATH "${Z42_RUNTIME_INCLUDE_DIR}" Z42_RUNTIME_INCLUDE_DIR)
if(NOT IS_DIRECTORY "${Z42_RUNTIME_INCLUDE_DIR}")
    message(FATAL_ERROR "z42 Runtime headers missing at ${Z42_RUNTIME_INCLUDE_DIR}")
endif()
target_include_directories(z42vm_jni PRIVATE "${Z42_RUNTIME_INCLUDE_DIR}")

# Locate the cargo-ndk-built libz42.so for the current
# ABI. Gradle's externalNativeBuild sets ANDROID_ABI; build.sh drops
# the .so files under jniLibs/<ABI>/ before invoking gradle.
set(Z42_RUST_LIB
    "${CMAKE_CURRENT_SOURCE_DIR}/../jniLibs/${ANDROID_ABI}/libz42.so"
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
