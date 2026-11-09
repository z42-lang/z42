//! VM-owned root enumeration; caller holds a GC pause.
use crate::gc::to_vm::GcRoots;
use crate::object::Value;
use crate::vm::context::{VmContext, VmCore};
use std::sync::Weak;

pub struct CoreRoots(pub Weak<VmCore>);
impl GcRoots for CoreRoots {
    unsafe fn visit_roots(&self, visit: &mut dyn FnMut(&Value)) {
        if let Some(core) = self.0.upgrade() {
            scan_roots(&core, visit);
        }
    }
}
impl GcRoots for VmContext {
    unsafe fn visit_roots(&self, visit: &mut dyn FnMut(&Value)) {
        scan_roots(&self.core, visit);
    }
}
fn scan_roots(c: &VmCore, visit: &mut dyn FnMut(&Value)) {
    // 1. Shared static fields.
    c.runtime.statics.scan_roots(visit);
    // Interned `ConstStr` literals: after their instruction they live only in
    // the VM's string table (`vm::runtime::ids::str_table`), which keeps them alive.
    c.runtime.strings.scan_roots(|s| visit(&Value::Str(s)));
    // 2-4. Per-thread roots, one VmContext per OS thread.
    //
    // SAFETY: each VmContextPtr was registered via
    // `VmContext::new()` *after* its `Pin<Box<...>>` heap
    // allocation, and `VmContext::drop` removes the entry
    // BEFORE the Box is dealloc'd. We hold `vm_contexts.lock()`
    // for the full walk, so a concurrent drop on another thread
    // blocks until we release — no use-after-free possible.
    let registry = c.threads.vm_contexts.lock();
    for ctx_ptr in registry.iter() {
        let ctx = unsafe { &*ctx_ptr.0 };
        // pending_exception / pending_thrown
        if let Some(v) = ctx.exec.pending_exception.lock().as_ref() {
            visit(v);
        }
        if let Some(v) = ctx.exec.pending_thrown.lock().as_ref() {
            visit(v);
        }
        // live z42 frame state — unified VmFrame entries.
        //
        // SAFETY (frame stack): the scanner runs only inside a GC
        // pause (`request_gc_pause` → `Marking`), where every
        // other registered context is parked and this one is the
        // collector's own — `scan_frames_parked`'s contract.
        // SAFETY (frame.regs): raw ptr valid for the
        // lifetime of the owning Rust frame (FrameGuard RAII for
        // interp; `enter_compiled` for JIT), which outlives the park.
        unsafe {
            ctx.scan_frames_parked(|frames| {
                crate::vm::exec::stack::visit_roots(frames, &mut *visit);
            });
        }
        // add-escape-analysis-stack-alloc: stack-alloc arena roots.
        // A stack object's slots / stack array's elements may hold heap
        // GcRefs that must stay marked while the stack object is live.
        // (The stack objects themselves are not GC-heap entries and are
        // freed by frame-exit truncation, not sweep.) The arena lock is
        // never held across a GC trigger, so this cannot deadlock.
        ctx.exec.stack_arena.lock().scan_roots(visit);
        // add-struct-value-semantics: value-struct blobs' reference
        // leaves are GC roots too (no-op for pure-primitive structs;
        // ref-leaf scanning by type ref-bitmap lands in A-use).
        ctx.exec.struct_arena.lock().scan_roots(visit);
        // make-value-copy: transient payloads (Ref target / StructRefHeap
        // backing array) hold GcRefs that must stay marked while the handle
        // is live; the arena is the root (handles are not traced through).
        ctx.exec.transient_arena.lock().scan_roots(visit);
    }
}

#[cfg(all(test, z42_test_facade))]
use crate::assembly::context::VmContextFactory as _;
