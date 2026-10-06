//! `FrameStack` — a context's chain of active script frames (`VmFrame`s),
//! accessed by its owner thread without locks.
//!
//! # Who may touch it
//!
//! - **The owner thread** — the thread running z42 code through this
//!   `VmContext` — pushes, pops, stamps the top frame's `pc` and reads frames
//!   (stack traces, the sampler, the stack-overflow report, `RefKind::Stack`
//!   deref). These are plain `Vec` operations; nothing is locked.
//! - **Any other thread** reads the frames only through
//!   [`FrameStack::scan_parked`], and only while the owner is parked (a GC
//!   stop-the-world window or handshake). That is how the GC root scanners
//!   reach other threads' frame registers.
//! - **The crash-signal handler** reads the frames of the thread it runs on
//!   (it *is* the owner), and for every other context only the
//!   [`published_depth`](FrameStack::published_depth) atomic.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::exception::VmFrame;
use crate::metadata::Value;

/// See the module docs.
pub(crate) struct FrameStack {
    frames: UnsafeCell<Vec<VmFrame>>,
    /// `frames.len()`, republished after every push / pop for readers that may
    /// not touch `frames` (the signal handler, for other threads' contexts).
    depth: AtomicUsize,
    /// [`thread_token`] of the thread that pushed the bottom frame, i.e. the
    /// owner whenever `depth > 0`. Written before the `Release` store of
    /// `depth = 1`, so a reader that sees `depth > 0` with `Acquire` sees the
    /// matching owner.
    owner: AtomicUsize,
}

// SAFETY: `frames` is written only by the owner thread, and read by another
// thread only through `scan_parked`, whose contract is that the owner is
// parked — see there for the happens-before argument. The two atomics are
// safe to share by themselves.
unsafe impl Sync for FrameStack {}

impl Default for FrameStack {
    fn default() -> Self {
        Self { frames: UnsafeCell::new(Vec::new()), depth: AtomicUsize::new(0), owner: AtomicUsize::new(0) }
    }
}

impl FrameStack {
    /// Owner only. Push one frame; `true` when it is the bottom frame (the
    /// stack was empty).
    #[inline]
    pub(crate) fn push(&self, frame: VmFrame) -> bool {
        crate::gc::safepoint::debug_assert_frame_change_not_parked();
        // SAFETY: owner-thread access (module docs); no reference into
        // `frames` outlives this call.
        let frames = unsafe { &mut *self.frames.get() };
        frames.push(frame);
        let len = frames.len();
        if len == 1 {
            // A new activation chain starts here: this thread owns the stack
            // until it is empty again.
            self.owner.store(thread_token(), Ordering::Relaxed);
            self.depth.store(len, Ordering::Release);
            true
        } else {
            self.debug_assert_owner();
            self.depth.store(len, Ordering::Relaxed);
            false
        }
    }

    /// Owner only. Pop the top frame (`None` when empty).
    #[inline]
    pub(crate) fn pop(&self) -> Option<VmFrame> {
        crate::gc::safepoint::debug_assert_frame_change_not_parked();
        self.debug_assert_owner();
        // SAFETY: owner-thread access.
        let frames = unsafe { &mut *self.frames.get() };
        let f = frames.pop();
        self.depth.store(frames.len(), Ordering::Relaxed);
        f
    }

    /// Owner only. Stamp the top frame's code offset (no-op when empty).
    #[inline]
    pub(crate) fn set_top_pc(&self, pc: u32) {
        // SAFETY: owner-thread access.
        if let Some(top) = unsafe { &*self.frames.get() }.last() {
            top.pc.set(pc);
        }
    }

    /// Owner only. Number of frames.
    #[inline]
    pub(crate) fn depth(&self) -> usize {
        // SAFETY: owner-thread access.
        unsafe { &*self.frames.get() }.len()
    }

    /// Owner only. The register file of frame `idx` (0 = outermost).
    #[inline]
    pub(crate) fn regs_at(&self, idx: usize) -> Option<*const Vec<Value>> {
        // SAFETY: owner-thread access; the pointer is copied out.
        unsafe { &*self.frames.get() }.get(idx).map(|f| f.regs)
    }

    /// Owner only. Run `f` over the frames, outermost first. `f` must not
    /// push or pop frames on this context (it would move the slice it reads).
    #[inline]
    pub(crate) fn with_frames<R>(&self, f: impl FnOnce(&[VmFrame]) -> R) -> R {
        self.debug_assert_owner();
        // SAFETY: owner-thread access; the borrow ends when `f` returns.
        f(unsafe { &*self.frames.get() })
    }

    /// Read the frames from a thread that may not be the owner.
    ///
    /// # Safety
    ///
    /// The caller is the owner thread, or the owner is parked for the whole
    /// call — the GC root scanners, which run while the collector holds the
    /// stop-the-world pause. The park protocol orders the owner's last push /
    /// pop before the scan and the scan before its next one:
    ///
    /// - a mutator parks with `parked_count.fetch_add(AcqRel)` and only then
    ///   takes the `gc_phase` lock (`park_until_idle`, `native_park_incr` in
    ///   `gc/safepoint.rs`); it never pushes / pops while counted as parked
    ///   (debug-asserted for the native park);
    /// - the collector reads `parked_count` with `Acquire` and switches to
    ///   `Marking` only once every other context is counted (`request_gc_pause`).
    ///   `parked_count` only changes by RMW, so that load synchronizes with
    ///   every park increment before it;
    /// - a parked mutator resumes only after the collector set `Idle` under the
    ///   `gc_phase` lock, and decrements under that lock (`park_until_idle`,
    ///   `native_park_decr`);
    /// - a context cannot register while the phase is `Marking`
    ///   (`VmContext::new_with_core`), so the scanner never meets an unparked
    ///   newcomer.
    ///
    /// `f` must not keep the slice or anything borrowed from it.
    #[inline]
    pub(crate) unsafe fn scan_parked<R>(&self, f: impl FnOnce(&[VmFrame]) -> R) -> R {
        // SAFETY: the caller's contract above.
        f(unsafe { &*self.frames.get() })
    }

    /// Frame count as last published by the owner. Readable from any thread
    /// (including a signal handler); from a non-owner it may be stale.
    #[inline]
    pub(crate) fn published_depth(&self) -> usize {
        self.depth.load(Ordering::Acquire)
    }

    /// Whether the calling thread owns a non-empty stack. Async-signal-safe.
    #[inline]
    pub(crate) fn owned_by_current_thread(&self) -> bool {
        self.published_depth() > 0 && self.owner.load(Ordering::Relaxed) == thread_token()
    }

    #[inline]
    fn debug_assert_owner(&self) {
        #[cfg(debug_assertions)]
        {
            let depth = self.depth.load(Ordering::Relaxed);
            let owner = self.owner.load(Ordering::Relaxed);
            debug_assert!(
                depth == 0 || owner == thread_token(),
                "VmContext frame stack touched by a thread other than its owner"
            );
        }
    }
}

/// A process-unique id of the calling thread. On unix it is `pthread_self()`
/// — a register read, safe in a signal handler (unlike a `thread_local!`,
/// whose first access may allocate).
#[cfg(unix)]
#[inline]
pub(crate) fn thread_token() -> usize {
    // SAFETY: `pthread_self` has no preconditions.
    unsafe { libc::pthread_self() as usize }
}

#[cfg(not(unix))]
#[inline]
pub(crate) fn thread_token() -> usize {
    thread_local! { static TOKEN: u8 = const { 0 }; }
    TOKEN.with(|t| t as *const u8 as usize)
}

#[cfg(test)]
#[path = "frame_stack_tests.rs"]
mod frame_stack_tests;
