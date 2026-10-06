import Foundation

/// Errors surfaced by every `Z42VM` API. Each case carries the runtime's
/// last-error message and the numeric `Z42HostStatus` code so callers
/// can branch precisely.
///
/// Spec: docs/spec/archive/2026-05-12-add-platform-ios/
public enum Z42VMError: Error, CustomStringConvertible {
    case alreadyInit(String)
    case notInit(String)
    case badConfig(String)
    case featureOff(String)
    case badZbc(String)
    case verification(String)
    case entryNotFound(String)
    case argMismatch(String)
    case vmException(String)
    /// Fatal VM error (stack overflow); shut the VM down.
    case fatal(String)
    case `internal`(String)

    /// Numeric `Z42HostStatus` value (1..99).
    public var status: Int32 {
        switch self {
        case .alreadyInit:    return 1
        case .notInit:        return 2
        case .badConfig:      return 3
        case .featureOff:     return 4
        case .badZbc:         return 10
        case .verification:   return 11
        case .entryNotFound:  return 20
        case .argMismatch:    return 21
        case .vmException:    return 30
        case .fatal:          return 31
        case .internal:       return 99
        }
    }

    public var message: String {
        switch self {
        case let .alreadyInit(m), let .notInit(m), let .badConfig(m),
             let .featureOff(m), let .badZbc(m), let .verification(m),
             let .entryNotFound(m), let .argMismatch(m),
             let .vmException(m), let .fatal(m), let .internal(m):
            return m
        }
    }

    public var description: String {
        "Z42VMError(\(status)): \(message)"
    }

    /// Construct from a runtime `Z42HostStatus` + message string.
    static func from(status: Int32, message: String) -> Z42VMError {
        switch status {
        case 1:  return .alreadyInit(message)
        case 2:  return .notInit(message)
        case 3:  return .badConfig(message)
        case 4:  return .featureOff(message)
        case 10: return .badZbc(message)
        case 11: return .verification(message)
        case 20: return .entryNotFound(message)
        case 21: return .argMismatch(message)
        case 30: return .vmException(message)
        case 31: return .fatal(message)
        default: return .internal(message)
        }
    }
}
