//! `#[unsafe(no_mangle)]` `z42_*` ABI entry points called by native libraries.
//!
//! These functions are the C-side surface of Tier 1: native code dlopens
//! the z42 host (or links statically against the test binary) and calls
//! these to register types and invoke z42 methods.
//!
//! The thread-local `CURRENT_VM` slot carries the active `VmContext` while
//! the interpreter is running; native callbacks fired from inside z42
//! find the VM through this slot. See [`VmGuard`] for the RAII wrapper.

use std::cell::Cell;
use std::ffi::CStr;
use std::marker::PhantomData;
use std::os::raw::c_char;
use std::ptr;
use std::sync::Arc;

use z42_abi::{Z42Error, Z42TypeDescriptor_v1, Z42TypeOpaque, Z42TypeRef, Z42Value};

use super::error;
use super::registry::RegisteredType;
use crate::vm_context::VmContext;

/// Categorical error codes mirrored into the last-error slot for embedder
/// consumption via `z42_last_error()`. 2026-05-11 retire-z-codes: named
/// `Z####` constants retired, but the numeric values stay for ABI compat
/// — embedder hosts may still pattern-match on them.
const TYPE_REGISTRATION_FAILURE: u32 = 905;
const ABI_VERSION_MISMATCH:      u32 = 906;

// ── thread_local current-VM pointer ──────────────────────────────────────

thread_local! {
    /// `*const VmContext` for the currently executing z42 interpreter on
    /// this thread. Set on entry to `interp::exec_function` via
    /// [`VmGuard`], cleared on exit (including unwind).
    static CURRENT_VM: Cell<*const VmContext> = const { Cell::new(ptr::null()) };
}

/// RAII guard that scopes a `VmContext` reference into `CURRENT_VM` for
/// the lifetime of `'a`. Used by `interp::exec_function` so any native
/// callback fired during interpretation can locate the VM.
pub struct VmGuard<'a> {
    prev: *const VmContext,
    /// `false` when `enter` found `CURRENT_VM` already this same ctx (a nested
    /// interp frame under the same VM/thread): store skipped, drop skips its TLS
    /// restore. Only the outermost frame per ctx has `active == true`.
    active: bool,
    _phantom: PhantomData<&'a VmContext>,
}

impl<'a> VmGuard<'a> {
    #[inline]
    pub fn enter(ctx: &'a VmContext) -> Self {
        // Per-frame guard, but the active VM is constant across a call tree. When a
        // nested frame re-enters with the SAME ctx, skip both the store and the
        // drop-time restore — saving a TLS (`_tlv_get_addr` on macOS) access per
        // nested frame. A different ctx (cross-VM native re-entry) still saves+sets+
        // restores as before, so callback location stays correct.
        let p = ctx as *const _;
        CURRENT_VM.with(|c| {
            let cur = c.get();
            if cur == p {
                VmGuard { prev: cur, active: false, _phantom: PhantomData }
            } else {
                c.set(p);
                VmGuard { prev: cur, active: true, _phantom: PhantomData }
            }
        })
    }
}

impl Drop for VmGuard<'_> {
    #[inline]
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        CURRENT_VM.with(|c| c.set(self.prev));
    }
}

/// Borrow the VM pointed to by `CURRENT_VM`. Returns `None` if no VM is
/// currently active on this thread.
fn current_vm<'a>() -> Option<&'a VmContext> {
    CURRENT_VM.with(|c| {
        let p = c.get();
        if p.is_null() { None } else { Some(unsafe { &*p }) }
    })
}

// ── Z42TypeRef sentinels ────────────────────────────────────────────────

const NULL_TYPE_REF: Z42TypeRef = Z42TypeRef(ptr::null_mut());

/// `Z42TypeRef` is a stable handle. We treat the raw pointer as a key into
/// a per-VM type table; for C2 we just leak `Rc::into_raw(ty)` so the
/// pointer stays alive as long as the VM holds the `Rc` clone.
fn type_ref_from_rc(ty: &Arc<RegisteredType>) -> Z42TypeRef {
    let ptr = Arc::as_ptr(ty) as *mut Z42TypeOpaque;
    Z42TypeRef(ptr)
}

// ── z42_register_type ────────────────────────────────────────────────────

/// Register a native type with the currently active VM. Returns a non-NULL
/// [`Z42TypeRef`] on success or `NULL` on failure (consult
/// [`z42_last_error`]).
///
/// # Safety
/// Caller must guarantee `desc` is a valid pointer to a
/// `Z42TypeDescriptor_v1` whose strings live for the VM's lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_register_type(
    desc: *const Z42TypeDescriptor_v1,
) -> Z42TypeRef {
    error::clear();

    let Some(vm) = current_vm() else {
        error::set(TYPE_REGISTRATION_FAILURE, "z42_register_type called outside an active z42 VM context");
        return NULL_TYPE_REF;
    };

    let registered = match unsafe { RegisteredType::from_descriptor(desc) } {
        Ok(r) => r,
        Err(e) => {
            // 2026-05-11 retire-z-codes: messages no longer carry `Z####:`
            // prefixes; recover the categorical code from the human-readable
            // text instead (only ABI mismatch carries a distinct surface
            // marker — everything else falls under TYPE_REGISTRATION_FAILURE).
            let msg = format!("{e:#}");
            let code = if msg.contains("ABI version mismatch") {
                ABI_VERSION_MISMATCH
            } else {
                TYPE_REGISTRATION_FAILURE
            };
            error::set(code, msg);
            return NULL_TYPE_REF;
        }
    };

    let arc = Arc::new(registered);
    if !vm.register_native_type(arc.clone()) {
        error::set(
            TYPE_REGISTRATION_FAILURE,
            format!(
                "duplicate native type {}::{}",
                arc.module(),
                arc.type_name()
            ),
        );
        return NULL_TYPE_REF;
    }

    type_ref_from_rc(&arc)
}

// ── z42_resolve_type ─────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_resolve_type(
    module: *const c_char,
    type_name: *const c_char,
) -> Z42TypeRef {
    error::clear();
    let Some(vm) = current_vm() else { return NULL_TYPE_REF; };
    if module.is_null() || type_name.is_null() { return NULL_TYPE_REF; }

    let m = match unsafe { CStr::from_ptr(module) }.to_str() {
        Ok(s) => s,
        Err(_) => return NULL_TYPE_REF,
    };
    let n = match unsafe { CStr::from_ptr(type_name) }.to_str() {
        Ok(s) => s,
        Err(_) => return NULL_TYPE_REF,
    };

    match vm.resolve_native_type(m, n) {
        Some(rc) => type_ref_from_rc(&rc),
        None => NULL_TYPE_REF,
    }
}

// ── z42_invoke (C2 stub: full method dispatch lands when used) ──────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_invoke(
    _ty: Z42TypeRef,
    _method: *const c_char,
    _args: *const Z42Value,
    _arg_count: usize,
) -> Z42Value {
    // C2 only validates the registry path; reverse-call (native → z42)
    // dispatch is wired in C5 when the source generator emits the calls.
    error::set(TYPE_REGISTRATION_FAILURE, "z42_invoke not implemented in C2 (lands in spec C5)");
    super::dispatch::z42_null()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_invoke_method(
    _receiver: Z42Value,
    _method: *const c_char,
    _args: *const Z42Value,
    _arg_count: usize,
) -> Z42Value {
    error::set(TYPE_REGISTRATION_FAILURE, "z42_invoke_method not implemented in C2 (lands in spec C5)");
    super::dispatch::z42_null()
}

// ── z42_last_error ───────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn z42_last_error() -> Z42Error {
    error::last()
}

// ── z42_set_panic_message (used by z42-rs shim panic recovery) ──────────

/// Forward a panic message from a Rust `extern "C"` shim into the VM's
/// thread-local last-error slot. Called from the `catch_unwind` recovery
/// branch emitted by `#[z42::methods]`.
///
/// # Safety
/// `msg` must be either NULL or a NUL-terminated string valid for the
/// duration of this call. The function copies the message; the caller
/// retains ownership of `msg`.
#[unsafe(no_mangle)]
pub extern "C" fn z42_set_panic_message(msg: *const c_char) {
    if msg.is_null() {
        error::set(TYPE_REGISTRATION_FAILURE, "native shim panic (no message)");
        return;
    }
    let owned = unsafe { CStr::from_ptr(msg) }
        .to_string_lossy()
        .into_owned();
    error::set(TYPE_REGISTRATION_FAILURE, format!("native shim panic: {owned}"));
}
