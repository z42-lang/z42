/// Core library — native function implementations backing the z42 standard library.
///
/// Provider descriptors are assembled in corelib::builtins.
/// Every builtin is reachable through the `BUILTINS` table (`crate::corelib::builtins`): the
/// engines call `vm::exec::call::builtins::invoke_builtin` (interp `Instruction::Builtin`, JIT `jit_builtin`, and
/// the `[Native]` wrapper short-circuit of both — `vm::exec::call::forward`), reading the
/// arguments in place from the caller's registers; callers that hold the arguments as a
/// slice use `vm::exec::call::builtins::exec_builtin`. A builtin is a typed Rust function (builtin ABI v2,
/// `vm::exec::call::builtin_abi`) — arguments and errors are typed by its Rust signature.
///
/// Submodules are organised by functional category (≈ CoreCLR `classlibnative/`):
///   `core`        — values, reflection, threading, diagnostics, and basic builtins
///   `io`          — console, TestIO sinks, filesystem, and shared byte buffers
///   `os`          — processes, platform capabilities, and system queries

mod resource_registry;
pub(crate) mod application;
pub mod core;
pub use self::core::{
    convert, string, math, object, reflection, assemblyloadcontext,
    diagnostics, appprops, array, config, char, gc, bench,
    threading, monitor, crypto,
};
pub(crate) use self::core::symavail;
#[cfg(test)]
mod resource_state_tests;
pub mod io;
pub use io::fs;
pub mod repl;
pub use repl::{editing as repl_editing, native as repl_native};
pub use crate::object::types::named_struct_layout as struct_reflect;
pub mod os;
pub use os::{platform, process, system};
pub mod net;
pub use net as network;
#[cfg(feature = "tls")]
pub mod tls;


#[cfg(test)]
mod tests;
#[cfg(test)]
mod native_decl_tests;   // [Native] declarations ↔ BUILTINS consistency

pub(crate) mod builtins;

pub(crate) fn object_protocol() -> crate::vm::exec::object_protocol::ObjectProtocol {
    crate::vm::exec::object_protocol::ObjectProtocol {
        get_type: crate::corelib::core::object::obj_get_type,
        default_to_string: crate::corelib::core::object::obj_to_str,
        make_type_from_name: crate::corelib::core::reflection::make_type_from_name,
        make_constructed_type: crate::corelib::core::reflection::make_constructed_type,
    }
}
