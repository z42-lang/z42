//! VM-owned root enumeration; caller holds a GC pause.
use crate::gc::{to_vm::GcRoots, walk::RootKind};
use crate::object::Value;
use crate::vm::{VmContext, VmCore};
use std::sync::{Arc, Weak};

pub(crate) fn install_vm_roots(core: &Arc<VmCore>) {
    core.heap().set_external_root_scanner(Box::new(CoreRoots(Arc::downgrade(core))));
}

struct CoreRoots(Weak<VmCore>);

impl GcRoots for CoreRoots {
    unsafe fn visit_roots(&self, visit: &mut dyn FnMut(&Value)) {
        // SAFETY: the caller holds the pause for the entire visit.
        unsafe { self.visit_categorized_roots(&mut |value, _| visit(value)); }
    }

    unsafe fn visit_categorized_roots(&self, visit: &mut dyn FnMut(&Value, RootKind)) {
        if let Some(core) = self.0.upgrade() {
            scan_roots(&core, visit);
        }
    }
}
impl GcRoots for VmContext {
    unsafe fn visit_roots(&self, visit: &mut dyn FnMut(&Value)) {
        scan_roots(&self.core, &mut |value, _| visit(value));
    }
}
fn scan_roots(c: &VmCore, visit: &mut (impl FnMut(&Value, RootKind) + ?Sized)) {
    // 1. Shared static fields.
    c.runtime.statics.scan_roots(&mut |value| visit(value, RootKind::StaticField));
    // Interned `ConstStr` literals: after their instruction they live only in
    // the VM's string table (`metadata::ids::str_table`), which keeps them alive.
    c.runtime.strings.scan_roots(|s| visit(&Value::Str(s), RootKind::StaticField));
    let mut visit_stack = |value: &Value| visit(value, RootKind::StackFrame);
    c.threads.visit_contexts(|ctx| {
        ctx.exec().visit_exception_roots(&mut visit_stack);
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
            ctx.exec().scan_frames_parked(ctx.core().threads().collector_active.load(std::sync::atomic::Ordering::Acquire), |frames| {
                crate::exec::stack::visit_roots(frames, &mut visit_stack);
            });
        }
        ctx.exec().scan_stack_alloc_roots(&mut visit_stack);
        ctx.exec().scan_struct_roots(&mut visit_stack);
        // make-value-copy: transient payloads (Ref target / StructRefHeap
        // backing array) hold GcRefs that must stay marked while the handle
        // is live; the arena is the root (handles are not traced through).
        ctx.exec().scan_transient_roots(&mut visit_stack);
    });
}

#[cfg(test)]
#[path = "roots_tests.rs"]
mod tests;
