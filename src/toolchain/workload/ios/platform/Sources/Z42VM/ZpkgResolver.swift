import Foundation
import Z42VMC

/// Resolve a namespace (e.g. `"z42.core"`, `"Std.IO"`) to its zpkg
/// bytes. Returning `nil` signals "this resolver doesn't know about
/// that namespace"; the runtime then falls back to the next configured
/// resolver (typically a search-path scan).
///
/// Spec: docs/design/runtime/embedding.md §11.
public protocol ZpkgResolver {
    func resolve(namespace: String) -> Data?
}

/// Default iOS resolver. Builds a `namespace → bytes` map by enumerating
/// the bundle's `*.zpkg` resources and reading each one's `NSPC` section
/// (via the runtime helper `z42_zpkg_read_namespaces`) — there is no
/// `index.json`. A single zpkg that ships several namespaces (e.g.
/// `z42.core.zpkg` provides `z42.core` + `Std` + `Std.Exceptions`) maps
/// all of them, which a `namespace == filename` guess could not.
///
/// Spec: 变更 drop-index-json-self-describing/
public struct BundleZpkgResolver: ZpkgResolver {
    public let bundle: Bundle
    public let subdirectory: String?
    private let byNamespace: [String: Data]

    public init(bundle: Bundle = .main, subdirectory: String? = "stdlib") {
        self.bundle = bundle
        self.subdirectory = subdirectory
        self.byNamespace = Self.buildMap(bundle: bundle, subdirectory: subdirectory)
    }

    public func resolve(namespace: String) -> Data? {
        byNamespace[namespace]
    }

    /// Enumerate `<subdirectory>/*.zpkg` and read each one's NSPC section
    /// to assemble the namespace → bytes map. First-wins on duplicates;
    /// filenames sorted for deterministic resolution.
    private static func buildMap(bundle: Bundle, subdirectory: String?) -> [String: Data] {
        var map: [String: Data] = [:]
        let urls = bundle.urls(
            forResourcesWithExtension: "zpkg",
            subdirectory: subdirectory
        ) ?? []
        for url in urls.sorted(by: { $0.lastPathComponent < $1.lastPathComponent }) {
            guard let data = try? Data(contentsOf: url) else { continue }
            for ns in zpkgNamespaces(data) where map[ns] == nil {
                map[ns] = data
            }
        }
        return map
    }
}

/// HashMap-backed resolver for tests and for hosts that build the zpkg
/// dictionary at startup (e.g. fetched over network, or a REPL that
/// injects packages on demand).
public final class MapZpkgResolver: ZpkgResolver {
    private var map: [String: Data]

    public init(_ initial: [String: Data] = [:]) {
        self.map = initial
    }

    public func set(_ namespace: String, _ bytes: Data) {
        map[namespace] = bytes
    }

    public func resolve(namespace: String) -> Data? {
        map[namespace]
    }
}

/// Read the namespaces a zpkg provides (its `NSPC` section) via the
/// runtime C ABI `z42_zpkg_read_namespaces`. Returns `[]` if the bytes
/// aren't a parseable zpkg. Lets hosts build a `namespace → bytes` map
/// from packages directly — no index file.
public func zpkgNamespaces(_ bytes: Data) -> [String] {
    var result: [String] = []
    withUnsafeMutablePointer(to: &result) { resultPtr in
        bytes.withUnsafeBytes { (raw: UnsafeRawBufferPointer) in
            let base = raw.bindMemory(to: UInt8.self).baseAddress
            _ = z42_zpkg_read_namespaces(base, bytes.count, { nsPtr, len, userData in
                guard let nsPtr = nsPtr, let userData = userData else { return }
                let u8 = UnsafeRawPointer(nsPtr).assumingMemoryBound(to: UInt8.self)
                let s = String(decoding: UnsafeBufferPointer(start: u8, count: len), as: UTF8.self)
                userData.assumingMemoryBound(to: [String].self).pointee.append(s)
            }, resultPtr)
        }
    }
    return result
}
