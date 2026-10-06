import Foundation
import Z42VMC

/// Single-instance z42 VM for iOS / macOS hosts. Mirror of the Tier 2
/// `z42_host::Host` Rust API. Construct with an optional
/// `ZpkgResolver`; `deinit` runs `z42_host_shutdown` automatically.
///
/// Spec: docs/spec/archive/2026-05-12-add-platform-ios/
///       docs/design/runtime/embedding.md §6.2 (Tier 3 iOS)
///
/// **Thread safety**: v0.1 the runtime is a process singleton and
/// callers must serialise their own calls. UI hosts dispatch `invoke`
/// off the main thread (e.g. `DispatchQueue.global().async`) and route
/// stdout back via the handler.
public final class Z42VM {
    /// User-supplied stdout sink. Called once per `Console.WriteLine`
    /// (or `Print`) with raw UTF-8 bytes.
    public var stdoutHandler: ((Data) -> Void)?
    public var stderrHandler: ((Data) -> Void)?

    /// Strong reference to the resolver so the C trampoline's
    /// `user_data` pointer stays valid for the VM's lifetime.
    private let resolverBox: ResolverBox?
    private var handle: OpaquePointer?

    /// Construct a new VM.
    ///
    /// - Parameters:
    ///   - zpkgResolver: handles `Console.WriteLine`-class dependency
    ///     loading. Default reads from `Bundle.main/stdlib/`.
    ///   - stdoutHandler / stderrHandler: optional output sinks. If
    ///     `nil`, output goes to the process stdout / stderr.
    public init(
        zpkgResolver: ZpkgResolver = BundleZpkgResolver(),
        stdoutHandler: ((Data) -> Void)? = nil,
        stderrHandler: ((Data) -> Void)? = nil
    ) throws {
        self.stdoutHandler = stdoutHandler
        self.stderrHandler = stderrHandler
        self.resolverBox = ResolverBox(resolver: zpkgResolver)

        var cfg = Z42HostConfig()
        cfg.abi_version = UInt32(Z42_HOST_ABI_VERSION)
        cfg.struct_size = UInt32(MemoryLayout<Z42HostConfig>.size)
        cfg.exec_mode = Z42_EXEC_MODE_INTERP
        cfg.zpkg_resolver = zpkgResolverTrampoline
        cfg.zpkg_resolver_user_data = UnsafeMutableRawPointer(
            Unmanaged.passUnretained(resolverBox!).toOpaque()
        )

        var raw: OpaquePointer? = nil
        let status = z42_host_initialize(&cfg, &raw)
        guard status == Z42_HOST_OK, let h = raw else {
            throw Self.lastError(prefix: "z42_host_initialize")
        }
        self.handle = h

        // Sinks installed AFTER initialize succeeds so user_data per
        // sink stays distinct from the resolver's. v0.1 wires stdout
        // only; stderr left for the unset path.
        if stdoutHandler != nil {
            let stdoutBox = SinkBox { [weak self] data in
                self?.stdoutHandler?(data)
            }
            _ = z42_host_set_stdout_sink(
                h,
                sinkTrampoline,
                UnsafeMutableRawPointer(
                    Unmanaged.passRetained(stdoutBox).toOpaque()
                )
            )
            self.stdoutBox = stdoutBox
        }
        if stderrHandler != nil {
            let stderrBox = SinkBox { [weak self] data in
                self?.stderrHandler?(data)
            }
            _ = z42_host_set_stderr_sink(
                h,
                sinkTrampoline,
                UnsafeMutableRawPointer(
                    Unmanaged.passRetained(stderrBox).toOpaque()
                )
            )
            self.stderrBox = stderrBox
        }
    }

    deinit {
        if let h = handle {
            _ = z42_host_shutdown(h)
        }
        // Release retained sink boxes (passRetained above).
        if let b = stdoutBox {
            Unmanaged.passUnretained(b).release()
        }
        if let b = stderrBox {
            Unmanaged.passUnretained(b).release()
        }
    }

    // ── load_zbc / resolve_entry / invoke ─────────────────────────────

    public func loadZbc(_ bytes: Data) throws -> Z42VMModule {
        guard let h = handle else {
            throw Z42VMError.notInit("Z42VM.loadZbc: VM already disposed")
        }
        var mod: OpaquePointer? = nil
        let status = bytes.withUnsafeBytes { rawBuf -> Int32 in
            let ptr = rawBuf.baseAddress?.assumingMemoryBound(to: UInt8.self)
            return Int32(z42_host_load_zbc(h, ptr, bytes.count, &mod).rawValue)
        }
        guard Int(status) == Int(Z42_HOST_OK.rawValue), let m = mod else {
            throw Self.lastError(prefix: "z42_host_load_zbc")
        }
        return Z42VMModule(handle: m)
    }

    public func resolveEntry(_ module: Z42VMModule, fqn: String) throws -> Z42VMEntry {
        guard let h = handle else {
            throw Z42VMError.notInit("Z42VM.resolveEntry: VM already disposed")
        }
        var entry: OpaquePointer? = nil
        let status = fqn.withCString { cstr in
            z42_host_resolve_entry(h, module.handle, cstr, &entry)
        }
        guard status == Z42_HOST_OK, let e = entry else {
            throw Self.lastError(prefix: "z42_host_resolve_entry")
        }
        return Z42VMEntry(handle: e)
    }

    @discardableResult
    public func invoke(_ entry: Z42VMEntry, args: [Z42VMValue] = []) throws -> Z42VMValue {
        var raw = args.map { $0.toRaw() }
        var result = Z42Value(tag: 0, reserved: 0, payload: 0)
        let status = raw.withUnsafeMutableBufferPointer { buf -> Z42HostStatus in
            z42_host_invoke(entry.handle, buf.baseAddress, buf.count, &result)
        }
        guard status == Z42_HOST_OK else {
            throw Self.lastError(prefix: "z42_host_invoke")
        }
        return Z42VMValue.fromRaw(result)
    }

    // ── Internals ─────────────────────────────────────────────────────

    private var stdoutBox: SinkBox?
    private var stderrBox: SinkBox?

    private static func lastError(prefix: String) -> Z42VMError {
        let err = z42_host_last_error(nil)
        let message: String = {
            if let cstr = err.message {
                return String(cString: cstr)
            }
            return prefix
        }()
        return Z42VMError.from(status: Int32(err.code), message: message)
    }
}

/// Wraps a `ZpkgResolver` for the C trampoline. Kept alive by the
/// `Z42VM` instance.
final class ResolverBox {
    let resolver: ZpkgResolver
    /// Holds the last-returned Data so the bytes pointer handed back to
    /// the runtime stays valid until the next resolve / VM teardown.
    var pinned: Data?

    init(resolver: ZpkgResolver) {
        self.resolver = resolver
    }
}

/// Wraps the per-sink Swift closure for the C trampoline.
final class SinkBox {
    let invoke: (Data) -> Void
    init(_ invoke: @escaping (Data) -> Void) {
        self.invoke = invoke
    }
}

/// Trampoline matching the C `Z42ZpkgResolverFn` signature.
func zpkgResolverTrampoline(
    namespace: UnsafePointer<CChar>?,
    outBytes: UnsafeMutablePointer<UnsafePointer<UInt8>?>?,
    outLength: UnsafeMutablePointer<Int>?,
    userData: UnsafeMutableRawPointer?
) -> Int32 {
    guard let ns = namespace, let ud = userData else { return 0 }
    let box = Unmanaged<ResolverBox>.fromOpaque(ud).takeUnretainedValue()
    let name = String(cString: ns)
    guard let data = box.resolver.resolve(namespace: name) else { return 0 }
    // Stash so the bytes pointer stays valid for the duration of the
    // runtime's load — runtime copies before our resolve call returns
    // but pinning across the call is defensive.
    box.pinned = data
    data.withUnsafeBytes { rawBuf in
        outBytes?.pointee = rawBuf.baseAddress?.assumingMemoryBound(to: UInt8.self)
        outLength?.pointee = data.count
    }
    return 1
}

/// Trampoline matching the C `Z42WriteSink` signature.
func sinkTrampoline(
    bytes: UnsafePointer<CChar>?,
    length: Int,
    userData: UnsafeMutableRawPointer?
) {
    guard let b = bytes, let ud = userData else { return }
    let box = Unmanaged<SinkBox>.fromOpaque(ud).takeUnretainedValue()
    let data = Data(bytes: b, count: length)
    box.invoke(data)
}
