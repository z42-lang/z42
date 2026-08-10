package io.z42.vm

/**
 * One-shot embedded app-run for Android (add-wasm-testhost G6).
 *
 * Wraps the `z42_host_run_app` C symbol via JNI: load a self-contained app
 * (`.zpkg`) and run its entry, sourcing the stdlib from [libsDir], then tear
 * down. Same core (`z42::app::run`) the desktop C shell and iOS Swift call —
 * one implementation, every platform.
 *
 * Android has a real filesystem, so a caller copies the bundled test-agent +
 * stdlib zpkgs + test bundle out of `assets` to `cacheDir` and passes their
 * paths here (no in-memory VFS as on wasm). Because an app has no useful
 * process stdout, the test-agent is given an out-path arg and writes its JSON
 * report to that file; the caller reads it back (see
 * `Z42EmbeddedInstrumentedTest`).
 */
object Z42TestHost {
    init {
        // Idempotent with Z42VM's own load; ensures the JNI lib is present even
        // if a caller uses Z42TestHost without ever constructing a Z42VM.
        System.loadLibrary("z42vm_jni")
    }

    /**
     * Run a bundled app through the embedded VM.
     *
     * @param appPath path to the app artifact (the test-agent `app.zpkg`).
     * @param entry   entry FQN override, or `null` for the baked-in entry.
     * @param libsDir stdlib dir (`z42.core.zpkg` + deps), or `null`.
     * @param args    program args forwarded to `GetCommandLineArgs()`
     *                (e.g. `[manifestPath, "json", outPath]`).
     * @return process-style exit code (0 = ok).
     */
    fun runApp(
        appPath: String,
        entry: String? = null,
        libsDir: String? = null,
        args: Array<String>,
    ): Int = nativeRunApp(appPath, entry, libsDir, args)

    @JvmStatic
    private external fun nativeRunApp(
        appPath: String,
        entry: String?,
        libsDir: String?,
        args: Array<String>,
    ): Int
}
