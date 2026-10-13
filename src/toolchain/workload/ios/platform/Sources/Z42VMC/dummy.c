/* Empty source file. SwiftPM requires at least one .c (or .m) file in a
 * regular target; the actual symbols come from the Rust staticlib linked
 * at xcframework-creation time (build.sh). The clang module map exposes
 * z42_abi.h + z42_host.h to Swift; this file just keeps SwiftPM happy.
 *
 * Spec: 变更 add-platform-ios/
 */
