// swift-tools-version: 5.9
//
// Z42VM — SwiftPM package for embedding the z42 VM into iOS / macOS
// apps. Spec: docs/spec/archive/2026-05-12-add-platform-ios/
//             docs/spec/changes/add-ios-tests/

import PackageDescription

let package = Package(
    name: "Z42VM",
    platforms: [
        .iOS(.v16),      // = versions.toml platform.ios.min_ios (drift-checked)
        .macOS(.v13),    // host-side build + dev workflow + `swift test`
    ],
    products: [
        .library(name: "Z42VM", targets: ["Z42VM"]),
    ],
    targets: [
        // Swift facade. Depends on the C bridge (headers/module map)
        // and the binary xcframework (libz42_platform_ios.a slices for
        // ios-arm64 / ios-arm64_x86_64-simulator / macos-arm64).
        .target(
            name: "Z42VM",
            dependencies: ["Z42VMC", "Z42VMBinary"],
            path: "Sources/Z42VM"
        ),

        // Static-library xcframework produced by build.sh. Provides the
        // actual z42_host_* / z42 runtime symbols; SwiftPM links it into
        // both library and test products.
        .binaryTarget(
            name: "Z42VMBinary",
            path: "Z42VM.xcframework"
        ),

        // C bridge — exposes z42_host.h + z42_abi.h to Swift via a
        // clang module. The actual symbols are linked from
        // Z42VM.xcframework's binary library; this target only
        // contributes headers + module map.
        //
        // The dummy.c source is required by SwiftPM (a regular target
        // needs ≥ 1 source file). It's empty.
        .target(
            name: "Z42VMC",
            path: "Sources/Z42VMC",
            sources: ["dummy.c"],
            publicHeadersPath: "include"
        ),

        // XCTest target. R1–R7 implementation per platform-test-contract:
        //   docs/spec/archive/2026-05-12-define-platform-test-contract/specs/platform-test-contract/spec.md
        //
        // Resources are produced by build.sh:
        //   Tests/Z42VMTests/Resources/test-fixtures/{hello,multi_line}.zbc
        //   Tests/Z42VMTests/Resources/stdlib/*.zpkg
        // Use `.copy` (not `.process`) to preserve subdirectory layout
        // so `Bundle.module.url(..., subdirectory: "test-fixtures")` and
        // `subdirectory: "stdlib"` resolve correctly.
        .testTarget(
            name: "Z42VMTests",
            dependencies: ["Z42VM"],
            path: "Tests/Z42VMTests",
            resources: [
                .copy("Resources/test-fixtures"),
                .copy("Resources/stdlib"),
                // add-wasm-testhost G6: embedded test-host corpus (agent app.zpkg
                // + flat stdlib zpkgs + test bundle), populated by
                // `xtask test embedded --platform ios`.
                .copy("Resources/embedded"),
            ]
        ),
    ]
)
