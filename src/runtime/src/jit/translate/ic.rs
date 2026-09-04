//! Inline-cache / site-cache token lookups (method_id / vcall / field / static ids).
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).

use super::*;

/// formalize-jit-method-token Phase 2.C helper: look up the resolved
/// `MethodId.0` for a `Call` site. Returns `UNRESOLVED` (= u32::MAX)
/// for cross-zpkg lazy targets — `jit_call` falls back to name lookup.
pub(super) fn method_id_at(func: &Function, block_idx: usize, instr_idx: usize) -> u32 {
    func.resolved.get()
        .and_then(|r| {
            let site = *r.site_index.get(block_idx)?.get(instr_idx)?;
            r.method_tokens.get(site as usize)
        })
        .map(|atom| atom.load(std::sync::atomic::Ordering::Relaxed))
        .unwrap_or(crate::metadata::tokens::UNRESOLVED)
}

/// make-vm-loading-lazy helper: stable raw pointer to the per-Call-site
/// `call_jit_ic` slot (an `AtomicU32` caching the resolved lazy/merged fn id)
/// for the `Call` site at `(block_idx, instr_idx)`. Returns null when
/// `Function.resolved` is unset (jit_call degrades to the by-name slow path).
/// The IC lives inside `Function.resolved` (inside Module) → valid for the
/// whole JitModule lifetime, like `vcall_ic_ptr_at`.
pub(super) fn call_jit_ic_ptr_at(func: &Function, block_idx: usize, instr_idx: usize) -> *const std::sync::atomic::AtomicU32 {
    func.resolved.get()
        .and_then(|r| {
            let site = *r.site_index.get(block_idx)?.get(instr_idx)?;
            r.call_jit_ic.get(site as usize)
        })
        .map(|ic| ic as *const _)
        .unwrap_or(std::ptr::null())
}

/// formalize-jit-method-token Phase 2.E helper: stable raw pointer to
/// the `VCallIC` slot for the VCall site at `(block_idx, instr_idx)`.
/// Returns null when `Function.resolved` is unset (helper degrades to
/// non-IC slow path). The IC lives inside `Function.resolved.vcall_ic`
/// (which lives inside Module), so the pointer is valid for the entire
/// JitModule lifetime.
pub(super) fn vcall_ic_ptr_at(func: &Function, block_idx: usize, instr_idx: usize) -> *const crate::metadata::resolver::VCallIC {
    func.resolved.get()
        .and_then(|r| {
            let site = *r.site_index.get(block_idx)?.get(instr_idx)?;
            r.vcall_ic.get(site as usize)
        })
        .map(|ic| ic as *const _)
        .unwrap_or(std::ptr::null())
}

/// formalize-jit-method-token Phase 2.E helper: stable raw pointer to
/// the `FieldIC` slot for a FieldGet/FieldSet site. Same lifetime
/// guarantees as `vcall_ic_ptr_at`.
pub(super) fn field_ic_ptr_at(func: &Function, block_idx: usize, instr_idx: usize) -> *const crate::metadata::resolver::FieldIC {
    func.resolved.get()
        .and_then(|r| {
            let site = *r.site_index.get(block_idx)?.get(instr_idx)?;
            r.field_ic.get(site as usize)
        })
        .map(|ic| ic as *const _)
        .unwrap_or(std::ptr::null())
}

/// **cache-ctorless-objnew**: stable raw pointer to the per-`ObjNew`-site
/// "this class has no constructor" mark. Same lifetime guarantees as
/// `field_ic_ptr_at` (the slot lives in `Function.resolved`, a write-once
/// `OnceLock`, so the address is stable for the module's life). Null when the
/// function was compiled without a resolved token table — the helper then
/// simply always re-resolves.
pub(super) fn ctorless_mark_ptr_at(func: &Function, block_idx: usize, instr_idx: usize) -> *const std::sync::atomic::AtomicUsize {
    func.resolved.get()
        .and_then(|r| {
            let site = *r.site_index.get(block_idx)?.get(instr_idx)?;
            r.ctorless_marks.get(site as usize)
        })
        .map(|m| m as *const _)
        .unwrap_or(std::ptr::null())
}

/// formalize-jit-method-token Phase 2 helper: look up the resolved
/// `StaticFieldId.0` for a `StaticGet` / `StaticSet` site at
/// `(block_idx, instr_idx)`, or `UNRESOLVED` when `Function.resolved` is unset.
///
/// make-vm-loading-lazy: a lazily-loaded function is JIT-compiled WITHOUT its
/// `resolved` token table (it never went through `resolve_module` — resolving
/// its method/type tokens against the lazy module would bind them to the wrong
/// indices; interp likewise leaves lazy functions unresolved). So this now
/// degrades to `UNRESOLVED` instead of panicking, and `jit_static_get/set`
/// resolve the field by NAME at runtime — mirroring interp's `exec_object`
/// `field_id: None` fallback. Static field ids are ctx-global (allocated by
/// name), so the by-name path is correct regardless of which module owns the fn.
pub(super) fn static_field_id_at(func: &Function, block_idx: usize, instr_idx: usize) -> u32 {
    func.resolved.get()
        .and_then(|r| {
            let site = *r.site_index.get(block_idx)?.get(instr_idx)?;
            r.static_field_tokens.get(site as usize)
        })
        .map(|atom| atom.load(std::sync::atomic::Ordering::Relaxed))
        .filter(|&id| id != crate::metadata::tokens::UNRESOLVED)
        .unwrap_or(crate::metadata::tokens::UNRESOLVED)
}
