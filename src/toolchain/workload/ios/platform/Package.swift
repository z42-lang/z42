// swift-tools-version: 6.0
//
// Z42VM — SwiftPM package for embedding the z42 VM into iOS / macOS
// apps. Spec: 变更 add-platform-ios/
//             变更 add-ios-tests/

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
        // The Runtime xcframework supplies the C module, headers and library.
        .target(
            name: "Z42VM",
            dependencies: ["Z42VMC"],
            path: "Sources/Z42VM"
        ),

        // Static-library xcframework produced by build.sh. Provides the
        // actual z42_host_* / z42 runtime symbols; SwiftPM links it into
        // both library and test products.
        .binaryTarget(
            name: "Z42VMC",
            path: "Z42VM.xcframework"
        ),

        // XCTest target. R1–R7 implementation per platform-test-contract:
        //   变更 define-platform-test-contract/specs/platform-test-contract/spec.md
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
                // `xtask test app desktop --platform ios`.
                .copy("Resources/embedded"),
            ]
        ),
    ]
)
