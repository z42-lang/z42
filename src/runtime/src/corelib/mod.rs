/// Core library — native function implementations backing the z42 standard library.
///
/// Every builtin is reachable through the `BUILTINS` table (`builtin_table.rs`): the
/// engines call [`invoke_builtin`] (interp `Instruction::Builtin`, JIT `jit_builtin`, and
/// the `[Native]` wrapper short-circuit of both — `interp::forward`), reading the
/// arguments in place from the caller's registers; callers that hold the arguments as a
/// slice use [`exec_builtin`]. A builtin is a typed Rust function (builtin ABI v2,
/// [`abi`]) — arguments and errors are typed by its Rust signature.
///
/// Submodules are organised by functional category (≈ CoreCLR `classlibnative/`):
///   `convert`     — value_to_str, parse / try_parse / to_string, boxing
///   `io`          — println, print, readline, TestIO sinks
///   `string`      — str_length/char_at/substring/split/join/format …
///   `math`        — abs/max/min/pow/sqrt/trig …
///   `fs`          — file_* / path_* / env_* / process_exit / time_now_ms
///   `object`      — obj_get_type / obj_ref_eq / obj_hash_code

mod symavail;
pub mod abi;
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
pub mod tls;
pub mod crypto;

use crate::metadata::tokens::BuiltinId;
use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Function pointer type of an **ext** builtin（dlopen / bundled 的 native 扩展 ABI，
/// `native::ext`）——不是 builtin 表的形态，表里只有 ABI v2 的类型化描述符
/// （`abi::BuiltinDesc`）。
pub type NativeFn = fn(&VmContext, &[Value]) -> Result<Value>;

// BUILTINS 表见 `builtin_table.rs`（纯数据，编译期拼接；下标即 BuiltinId，不跨进程持久化）。
pub(crate) use builtin_table::BUILTINS;

/// Lazy-built `name → BuiltinId` index for `exec_builtin(name, args)` and the
/// resolver's `builtin_id_of` lookup. Built once on first access from
/// `BUILTINS` (single source of truth).
static BUILTIN_INDEX: OnceLock<HashMap<&'static str, u32>> = OnceLock::new();

fn builtin_index() -> &'static HashMap<&'static str, u32> {
    BUILTIN_INDEX.get_or_init(|| {
        BUILTINS.iter().enumerate()
            .map(|(i, d)| (d.name, i as u32))
            .collect()
    })
}

/// High bit of `BuiltinId.0` marks an ext (dlopen / bundled) builtin
/// resolved through `VmCore.ext_builtins` rather than the static
/// `BUILTINS` slice. Low 31 bits are the index into the ext table.
/// add-z42-compression .
pub const BUILTIN_ID_EXT_BIT: u32 = 0x8000_0000;

/// Resolve a builtin name to its `BuiltinId`. Static `BUILTINS[]` first;
/// callers should fall back to [`ext_builtin_id_of`] if this returns
/// `None` (the resolver needs a `VmContext` for the ext table).
pub fn builtin_id_of(name: &str) -> Option<BuiltinId> {
    builtin_index().get(name).copied().map(BuiltinId)
}

/// Resolve a builtin name via the per-VM extension table. A miss loads the
/// ext library that provides `name` (first use only, `native::ext::ensure_lib_for`)
/// and looks again. Returns a `BuiltinId` whose high bit is set; dispatch
/// routes it through `ext_builtins.dispatch`. Only available when the
/// `native-interop` feature is enabled.
#[cfg(feature = "native-interop")]
pub fn ext_builtin_id_of(ctx: &VmContext, name: &str) -> Option<BuiltinId> {
    let lookup = || ctx.core.ext_builtins.lock().lookup_id(name)
        .map(|idx| BuiltinId(idx | BUILTIN_ID_EXT_BIT));
    lookup().or_else(|| {
        if crate::native::ext::ensure_lib_for(ctx, name) { lookup() } else { None }
    })
}

/// Name-keyed entry for callers that hold the arguments as a slice — unit tests,
/// embedders, and the interpreter's `ToString` dispatch fallback
/// (`exec_builtin_value`); the engines use [`invoke_builtin`]. Static `BUILTINS[]`
/// first; ext (dlopened) second. A miss in both throws `Std.MissingSymbolException`
/// (as an undefined function does).
pub fn exec_builtin(ctx: &VmContext, name: &str, args: &[Value]) -> Result<Option<Value>> {
    // add-runtime-counters : name-keyed slow path also increments
    // for consistency with invoke_builtin_id (callers may hit either).
    ctx.counters().builtin_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    if let Some(&id) = builtin_index().get(name) {
        return (BUILTINS[id as usize].call_slice)(ctx, args).map_err(|e| (*e).into_anyhow(ctx));
    }
    #[cfg(feature = "native-interop")]
    if let Some(id) = ext_builtin_id_of(ctx, name) {
        let fn_ptr = ctx.core.ext_builtins.lock().dispatch(id.0 & !BUILTIN_ID_EXT_BIT);
        if let Some(fn_ptr) = fn_ptr {
            // The table lock is released before invoking — wrappers may re-enter.
            return fn_ptr(ctx, args).map(Some);
        }
    }
    Err(unknown_builtin(name).into_anyhow(ctx))
}

/// An unresolvable builtin name at run time — a stdlib built against another VM.
#[cold]
fn unknown_builtin(name: &str) -> abi::BuiltinError {
    abi::BuiltinError::Exception {
        class: "Std.MissingSymbolException",
        message: crate::probing::with_sdk_missing_hint(format!("unknown builtin `{name}`")),
    }
}

/// The engines' builtin call (interp `Builtin`, JIT `jit_builtin`): the arguments are
/// read straight from the caller's register file — `this` first when the receiver is
/// not in a register (vcall), then `regs[args[i]]`; a typed builtin reads them in place.
/// `Ok(None)` = void.
///
/// `id` = the site's resolved `BuiltinId`, or `UNRESOLVED` to resolve `name` now
/// (which re-checks the ext registry; a miss throws `MissingSymbolException`).
#[inline]
pub(crate) fn invoke_builtin(
    ctx: &VmContext, id: u32, name: &str,
    this: Option<Value>, regs: &[Value], args: &[u32],
) -> abi::CallResult {
    if id == crate::metadata::tokens::UNRESOLVED {
        return invoke_by_name(ctx, name, this, regs, args);
    }
    invoke_builtin_id(ctx, id, this, regs, args)
}

/// [`invoke_builtin`] for a resolved id (the `[Native]` forward short-circuit always has one).
#[inline(always)]
pub(crate) fn invoke_builtin_id(
    ctx: &VmContext, id: u32, this: Option<Value>, regs: &[Value], args: &[u32],
) -> abi::CallResult {
    ctx.counters().builtin_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    #[cfg(feature = "native-interop")]
    if id & BUILTIN_ID_EXT_BIT != 0 {
        return invoke_ext(ctx, id & !BUILTIN_ID_EXT_BIT, this, regs, args);
    }
    let idx = id as usize;
    debug_assert!(idx < BUILTINS.len(), "BuiltinId {id} out of range");
    (BUILTINS[idx].call_regs)(ctx, this, regs, args)
}

/// An ext (dlopen'd / bundled) builtin's `anyhow` result into the call's result
/// (the error path boxes, cold).
#[inline(always)]
fn legacy(r: Result<Option<Value>>) -> abi::CallResult {
    match r {
        Ok(v) => Ok(v),
        Err(e) => Err(legacy_fault(e)),
    }
}

#[cold]
#[inline(never)]
fn legacy_fault(e: anyhow::Error) -> abi::Fault {
    Box::new(abi::BuiltinError::Legacy(e))
}

/// An ext (dlopen'd / bundled) builtin by its ext-table index.
#[cfg(feature = "native-interop")]
#[inline(never)]
fn invoke_ext(ctx: &VmContext, idx: u32, this: Option<Value>, regs: &[Value], args: &[u32]) -> abi::CallResult {
    let fn_ptr = ctx.core.ext_builtins.lock().dispatch(idx)
        .ok_or_else(|| Box::new(abi::BuiltinError::Internal(format!("ext builtin id {idx} out of range"))))?;
    // The table lock is released before invoking — wrappers may re-enter.
    legacy(with_reg_args(this, regs, args, |a| fn_ptr(ctx, a)).map(Some))
}

/// [`invoke_builtin`] by name: the static table, then the ext registry, else
/// `MissingSymbolException`.
#[cold]
#[inline(never)]
fn invoke_by_name(
    ctx: &VmContext, name: &str, this: Option<Value>, regs: &[Value], args: &[u32],
) -> abi::CallResult {
    if let Some(&i) = builtin_index().get(name) {
        return invoke_builtin_id(ctx, i, this, regs, args);
    }
    #[cfg(feature = "native-interop")]
    if let Some(id) = ext_builtin_id_of(ctx, name) {
        return invoke_builtin_id(ctx, id.0, this, regs, args);
    }
    let _ = (ctx, this, regs, args);
    Err(Box::new(unknown_builtin(name)))
}

/// Ext builtins take `&[Value]`: copy the arguments out into a stack buffer
/// (≤ 6, the common case — a heap `Vec` beyond) and run `f` on it.
///
/// Copies straight from the registers (`regs[r]`, a register index is in range by
/// construction) into a buffer that never moves: going through `Option<Value>` made the
/// compiler rebuild each value from its tag byte plus an overlapping payload copy, and
/// a `SmallVec` was moved again before the call — the builtin's reload of its arguments
/// then missed store forwarding (+25% per folded ext call in a string hashing loop).
#[inline(always)]
fn with_reg_args<R>(this: Option<Value>, regs: &[Value], args: &[u32], f: impl FnOnce(&[Value]) -> R) -> R {
    const INLINE: usize = 6;
    let n = args.len() + this.is_some() as usize;
    if n <= INLINE {
        let mut buf = [Value::Null; INLINE];
        let mut k = 0;
        if let Some(t) = &this { buf[0] = *t; k = 1; }
        for &r in args {
            buf[k] = regs[r as usize];
            k += 1;
        }
        f(&buf[..n])
    } else {
        let mut v = Vec::with_capacity(n);
        if let Some(t) = &this { v.push(*t); }
        v.extend(args.iter().map(|&r| regs[r as usize]));
        f(&v)
    }
}

/// Whether a folded call of builtin `id`'s `[Native]` wrapper may skip the wrapper's
/// frame entirely (`interp::forward`): the builtin never runs z42 code — a typed one
/// without `reenters`. Decided once per wrapper, when its forward is computed.
pub(crate) fn folds_frameless(id: u32) -> bool {
    BUILTINS.get(id as usize).is_some_and(|d| !d.flags.reenters)
}

/// The name of builtin `id` (for an internal error's report).
pub(crate) fn builtin_name(id: u32, fallback: &str) -> &str {
    BUILTINS.get(id as usize).map_or(fallback, |d| d.name)
}

/// 给**明确期待有返回值**的调用方（`__obj_to_str` 这类）：撞上 void builtin 就报错，
/// 而不是静默给一个占位值。
///
/// split-null-sentinel-channels ④：此前这些调用方拿到的是 `Value`，void 与「返回 null」
/// 无从分辨；现在 `None` 会变成一条明确的错误，说明表与调用方对这个 builtin 的
/// 「有没有返回值」认知不一致。
pub fn exec_builtin_value(ctx: &VmContext, name: &str, args: &[Value]) -> Result<Value> {
    exec_builtin(ctx, name, args)?.ok_or_else(|| {
        anyhow::anyhow!("builtin `{name}` is declared void but its result was used as a value")
    })
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod native_decl_tests;   // [Native] declarations ↔ BUILTINS consistency
