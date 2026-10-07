use crate::exception::VmFrame;
use crate::gc::ambient::current_heap_epoch;
use crate::vm_context::VmContext;

fn frame() -> VmFrame {
    VmFrame::new(std::ptr::null(), std::ptr::null(), std::ptr::null())
}

#[cfg(feature = "native-interop")]
fn current_vm_ptr() -> *const VmContext {
    crate::native::exports::current_vm().map_or(std::ptr::null(), |v| v as *const _)
}

#[test]
fn bottom_frame_scopes_the_context_until_the_stack_is_empty() {
    let ctx = VmContext::new();
    let epoch = ctx.heap().heap_epoch();
    assert_eq!(current_heap_epoch(), 0, "no engine entry yet");

    ctx.push_frame(frame());
    assert_eq!(current_heap_epoch(), epoch, "the bottom push installs the heap");
    #[cfg(feature = "native-interop")]
    assert_eq!(current_vm_ptr(), &*ctx as *const VmContext);

    ctx.push_frame(frame());
    ctx.pop_frame();
    assert_eq!(current_heap_epoch(), epoch, "a nested pop leaves the guards in place");

    ctx.pop_frame();
    assert_eq!(current_heap_epoch(), 0, "the last pop restores the thread-locals");
    #[cfg(feature = "native-interop")]
    assert!(current_vm_ptr().is_null());
}

#[test]
fn a_second_context_on_the_same_thread_nests_and_restores() {
    let a = VmContext::new();
    let b = VmContext::new();
    a.push_frame(frame());
    b.push_frame(frame());
    assert_eq!(current_heap_epoch(), b.heap().heap_epoch());
    #[cfg(feature = "native-interop")]
    assert_eq!(current_vm_ptr(), &*b as *const VmContext);
    b.pop_frame();
    assert_eq!(current_heap_epoch(), a.heap().heap_epoch(), "b's last pop restores a's");
    #[cfg(feature = "native-interop")]
    assert_eq!(current_vm_ptr(), &*a as *const VmContext);
    a.pop_frame();
    assert_eq!(current_heap_epoch(), 0);
}

#[test]
fn frame_ids_are_never_zero() {
    let ctx = VmContext::new();
    ctx.next_frame_id.store(u32::MAX, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(ctx.next_frame_id(), u32::MAX);
    assert_eq!(ctx.next_frame_id(), 1, "wraps past 0, the \"not taken\" marker");
}
