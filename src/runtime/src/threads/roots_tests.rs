//! Root-provider parity and managed-value liveness under the VM pause protocol.
use super::*;
use crate::context::VmContext;
use std::sync::Arc;

#[test]
fn categorized_and_collector_roots_preserve_pending_exceptions() {
    let ctx = VmContext::new();
    let first = ctx.heap().alloc_array(vec![Value::I64(11)]);
    let second = ctx.heap().alloc_array(vec![Value::I64(22)]);
    let first_weak = ctx.heap().make_weak(&first).unwrap();
    let second_weak = ctx.heap().make_weak(&second).unwrap();
    *ctx.exec.pending_exception.lock() = Some(first);
    ctx.set_pending_thrown(second);
    let _pause = crate::threads::safepoint::request_gc_pause(&ctx).unwrap();
    let mut collector = Vec::new();
    unsafe { CoreRoots(Arc::downgrade(ctx.core())).visit_roots(&mut |v| collector.push(*v)); }
    let mut classified = Vec::new();
    unsafe {
        scan_categorized_roots(ctx.core(), &mut |v, kind| classified.push((*v, kind)));
    }
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
    assert_eq!(*ctx.exec.pending_exception.lock(), Some(first));
    drop(collector);
    drop(classified);
    assert_eq!(ctx.take_pending_thrown(), Some(second));
    *ctx.exec.pending_exception.lock() = None;
    ctx.heap().collect_full();
    assert!(ctx.heap().upgrade_weak(&first_weak).is_none());
    assert!(ctx.heap().upgrade_weak(&second_weak).is_none());
}
