package io.z42.vm

/**
 * Single-instance z42 VM for Android hosts. Mirror of the Tier 2
 * `z42_host::Host` Rust API. Construct with an optional
 * [ZpkgResolver]; `close()` (or `use { ... }`) runs `z42_host_shutdown`.
 *
 * Spec: docs/spec/archive/2026-05-12-add-platform-android/
 *       docs/design/runtime/embedding.md §6.2 (Tier 3 Android)
 *
 * **Thread safety**: v0.1 the runtime is a process singleton and
 * callers must serialise their own calls. UI hosts dispatch
 * `invoke()` off the main thread (e.g. `Dispatchers.Default`) and
 * route stdout back via the handler on the main thread.
 */
class Z42VM(
    zpkgResolver: ZpkgResolver,
    /**
     * Receives `Console.WriteLine` output bytes (raw UTF-8). Called
     * once per write. Defaults to `null` → output goes to logcat /
     * stdout. Thread: same thread that called [invoke].
     */
    @JvmField var stdoutHandler: ((ByteArray) -> Unit)? = null,
    @JvmField var stderrHandler: ((ByteArray) -> Unit)? = null,
) : AutoCloseable {

    private var nativeHandle: Long = 0L
    private val resolverRef = zpkgResolver  // kept alive for JNI callback duration

    init {
        nativeHandle = nativeInitialize(resolverRef, stdoutHandler, stderrHandler)
        if (nativeHandle == 0L) {
            // JNI side already threw Z42VMException — defensive guard.
            throw Z42VMException(
                Z42VMException.INTERNAL,
                "z42_host_initialize returned NULL without throwing"
            )
        }
    }

    /** Load a `.zbc` byte buffer. */
    fun loadZbc(bytes: ByteArray): Z42VMModule =
        Z42VMModule(nativeLoadZbc(nativeHandle, bytes))

    /** Resolve an entry by fully qualified name (e.g. `"App.Main"`). */
    fun resolveEntry(module: Z42VMModule, fqn: String): Z42VMEntry =
        Z42VMEntry(nativeResolveEntry(nativeHandle, module.nativeHandle, fqn))

    /**
     * Synchronously invoke an entry. v0.1 marshal supports null / i64
     * / f64 / bool args + return. Returns the function's value (or
     * `Z42VMValue.Null` for void).
     */
    fun invoke(entry: Z42VMEntry, vararg args: Z42VMValue): Z42VMValue {
        // Pack args into parallel tag + payload arrays so the JNI side
        // can memcpy straight into Z42Value structs without boxing.
        val tags = IntArray(args.size) { args[it].tag }
        val payloads = LongArray(args.size) { args[it].payload }
        // JNI returns a 2-element `[tag, payload]` long array. We use
        // LongArray (not a packed long) because i64 payloads need a
        // full 64 bits and the tag deserves its own slot.
        val result = nativeInvoke(entry.nativeHandle, tags, payloads)
        return Z42VMValue.fromRaw(result[0].toInt(), result[1])
    }

    /** Tear down the VM. After this, the instance is unusable. */
    override fun close() {
        if (nativeHandle != 0L) {
            nativeShutdown(nativeHandle)
            nativeHandle = 0L
        }
    }

    // ── JNI surface ───────────────────────────────────────────────────

    private external fun nativeInitialize(
        resolver: ZpkgResolver,
        stdoutHandler: ((ByteArray) -> Unit)?,
        stderrHandler: ((ByteArray) -> Unit)?,
    ): Long

    private external fun nativeLoadZbc(host: Long, bytes: ByteArray): Long

    private external fun nativeResolveEntry(host: Long, module: Long, fqn: String): Long

    /**
     * Returns a 2-element `[tag, payload]` long array. Slot 0 is the
     * `Z42_VALUE_TAG_*` value cast to long; slot 1 is the raw 64-bit
     * payload (i64 / f64 bit pattern / bool flag).
     */
    private external fun nativeInvoke(
        entry: Long,
        argTags: IntArray,
        argPayloads: LongArray,
    ): LongArray

    private external fun nativeShutdown(host: Long)

    companion object {
        init {
            System.loadLibrary("z42_platform_android")
            System.loadLibrary("z42vm_jni")
        }

        /**
         * Read the namespaces a zpkg provides (its NSPC section) via
         * `z42_zpkg_read_namespaces`. Stateless — needs no VM instance.
         * [AssetZpkgResolver] uses it to map namespace → bytes from the
         * asset packages directly, so no index file is shipped.
         */
        @JvmStatic
        external fun readNamespaces(bytes: ByteArray): Array<String>
    }
}
