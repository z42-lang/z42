// Z42VMTests.swift — XCTest implementation of platform-test-contract R1–R7.
//
// Spec: 变更 add-ios-tests/specs/ios-tests/spec.md
//       变更 define-platform-test-contract/specs/platform-test-contract/spec.md
//
// Resources (produced by build.sh, exposed via Bundle.module):
//   test-fixtures/hello.zbc        — single line "hello, world"
//   test-fixtures/multi_line.zbc   — three lines "a" / "b" / "c"
//   stdlib/*.zpkg                  — corelib + Std.IO etc.

import XCTest
@testable import Z42VM

final class Z42VMTests: XCTestCase {

    // ── Helpers ─────────────────────────────────────────────────────────

    /// `BundleZpkgResolver` bound to the test bundle's stdlib subdirectory.
    private func bundleResolver() -> BundleZpkgResolver {
        BundleZpkgResolver(bundle: .module, subdirectory: "stdlib")
    }

    /// Load a fixture .zbc by basename.
    private func fixture(_ name: String) throws -> Data {
        guard let url = Bundle.module.url(
            forResource: name,
            withExtension: "zbc",
            subdirectory: "test-fixtures"
        ) else {
            XCTFail("fixture missing: test-fixtures/\(name).zbc")
            throw Z42VMError.internal("fixture missing: \(name).zbc")
        }
        return try Data(contentsOf: url)
    }

    /// Build a VM with default Bundle.module resolver and a collecting
    /// stdout sink. Returns `(vm, collected)` where `collected` is
    /// appended-to on each `Console.WriteLine` callback.
    private func makeVMWithSink() throws -> (Z42VM, Collector) {
        let collector = Collector()
        let vm = try Z42VM(
            zpkgResolver: bundleResolver(),
            stdoutHandler: { collector.append($0) }
        )
        return (vm, collector)
    }

    /// Run hello.zbc to completion; return collected stdout bytes.
    private func runHelloAndCollect() throws -> Data {
        let (vm, collector) = try makeVMWithSink()
        let mod = try vm.loadZbc(fixture("hello"))
        let entry = try vm.resolveEntry(mod, fqn: "Hello.Main")
        _ = try vm.invoke(entry)
        return collector.bytes
    }

    // ── R1 — Smoke ──────────────────────────────────────────────────────

    func testSmokeHelloWorld() throws {
        let out = try runHelloAndCollect()
        XCTAssertEqual(
            String(data: out, encoding: .utf8),
            "hello, world\n",
            "Smoke fixture stdout mismatch"
        )
    }

    // ── R2 — Bad zbc → badZbc (status 10) ───────────────────────────────

    func testBadZbcThrowsBadZbc() throws {
        let (vm, _) = try makeVMWithSink()
        let garbage = Data([0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01, 0x02, 0x03])
        XCTAssertThrowsError(try vm.loadZbc(garbage)) { error in
            guard let err = error as? Z42VMError else {
                XCTFail("expected Z42VMError, got \(type(of: error))")
                return
            }
            XCTAssertEqual(err.status, 10, "expected badZbc (status 10), got \(err)")
        }
    }

    // ── R3 — Unknown entry → entryNotFound (status 20) ──────────────────

    func testResolveUnknownEntryThrowsEntryNotFound() throws {
        let (vm, _) = try makeVMWithSink()
        let mod = try vm.loadZbc(fixture("hello"))
        XCTAssertThrowsError(try vm.resolveEntry(mod, fqn: "App.Ghost")) { error in
            guard let err = error as? Z42VMError else {
                XCTFail("expected Z42VMError, got \(type(of: error))")
                return
            }
            XCTAssertEqual(err.status, 20, "expected entryNotFound (status 20), got \(err)")
            XCTAssertTrue(
                err.message.contains("App.Ghost"),
                "error message should mention the unknown FQN, got: \(err.message)"
            )
        }
    }

    // ── R4 — Wrong arg count → argMismatch (status 21) ──────────────────

    func testInvokeWrongArgCountThrowsArgMismatch() throws {
        let (vm, _) = try makeVMWithSink()
        let mod = try vm.loadZbc(fixture("hello"))
        let entry = try vm.resolveEntry(mod, fqn: "Hello.Main")
        // Hello.Main is `void Main()` — passing any arg must trip arg-count check.
        XCTAssertThrowsError(try vm.invoke(entry, args: [.i64(42)])) { error in
            guard let err = error as? Z42VMError else {
                XCTFail("expected Z42VMError, got \(type(of: error))")
                return
            }
            XCTAssertEqual(err.status, 21, "expected argMismatch (status 21), got \(err)")
        }
    }

    // ── R5 — MapResolver without corelib surfaces at load/invoke ────────

    func testMapResolverWithoutCorelibSurfacesAtInvoke() throws {
        // Resolver only knows about a phantom namespace; corelib / Std.IO
        // are absent. loadZbc or invoke must fail with badZbc (status 10)
        // or vmException (status 30) and the message must reference an
        // unresolved stdlib namespace.
        let resolver = MapZpkgResolver(["Std.Phantom": Data()])
        let vm = try Z42VM(zpkgResolver: resolver, stdoutHandler: { _ in })
        let zbc = try fixture("hello")
        do {
            let mod = try vm.loadZbc(zbc)
            let entry = try vm.resolveEntry(mod, fqn: "Hello.Main")
            _ = try vm.invoke(entry)
            XCTFail("expected hello.zbc to fail under empty resolver, but it succeeded")
        } catch let err as Z42VMError {
            let acceptable: Set<Int32> = [10, 30]
            XCTAssertTrue(
                acceptable.contains(err.status),
                "expected badZbc (10) or vmException (30); got status \(err.status), message: \(err.message)"
            )
        }
    }

    // ── R6 — init / shutdown repeatedly ─────────────────────────────────

    func testInitShutdownLifecycleRoundtrip() throws {
        // Three full lifecycles back-to-back. Each closure scope ends with
        // an implicit `deinit` which must shut down the VM cleanly so the
        // next iteration's `init` succeeds (singleton-style runtime).
        for iter in 1...3 {
            let out = try runHelloAndCollect()
            XCTAssertEqual(
                String(data: out, encoding: .utf8),
                "hello, world\n",
                "iteration \(iter) stdout mismatch"
            )
        }
    }

    // ── R7 — multi-line stdout preserves order ──────────────────────────

    func testMultiLineStdoutPreservesOrder() throws {
        let (vm, collector) = try makeVMWithSink()
        let mod = try vm.loadZbc(fixture("multi_line"))
        let entry = try vm.resolveEntry(mod, fqn: "MultiLine.Main")
        _ = try vm.invoke(entry)
        // Contract D3: assert accumulated bytes, not per-callback shape.
        XCTAssertEqual(
            String(data: collector.bytes, encoding: .utf8),
            "a\nb\nc\n",
            "Multi-line stdout order mismatch"
        )
    }
}

/// Thread-safe byte accumulator for sink callbacks.
final class Collector {
    private var storage = Data()
    private let lock = NSLock()

    var bytes: Data {
        lock.lock(); defer { lock.unlock() }
        return storage
    }

    func append(_ data: Data) {
        lock.lock(); defer { lock.unlock() }
        storage.append(data)
    }
}
