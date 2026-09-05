/// Core library — native function implementations backing the z42 standard library.
///
/// All functions are reachable via a single stable entry point `exec_builtin(name, args)`
/// which is called by:
///   • the interpreter  (`Instruction::Builtin` in interp/mod.rs)
///   • the JIT backend  (`jit_builtin` extern "C" helper in jit/helpers.rs)
///
/// Submodules are organised by functional category (≈ CoreCLR `classlibnative/`):
///   `convert`     — value_to_str, require_str/usize, parse/to_str
///   `io`          — println, print, readline, concat, len, contains
///   `string`      — str_substring/contains/split/join/format …
///   `math`        — abs/max/min/pow/sqrt/trig …
///   `fs`          — file_* / path_* / env_* / process_exit / time_now_ms
///   `object`      — obj_get_type / obj_ref_eq / obj_hash_code / assert_*
///
/// 2026-04-26 script-first-stringbuilder: removed `string_builder` module —
/// `Std.Text.StringBuilder` is now a pure z42 script in `z42.text`,
/// backed by `List<string>` + `String.FromChars` (no VM intrinsic needed).
///
/// 2026-04-26 extern-audit-wave0: removed `collections` module (13 builtins)
/// — `Std.Collections.List<T>` / `Dictionary<K,V>` are pure z42 scripts atop
/// `T[]`; compiler stopped emitting `__list_*` / `__dict_*` after L3-G4h step3.
///
/// 2026-04-27 wave1-assert-script: removed 6 `__assert_*` builtins —
/// `Std.Assert` methods are now pure z42 scripts (`if (!cond) throw new
/// Exception(...)`), matching BCL `Debug.Assert` / Rust `assert!`.

pub mod convert;
pub mod io;
pub mod repl;
pub mod repl_editing;
// Lazy dlopen loader for the host-only REPL editor cdylib (libz42_repl). Host-only:
// wasm keeps the plain-stdin fallback in `repl` and never references this module.
// (extract-repl-native-cdylib)
#[cfg(not(target_arch = "wasm32"))]
pub mod repl_native;
pub mod string;
pub mod str_meta;
pub mod math;
pub mod fs;
pub mod fs_backend;   // add-wasm-vfs-backend: 平台隔离 fs 后端（native / memory VFS）
pub mod object;
pub mod reflection;
pub mod struct_reflect;   // add-boxed-struct-identity (P4b): value-struct field layout replication
pub mod assemblyloadcontext;
pub mod diagnostics;
pub mod appprops;
pub mod array;
mod builtin_table;
mod builtin_table_ext;
pub mod config;
pub mod char;
pub mod gc;
pub mod bench;
pub mod process;
pub mod platform;
pub mod system;
pub mod threading;
pub mod sync;
pub mod network;
pub mod tls;
pub mod crypto;

use crate::metadata::tokens::BuiltinId;
use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Function pointer type for all native builtins.
///
/// Carries `&VmContext` so builtins can access the GC heap (e.g. allocate
/// `Std.Type` objects via `ctx.heap().alloc_object(...)`) and other runtime
/// state. **2026-04-29 extend-native-fn-signature** added `&VmContext` —
/// previously `fn(&[Value]) -> Result<Value>`, which forced corelib allocation
/// callsites to bypass the heap interface.
pub type NativeFn = fn(&VmContext, &[Value]) -> Result<Value>;

// BUILTINS 表见 `builtin_table.rs`（纯数据；只可表尾追加，下标即 BuiltinId）。
pub(crate) use builtin_table::BUILTINS;

// runtime-dynamic-load-call stubs (DEFERRED): registered so zpkgs that declare
// [Native("__load_zpkg")] / [Native("__call_static")] load cleanly; calls fail
// at runtime until the reflection MVP is complete.
fn builtin_load_zpkg_stub(_ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    anyhow::bail!("__load_zpkg: not yet implemented (runtime-dynamic-load-call DEFERRED)")
}
fn builtin_call_static_stub(_ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    anyhow::bail!("__call_static: not yet implemented (runtime-dynamic-load-call DEFERRED)")
}

/// Lazy-built `name → BuiltinId` index for `exec_builtin(name, args)` and the
/// resolver's `builtin_id_of` lookup. Built once on first access from
/// `BUILTINS` (single source of truth).
static BUILTIN_INDEX: OnceLock<HashMap<&'static str, u32>> = OnceLock::new();

fn builtin_index() -> &'static HashMap<&'static str, u32> {
    BUILTIN_INDEX.get_or_init(|| {
        BUILTINS.iter().enumerate()
            .map(|(i, (name, _))| (*name, i as u32))
            .collect()
    })
}

/// High bit of `BuiltinId.0` marks an ext (dlopen / bundled) builtin
/// resolved through `VmCore.ext_builtins` rather than the static
/// `BUILTINS` slice. Low 31 bits are the index into the ext table.
/// add-z42-compression (2026-05-22).
pub const BUILTIN_ID_EXT_BIT: u32 = 0x8000_0000;

/// Resolve a builtin name to its `BuiltinId`. Static `BUILTINS[]` first;
/// callers should fall back to [`ext_builtin_id_of`] if this returns
/// `None` (the resolver needs a `VmContext` for the ext table).
pub fn builtin_id_of(name: &str) -> Option<BuiltinId> {
    builtin_index().get(name).copied().map(BuiltinId)
}

/// Resolve a builtin name via the per-VM extension table populated at
/// VM startup by `native::ext::load_all`. Returns a `BuiltinId` whose
/// high bit is set; dispatch routes it through `ext_builtins.dispatch`.
/// Only available when the `native-interop` feature is enabled.
#[cfg(feature = "native-interop")]
pub fn ext_builtin_id_of(ctx: &VmContext, name: &str) -> Option<BuiltinId> {
    ctx.core.ext_builtins.lock().lookup_id(name)
        .map(|idx| BuiltinId(idx | BUILTIN_ID_EXT_BIT))
}

/// Fast-path dispatch by id. Static ids index into `BUILTINS`; ids with
/// the ext bit set index into `VmCore.ext_builtins.by_idx`.
#[inline]
pub fn exec_builtin_by_id(ctx: &VmContext, id: BuiltinId, args: &[Value]) -> Result<Value> {
    // add-runtime-counters (2026-05-26): observation-only fetch_add on
    // the hot path — single atomic Relaxed op, no control-flow impact.
    ctx.core.counters.builtin_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    #[cfg(feature = "native-interop")]
    if id.0 & BUILTIN_ID_EXT_BIT != 0 {
        let idx = id.0 & !BUILTIN_ID_EXT_BIT;
        let fn_ptr = {
            let ext = ctx.core.ext_builtins.lock();
            ext.dispatch(idx)
                .ok_or_else(|| anyhow::anyhow!("ext builtin id {} out of range", idx))?
        };
        return fn_ptr(ctx, args);
    }
    let idx = id.0 as usize;
    debug_assert!(idx < BUILTINS.len(), "BuiltinId {} out of range", id.0);
    BUILTINS[idx].1(ctx, args)
}

/// Stable public entry point — called by the interpreter and JIT `jit_builtin`.
/// Static `BUILTINS[]` first; ext (dlopened) second. A miss in both is a
/// hard error.
pub fn exec_builtin(ctx: &VmContext, name: &str, args: &[Value]) -> Result<Value> {
    // add-runtime-counters (2026-05-26): name-keyed slow path also increments
    // for consistency with exec_builtin_by_id (callers may hit either).
    ctx.core.counters.builtin_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    if let Some(&id) = builtin_index().get(name) {
        return BUILTINS[id as usize].1(ctx, args);
    }
    #[cfg(feature = "native-interop")]
    {
        let ext = ctx.core.ext_builtins.lock();
        if let Some(idx) = ext.lookup_id(name) {
            if let Some(fn_ptr) = ext.dispatch(idx) {
                drop(ext);  // release before invoking — wrappers may re-enter
                return fn_ptr(ctx, args);
            }
        }
    }
    Err(anyhow::anyhow!("unknown builtin `{name}`"))
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod native_decl_tests;   // [Native] declarations ↔ BUILTINS consistency
