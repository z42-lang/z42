use super::*;
use crate::exception::VmFrame;
use crate::vm_context::VmContext;

fn frame() -> VmFrame {
    VmFrame::new(std::ptr::null(), std::ptr::null(), std::ptr::null())
}

#[test]
fn push_pop_publish_depth_and_owner() {
    let s = FrameStack::default();
    assert_eq!(s.depth(), 0);
    assert!(!s.owned_by_current_thread(), "an empty stack has no owner");
    s.push(frame());
    s.push(frame());
    assert_eq!((s.depth(), s.published_depth()), (2, 2));
    assert!(s.owned_by_current_thread());
    s.set_top_pc(7);
    s.with_frames(|f| {
        assert_eq!(f[1].pc.get(), 7, "set_top_pc stamps the top frame");
        assert_eq!(f[0].pc.get(), crate::exception::PC_UNSET);
    });
    assert!(s.pop().is_some());
    assert!(s.pop().is_some());
    assert!(s.pop().is_none());
    assert_eq!((s.depth(), s.published_depth()), (0, 0));
}

#[test]
fn regs_at_copies_the_frame_register_pointer() {
    let regs: Vec<crate::metadata::Value> = Vec::new();
    let s = FrameStack::default();
    s.push(VmFrame::new(std::ptr::null(), &regs, std::ptr::null()));
    assert_eq!(s.regs_at(0), Some(&regs as *const _));
    assert_eq!(s.regs_at(1), None);
    s.pop();
}

#[test]
fn another_thread_sees_depth_but_not_ownership() {
    let s = FrameStack::default();
    s.push(frame());
    std::thread::scope(|sc| {
        sc.spawn(|| {
            assert_eq!(s.published_depth(), 1);
            assert!(!s.owned_by_current_thread());
        });
    });
    s.pop();
}

#[test]
fn a_new_activation_chain_takes_ownership() {
    // A context emptied by one thread may be driven by another next (host invoke).
    let s = FrameStack::default();
    std::thread::scope(|sc| {
        sc.spawn(|| {
            s.push(frame());
            s.pop();
        });
    });
    s.push(frame());
    assert!(s.owned_by_current_thread());
    s.pop();
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "inside a NativeParkGuard region")]
fn push_inside_a_native_park_panics_in_debug() {
    let ctx = VmContext::new();
    let _park = crate::gc::safepoint::NativeParkGuard::enter(&ctx);
    ctx.push_frame(frame());
}

#[cfg(debug_assertions)]
#[test]
fn scanning_a_running_thread_outside_a_pause_panics_in_debug() {
    let ctx = VmContext::new();
    ctx.push_frame(frame());
    let ctx_ref: &VmContext = &ctx;
    let r = std::thread::scope(|sc| {
        sc.spawn(|| {
            // SAFETY: deliberately violates the contract; the debug assertion
            // fires before any frame is read.
            unsafe { ctx_ref.scan_frames_parked(|f| f.len()) }
        })
        .join()
    });
    assert!(r.is_err(), "non-owner scan outside a GC pause must trip the assertion");
    ctx.pop_frame();
}

#[test]
fn owner_scan_is_allowed() {
    let ctx = VmContext::new();
    ctx.push_frame(frame());
    // SAFETY: owner thread.
    assert_eq!(unsafe { ctx.scan_frames_parked(|f| f.len()) }, 1);
    ctx.pop_frame();
}
