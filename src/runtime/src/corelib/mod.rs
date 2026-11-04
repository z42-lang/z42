/// Core library — native function implementations backing the z42 standard library.
///
/// The facade assembles providers in builtin_assembly.
/// Every builtin is reachable through the `BUILTINS` table (`crate::builtin_assembly`): the
/// engines call `vm::exec::call::builtins::invoke_builtin` (interp `Instruction::Builtin`, JIT `jit_builtin`, and
/// the `[Native]` wrapper short-circuit of both — `vm::exec::call::forward`), reading the
/// arguments in place from the caller's registers; callers that hold the arguments as a
/// slice use `vm::exec::call::builtins::exec_builtin`. A builtin is a typed Rust function (builtin ABI v2,
/// `vm::exec::call::builtin_abi`) — arguments and errors are typed by its Rust signature.
///
/// Submodules are organised by functional category (≈ CoreCLR `classlibnative/`):
///   `convert`     — value_to_str, parse / try_parse / to_string, boxing
///   `io`          — println, print, readline, TestIO sinks
///   `string`      — str_length/char_at/substring/split/join/format …
///   `math`        — abs/max/min/pow/sqrt/trig …
///   `fs`          — file_* / path_* / env_* / process_exit / time_now_ms
///   `object`      — obj_get_type / obj_ref_eq / obj_hash_code

pub(crate) mod symavail;
mod resource_registry;
pub(crate) mod application;
#[cfg(test)]
mod resource_state_tests;
pub mod convert;
pub mod io;
pub mod repl;
pub mod repl_editing;
// Lazy dlopen loader for the REPL editor cdylib (libz42_repl); without `native-interop`
// (wasm) it is the plain-stdin fallback in `repl`. (extract-repl-native-cdylib)
pub mod repl_native;
pub mod string;
pub mod math;
pub mod fs;
pub mod object;
pub mod reflection;
pub mod struct_reflect;   // add-boxed-struct-identity (P4b): value-struct field layout replication
pub mod assemblyloadcontext;
pub mod diagnostics;
pub mod appprops;
pub mod array;
pub mod config;
pub mod char;
pub mod gc;
pub mod bench;
pub mod process;
pub mod platform;
pub mod system;
pub mod threading;
pub mod monitor;
// add-concurrency-probes (P1b): 用户锁争用探针（`profile-contention` gated，默认零开销）。
// store-sync-values-in-heap: 从 `sync.rs` 内部私有模块上移到这里 —— `monitor.rs` 与（待删的）
// `sync.rs` 都用它。
mod sync_contention;
pub mod network;
#[cfg(feature = "tls")]
pub mod tls;
pub mod crypto;


#[cfg(test)]
mod tests;
#[cfg(test)]
mod native_decl_tests;   // [Native] declarations ↔ BUILTINS consistency
