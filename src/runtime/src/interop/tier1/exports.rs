//! `#[unsafe(no_mangle)]` `z42_*` ABI entry points called by native libraries.
//!
//! These functions are the C-side surface of Tier 1: native code dlopens
//! the z42 host (or links statically against the test binary) and calls
//! these to register and resolve native types.
//!
//! Native callbacks find the active context through VM execution's
//! `active_vm` slot. [`VmGuard`] re-exports its activation guard.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::ptr;
use std::sync::Arc;

use z42_abi::{Z42Error, Z42TypeDescriptor_v1, Z42TypeOpaque, Z42TypeRef};

use super::error;
use super::registry::RegisteredType;

/// Categorical error codes mirrored into the last-error slot for embedder
/// consumption via `z42_last_error()`.  retire-z-codes: named
/// `Z####` constants retired, but the numeric values stay for ABI compat
/// — embedder hosts may still pattern-match on them.
const TYPE_REGISTRATION_FAILURE: u32 = 905;
const ABI_VERSION_MISMATCH:      u32 = 906;

pub use crate::exec::active_vm::VmGuard;
pub(crate) use crate::exec::active_vm::current_vm;

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
            // retire-z-codes: messages no longer carry `Z####:`
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
    let inserted = match super::super::state::register_native_type(vm, arc.clone()) {
        Ok(inserted) => inserted,
        Err(e) => {
            error::set(TYPE_REGISTRATION_FAILURE, format!("{e:#}"));
            return NULL_TYPE_REF;
        }
    };
    if !inserted {
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

    match super::super::state::resolve_native_type(vm, m, n) {
        Ok(Some(rc)) => type_ref_from_rc(&rc),
        Ok(None) => NULL_TYPE_REF,
        Err(e) => {
            error::set(TYPE_REGISTRATION_FAILURE, format!("{e:#}"));
            NULL_TYPE_REF
        }
    }
}

// ── z42_last_error ───────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn z42_last_error() -> Z42Error {
    error::last()
}

// ── z42_set_panic_message (used by z42-rs shim panic recovery) ──────────

/// Store a shim panic message in the VM's per-thread last-error slot.
/// Both the C entry below and the `z42-abi` panic hook installed at boot
/// ([`install_panic_hook`]) land here; see `z42-rs`'s `native_helpers` for
/// why the Rust side reaches this through a hook pointer instead of the
/// link-time symbol.
fn forward_panic_message(msg: &str) {
    error::set(TYPE_REGISTRATION_FAILURE, format!("native shim panic: {msg}"));
}

/// Install the `z42-abi` panic hook pointing at [`forward_panic_message`].
/// Idempotent (first install wins). Called from native registry init and
/// host initialization so any image that runs native shims also owns the
/// forwarding path.
pub(crate) fn install_panic_hook() {
    z42_abi::panic_hook::install_panic_hook(forward_panic_message);
}

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
    forward_panic_message(&owned);
}
