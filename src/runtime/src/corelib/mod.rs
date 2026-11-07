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
///   `convert`     — parse / try_parse / to_string, boxing; re-exports object value formatting
///   `io`          — console, TestIO sinks, filesystem, and shared byte buffers
///   `string`      — str_length/char_at/substring/split/join/format …
///   `math`        — abs/max/min/pow/sqrt/trig …
///   `object`      — obj_get_type / obj_ref_eq / obj_hash_code
///   `os`          — processes, platform capabilities, and system queries

pub(crate) mod symavail;
mod resource_registry;
pub(crate) mod application;
#[cfg(test)]
mod resource_state_tests;
pub mod convert;
pub mod io;
pub use io::fs;
pub mod repl;
pub use repl::{editing as repl_editing, native as repl_native};
pub mod string;
pub mod math;
pub mod object;
pub mod reflection;
pub use crate::object::types::named_struct_layout as struct_reflect;
pub mod assemblyloadcontext;
pub mod diagnostics;
pub mod appprops;
pub mod array;
pub mod config;
pub mod char;
pub mod gc;
pub mod bench;
pub mod os;
pub use os::{platform, process, system};
pub mod threading;
pub mod monitor;
// add-concurrency-probes (P1b): 用户锁争用探针（`profile-contention` gated，默认零开销）。
// store-sync-values-in-heap: 从 `sync.rs` 内部私有模块上移到这里 —— `monitor.rs` 与（待删的）
// `sync.rs` 都用它。
mod sync_contention;
pub mod net;
pub use net as network;
#[cfg(feature = "tls")]
pub mod tls;
pub mod crypto;


#[cfg(test)]
mod tests;
#[cfg(test)]
mod native_decl_tests;   // [Native] declarations ↔ BUILTINS consistency
