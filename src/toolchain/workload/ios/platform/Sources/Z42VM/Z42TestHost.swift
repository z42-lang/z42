import Foundation
import Z42VMC

/// One-shot embedded app-run for iOS (add-wasm-testhost G6).
///
/// Wraps the `z42_host_run_app` C symbol: load a self-contained app (`.zpkg`)
/// and run its entry, sourcing the stdlib from `libsDir`, then tear down. This
/// is the same core (`z42::app::run`) the desktop C shell and the Android JNI
/// bridge call — one implementation, every platform.
///
/// iOS has a real filesystem, so the bundled test-agent + stdlib zpkgs + test
/// bundle are read straight from their (bundle / temp) paths — no in-memory VFS
/// as on wasm. Because an app has no useful process stdout, the test-agent is
/// given an out-path arg and writes its JSON report to that file; the caller
/// reads it back (see `Z42VMTests.testEmbeddedBundle`).
public enum Z42TestHost {
    /// Run a bundled app through the embedded VM.
    ///
    /// - Parameters:
    ///   - appPath: path to the app artifact (the test-agent `app.zpkg`).
    ///   - entry:   entry FQN override, or `nil` for the app's baked-in entry.
    ///   - libsDir: stdlib dir (`z42.core.zpkg` + deps), or `nil`.
    ///   - args:    program args forwarded to `GetCommandLineArgs()`
    ///              (e.g. `[manifestPath, "json", outPath]`).
    /// - Returns: a process-style exit code (0 = ok).
    @discardableResult
    public static func runApp(
        appPath: String,
        entry: String? = nil,
        libsDir: String? = nil,
        args: [String]
    ) -> Int32 {
        // Heap-dup each arg into a C string; free after the call. The array is
        // `[UnsafeMutablePointer<CChar>?]`; z42_host_run_app wants
        // `const char* const*`, so rebind the buffer base to the const type.
        var argv: [UnsafeMutablePointer<CChar>?] = args.map { strdup($0) }
        defer { for p in argv { free(p) } }

        return argv.withUnsafeMutableBufferPointer { buf -> Int32 in
            let argvPtr = buf.baseAddress.map {
                UnsafeRawPointer($0).assumingMemoryBound(to: UnsafePointer<CChar>?.self)
            }
            let argc = Int32(args.count)

            // entry / libsDir are optional → nest withCString, pass NULL when nil.
            func call(_ entryC: UnsafePointer<CChar>?, _ libsC: UnsafePointer<CChar>?) -> Int32 {
                appPath.withCString { appC in
                    z42_host_run_app(appC, entryC, libsC, argc, argvPtr)
                }
            }
            switch (entry, libsDir) {
            case let (e?, l?): return e.withCString { ec in l.withCString { lc in call(ec, lc) } }
            case let (e?, nil): return e.withCString { ec in call(ec, nil) }
            case let (nil, l?): return l.withCString { lc in call(nil, lc) }
            case (nil, nil): return call(nil, nil)
            }
        }
    }
}
