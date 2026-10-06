//! `EngineGuards` — the thread-local VM / heap pointers a context installs while
//! it has frames on its stack.
//!
//! Two thread-locals must name the running VM while z42 code runs:
//! `gc::ambient`'s heap (heap-less `Str::new` / `.into()` allocate from it) and,
//! with `native-interop`, `native::exports`' `CURRENT_VM` (`z42_*` exports find
//! the VM through it). Both stay the same for a whole activation chain, so they
//! are installed when the **bottom** frame is pushed (`VmContext::push_frame`,
//! frame-stack depth 0 → 1) and restored when the stack is empty again
//! (`pop_frame`). A nested call costs no thread-local access.
//!
//! Every other engine entry on the same thread and context — reflection invoke,
//! static constructors, `ToString` dispatch, the REPL, JIT ↔ interp diverts —
//! runs above that bottom frame, under the guards already installed. The one
//! way another context's code can run on this thread while this context has
//! frames is a host re-entry (`z42_host_invoke` from a native callback), and
//! `host::ops::invoke_impl` installs both guards itself, unconditionally. Each
//! guard restores what it found, so the nesting unwinds in order.
//!
//! Owner-thread only, like the frame stack (`frame_stack.rs`).

use std::cell::UnsafeCell;

use crate::gc::ambient::HeapGuard;

use super::VmContext;

struct Installed {
    // Field order = drop order: restore `CURRENT_VM`, then the heap.
    #[cfg(feature = "native-interop")]
    _vm: crate::native::exports::VmGuard<'static>,
    _heap: HeapGuard,
}

/// See the module docs.
#[derive(Default)]
pub(crate) struct EngineGuards {
    installed: UnsafeCell<Option<Installed>>,
}

// SAFETY: `installed` is touched only by the owner thread, at the bottom push
// and the last pop (module docs). Dropping a context that still has frames on
// another thread would restore that thread's slots — the same as dropping a
// leaked RAII guard there; no context is dropped with live frames.
unsafe impl Sync for EngineGuards {}
unsafe impl Send for EngineGuards {}

impl EngineGuards {
    /// Owner only, at the bottom frame's push.
    #[cold]
    #[inline(never)]
    pub(crate) fn install(&self, ctx: &VmContext) {
        // SAFETY: the context outlives its guards — they are dropped at the
        // last pop or, at the latest, with the context itself.
        #[cfg(feature = "native-interop")]
        let ctx_static: &'static VmContext = unsafe { &*(ctx as *const VmContext) };
        let installed = Installed {
            #[cfg(feature = "native-interop")]
            _vm: crate::native::exports::VmGuard::enter(ctx_static),
            _heap: HeapGuard::enter(ctx.heap()),
        };
        // SAFETY: owner-thread access.
        let slot = unsafe { &mut *self.installed.get() };
        debug_assert!(slot.is_none(), "engine guards installed twice");
        *slot = Some(installed);
    }

    /// Owner only, once the frame stack is empty again. Restores what
    /// [`install`](Self::install) replaced.
    #[cold]
    #[inline(never)]
    pub(crate) fn release(&self) {
        // SAFETY: owner-thread access; the guards drop after the borrow ends.
        let installed = unsafe { &mut *self.installed.get() }.take();
        drop(installed);
    }
}

#[cfg(test)]
#[path = "engine_guards_tests.rs"]
mod engine_guards_tests;
