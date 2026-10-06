//! `RegPool` — a context's free-list of register-file `Vec`s, shared by the
//! interpreter (`interp::Frame`) and the JIT (`JitFrame`).
//!
//! # Invariant
//!
//! Every pooled `Vec` has length 0. [`RegPool::take`] grows it with
//! `resize(need, Null)` — exactly one `Null` write per slot per call, which the
//! GC needs: a frame's `regs[0..len)` are scanned as roots as soon as the frame
//! is pushed, so every slot must hold a valid value by then. [`RegPool::give`]
//! only `clear()`s (`Value` is `Copy`, so that is a length store). Slots past
//! `len` may keep stale bits; nothing reads them — a pooled `Vec` is not a root.
//!
//! # Who may touch it
//!
//! The thread running z42 code through this `VmContext`, like the frame stack
//! (`frame_stack.rs`): no lock, no thread-local. Engine entries on one context
//! never overlap across threads (a VM thread has its own context; a host drives
//! a context from one thread at a time).

use std::cell::UnsafeCell;

use crate::metadata::Value;

/// Max `Vec`s retained. Caps idle memory after a deep-then-shallow recursion;
/// excess `Vec`s are freed.
const CAP: usize = 512;

/// See the module docs.
#[derive(Default)]
pub(crate) struct RegPool {
    free: UnsafeCell<Vec<Vec<Value>>>,
}

// SAFETY: only the thread currently running z42 code on the owning context
// touches `free` (module docs); there is no cross-thread reader.
unsafe impl Sync for RegPool {}

impl RegPool {
    /// Owner only. A register file of `need` `Null` slots.
    #[inline]
    pub(crate) fn take(&self, need: usize) -> Vec<Value> {
        // SAFETY: owner-thread access; the borrow ends here.
        let mut regs = unsafe { &mut *self.free.get() }.pop().unwrap_or_default();
        debug_assert!(regs.is_empty(), "pooled register file not cleared");
        regs.resize(need, Value::Null);
        regs
    }

    /// Owner only. Return a register file once no frame points at it any more.
    #[inline]
    pub(crate) fn give(&self, mut regs: Vec<Value>) {
        if regs.capacity() == 0 {
            return;
        }
        regs.clear();
        // SAFETY: owner-thread access; the borrow ends here.
        let free = unsafe { &mut *self.free.get() };
        if free.len() < CAP {
            free.push(regs);
        }
    }
}

#[cfg(test)]
#[path = "reg_pool_tests.rs"]
mod reg_pool_tests;
