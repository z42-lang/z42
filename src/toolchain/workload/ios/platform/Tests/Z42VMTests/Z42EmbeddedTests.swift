// Z42EmbeddedTests.swift — embedded test-host (add-wasm-testhost G6).
//
// Runs the bundled test corpus THROUGH the embedded VM, exactly as desktop /
// wasm do: Z42TestHost.runApp → z42_host_run_app → z42::app::run → the shared
// z42 test-agent → a JSON report file. Proves the SAME agent + bundle run on
// iOS with a real filesystem (no VFS): the agent + stdlib + bundle are copied
// into the test bundle under Resources/embedded/ by `xtask test embedded
// --platform ios`.
//
//   Resources/embedded/app/z42.testagent.zpkg
//   Resources/embedded/libs/*.zpkg
//   Resources/embedded/bundle/manifest.json + *.zbc

import XCTest
import Foundation
@testable import Z42VM

final class Z42EmbeddedTests: XCTestCase {

    /// Absolute path of a subdir under the bundled `embedded/` resources.
    private func embeddedDir(_ sub: String) throws -> String {
        guard let base = Bundle.module.resourceURL?
                .appendingPathComponent("embedded/\(sub)"),
              FileManager.default.fileExists(atPath: base.path) else {
            throw Z42VMError.internal("embedded resource missing: embedded/\(sub)")
        }
        return base.path
    }

    func testEmbeddedBundle() throws {
        let app = try embeddedDir("app") + "/z42.testagent.zpkg"
        let libs = try embeddedDir("libs")
        let manifest = try embeddedDir("bundle") + "/manifest.json"
        // Report goes to a writable temp file (an app has no useful stdout).
        let outPath = NSTemporaryDirectory() + "z42-report-\(UUID().uuidString).json"

        let code = Z42TestHost.runApp(
            appPath: app, entry: nil, libsDir: libs,
            args: [manifest, "json", outPath]
        )
        XCTAssertEqual(code, 0, "embedded run exited non-zero (\(code))")

        let report = try String(contentsOfFile: outPath, encoding: .utf8)
        XCTAssertTrue(
            report.contains("\"failed\":0") || report.contains("\"failed\": 0"),
            "embedded report reports failures:\n\(report)"
        )
    }
}
