//! Root-provider parity and managed-value liveness under the VM pause protocol.
use super::*;
use crate::vm::VmContext;

#[test]
fn categorized_and_collector_roots_preserve_pending_exceptions() {
    let ctx = VmContext::new();
    let first = ctx.heap().alloc_array(vec![Value::I64(11)]);
    let second = ctx.heap().alloc_array(vec![Value::I64(22)]);
    let first_weak = ctx.heap().make_weak(&first).unwrap();
    let second_weak = ctx.heap().make_weak(&second).unwrap();
    ctx.exec().set_exception(first);
    ctx.exec().set_pending_thrown(second);
    let pause = crate::threads::safepoint::request_gc_pause(&ctx).unwrap();
    let mut collector = Vec::new();
    unsafe { ctx.visit_roots(&mut |v| collector.push(*v)); }
    let mut classified = Vec::new();
    pause.inspect_heap().roots(&mut |v, kind| classified.push((*v, kind)));
    assert!(collector.contains(&first));
    assert!(collector.contains(&second));
    assert_eq!(collector, classified.iter().map(|(v, _)| *v).collect::<Vec<_>>());
    assert!(classified.contains(&(first, RootKind::StackFrame)));
    assert!(classified.contains(&(second, RootKind::StackFrame)));

    // Marking uses the unclassified provider. The categorized view must describe
    // the same objects that survive a real collection.
    ctx.heap().collect_full();
    assert!(ctx.heap().upgrade_weak(&first_weak).is_some());
    assert!(ctx.heap().upgrade_weak(&second_weak).is_some());
    assert_eq!(ctx.exec().peek_exception(), Some(first));
    drop(collector);
    drop(classified);
    assert_eq!(ctx.exec().take_pending_thrown(), Some(second));
    assert_eq!(ctx.exec().take_exception(), Some(first));
    ctx.heap().collect_full();
    assert!(ctx.heap().upgrade_weak(&first_weak).is_none());
    assert!(ctx.heap().upgrade_weak(&second_weak).is_none());
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn paused_heap_inspection_reads_a_parked_workers_register_roots() {
    use crate::exec::frames::VmFrame;
    use crate::threads::safepoint::{request_gc_pause, NativeParkGuard};
    use std::sync::mpsc;

    let ctx = VmContext::new();
    std::thread::scope(|scope| {
        let (parked_tx, parked_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel::<()>();
        let core = ctx.core_arc();
        scope.spawn(move || {
            let worker = VmContext::new_with_core(core);
            let value = worker.heap().alloc_array(vec![Value::I64(42)]);
            let regs = vec![value];
            crate::exec::push_frame(&worker, VmFrame::new(std::ptr::null(), &regs));
            let parked = NativeParkGuard::enter(&worker);
            parked_tx.send(value).unwrap();
            let _ = resume_rx.recv();
            drop(parked);
            worker.exec().pop_frame();
        });

        let worker_root = parked_rx.recv().unwrap();
        let pause = request_gc_pause(&ctx).unwrap();
        ctx.heap().collect_full();
        let mut found = false;
        pause.inspect_heap().roots(&mut |value, kind| {
            found |= *value == worker_root && kind == RootKind::StackFrame;
        });
        assert!(found);
        drop(pause);
        drop(resume_tx);
    });
}
