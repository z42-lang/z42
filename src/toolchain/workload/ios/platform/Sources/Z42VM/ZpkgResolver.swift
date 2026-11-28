import Foundation
import Z42VMC

/// Supply bytes for a named Z42 program, dependency, or runtimeconfig.
public protocol FileResolver {
    func resolve(fileName: String, kind: UInt32) throws -> Data?
}

public struct BundleZpkgResolver: FileResolver {
    public let bundle: Bundle
    public let subdirectory: String?

    public init(bundle: Bundle = .main, subdirectory: String? = "stdlib") {
        self.bundle = bundle
        self.subdirectory = subdirectory
    }

    public func resolve(fileName: String, kind: UInt32) throws -> Data? {
        let fromApp = kind == UInt32(Z42_FILE_RUNTIME_CONFIG) || kind == UInt32(Z42_FILE_ZBC_DEPS)
        let directory = fromApp ? nil : subdirectory
        guard let url = bundle.url(forResource: fileName, withExtension: nil, subdirectory: directory) else {
            return nil
        }
        return try Data(contentsOf: url)
    }
}

public final class MapZpkgResolver: FileResolver {
    private let lock = NSLock()
    private var map: [String: Data]

    public init(_ files: [String: Data] = [:]) {
        self.map = files
    }

    public func set(_ name: String, _ bytes: Data) {
        lock.lock()
        defer { lock.unlock() }
        map[name] = bytes
    }

    public func resolve(fileName: String, kind: UInt32) -> Data? {
        lock.lock()
        defer { lock.unlock() }
        return map[fileName]
    }
}
