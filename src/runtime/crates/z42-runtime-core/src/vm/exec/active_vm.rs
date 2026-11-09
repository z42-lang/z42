//! Thread-local active VM context for native callbacks.
//! Bottom-frame entry and host re-entry install the same guard and restore the
//! previous context when their activation ends.

use std::cell::Cell;
use std::marker::PhantomData;
use std::ptr;
use crate::vm::context::VmContext;

// ── thread_local current-VM pointer ──────────────────────────────────────

thread_local! {
    /// `*const VmContext` for the VM currently running z42 code on this
    /// thread. Set via [`VmGuard`] when a context pushes its bottom frame
    /// (`vm/exec/call/engine_guards.rs`) and on host invoke; restored when that
    /// activation ends.
    static CURRENT_VM: Cell<*const VmContext> = const { Cell::new(ptr::null()) };
}

/// RAII guard that scopes a `VmContext` reference into `CURRENT_VM` for
/// the lifetime of `'a`, so any native callback fired while the VM runs can
/// locate it. Installed at engine entry only (see `CURRENT_VM`).
pub struct VmGuard<'a> {
    prev: *const VmContext,
    /// `false` when `enter` found `CURRENT_VM` already this same ctx (a nested
    /// entry under the same VM/thread): store skipped, drop skips its TLS
    /// restore. Only the outermost entry per ctx has `active == true`.
    active: bool,
    _phantom: PhantomData<&'a VmContext>,
}

impl<'a> VmGuard<'a> {
    #[inline]
    pub fn enter(ctx: &'a VmContext) -> Self {
        // Engine entries nest (host invoke → bottom frame). When the SAME ctx is
        // already installed, skip both the store and the drop-time restore. A
        // different ctx (cross-VM re-entry) saves + sets + restores, so callback
        // location stays correct.
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
pub fn current_vm<'a>() -> Option<&'a VmContext> {
    CURRENT_VM.with(|c| {
        let p = c.get();
        if p.is_null() { None } else { Some(unsafe { &*p }) }
    })
}

#[cfg(all(test, z42_test_facade))]
#[path = "active_vm_tests.rs"]
mod active_vm_tests;
