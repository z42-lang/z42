//! ZpkgResolver trait + C hook adapter for the embedding API.
//!
//! Spec: docs/spec/archive/2026-05-12-add-zpkg-resolver-hook/
//!       docs/internals/src/runtime/embedding.md §11
//!
//! The runtime accepts two resolver shapes:
//!
//! 1. **Rust `Arc<dyn ZpkgResolver>`** — used by Tier 2 (`z42-host`)
//!    clients that already speak Rust. The trait object goes straight
//!    into `HostState::config::zpkg_resolver` (no C round-trip).
//! 2. **C function pointer + user_data** (`Z42ZpkgResolverFn`) — set
//!    via `Z42HostConfig` from a C/JNI/wasm-bindgen client. The
//!    runtime wraps this pair in `CHookResolver` so both paths
//!    converge on the same trait surface.

use std::ffi::CString;
use std::os::raw::c_void;
use std::sync::Arc;

use super::config::Z42ZpkgResolverFn;

/// Resolve a namespace name (as written in z42 source, e.g. `"Std.IO"`
/// or `"z42.core"`) to its zpkg bytes.
///
/// Return `None` to signal "this resolver doesn't know about that
/// namespace"; the runtime continues with the next fallback (typically
/// `search_paths`).
pub trait ZpkgResolver: Send + Sync {
    fn resolve(&self, namespace: &str) -> Option<Vec<u8>>;
}

/// Adapter that turns a C function pointer + opaque `user_data` pair
/// into a [`ZpkgResolver`]. Constructed during `z42_host_initialize`
/// when the caller sets `Z42HostConfig::zpkg_resolver`.
///
/// `user_data` is held as `usize` to inherit `Send + Sync` cleanly — the
/// runtime never dereferences it; only passes it back to the callback.
pub(crate) struct CHookResolver {
    pub(crate) callback: unsafe extern "C" fn(
        *const std::os::raw::c_char,
        *mut *const u8,
        *mut usize,
        *mut c_void,
    ) -> i32,
    pub(crate) user_data: usize,
}

// Function pointer + opaque `usize`: both are trivially Send/Sync;
// thread-safety of the user-supplied callback is the host's contract
// (see docs/internals/src/runtime/embedding.md §7).
unsafe impl Send for CHookResolver {}
unsafe impl Sync for CHookResolver {}

impl ZpkgResolver for CHookResolver {
    fn resolve(&self, namespace: &str) -> Option<Vec<u8>> {
        let cname = CString::new(namespace).ok()?;
        let mut bytes_ptr: *const u8 = std::ptr::null();
        let mut length: usize = 0;
        // SAFETY: the callback signature matches the Rust type by
        // construction (we got it from a typed field in `Z42HostConfig`).
        // The runtime keeps the bytes only for the lifetime of this call.
        let rc = unsafe {
            (self.callback)(
                cname.as_ptr(),
                &mut bytes_ptr,
                &mut length,
                self.user_data as *mut c_void,
            )
        };
        if rc == 0 || bytes_ptr.is_null() || length == 0 {
            return None;
        }
        // SAFETY: contract requires the host to keep these bytes valid
        // until our callback returns. We immediately copy into a `Vec`
        // owned by the runtime; nothing in `bytes_ptr` escapes this
        // function.
        let slice = unsafe { std::slice::from_raw_parts(bytes_ptr, length) };
        Some(slice.to_vec())
    }
}

/// Construct a [`CHookResolver`] `Arc<dyn ZpkgResolver>` from the
/// (optional) C ABI pair carried by `Z42HostConfig`. Returns `None`
/// when the caller didn't supply a resolver.
pub(crate) fn arc_from_c_pair(
    callback: Z42ZpkgResolverFn,
    user_data: *mut c_void,
) -> Option<Arc<dyn ZpkgResolver>> {
    let callback = callback?;
    Some(Arc::new(CHookResolver {
        callback,
        user_data: user_data as usize,
    }))
}
