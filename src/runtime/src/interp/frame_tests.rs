use super::*;
use std::sync::atomic::Ordering::Relaxed;

#[test]
fn frame_id_is_taken_on_first_use_only() {
    let ctx = VmContext::new();
    let before = ctx.next_frame_id.load(Relaxed);
    let frame = Frame::new(&ctx, &[], 4);
    assert_eq!(frame.frame_id_if_taken(), 0, "a fresh frame has no id");
    assert_eq!(ctx.next_frame_id.load(Relaxed), before, "building a frame takes no id");

    let id = frame.frame_id(&ctx);
    assert_ne!(id, 0);
    assert_eq!(frame.frame_id(&ctx), id, "the id is stable");
    assert_eq!(frame.frame_id_if_taken(), id);
    assert_eq!(ctx.next_frame_id.load(Relaxed), before.wrapping_add(1), "one id taken");
}

#[test]
fn frames_draw_registers_from_the_context_pool() {
    let ctx = VmContext::new();
    let mut frame = Frame::new(&ctx, &[Value::I64(1), Value::I64(2)], 6);
    assert_eq!(frame.regs.len(), 6);
    assert!(matches!(frame.regs[1], Value::I64(2)));
    assert!(frame.regs[2..].iter().all(|v| matches!(v, Value::Null)));
    frame.regs[5] = Value::Bool(true);
    let ptr = frame.regs.as_ptr();
    ctx.reg_pool.give(std::mem::take(&mut frame.regs));

    let again = Frame::new(&ctx, &[], 6);
    assert_eq!(again.regs.as_ptr(), ptr, "the returned file is reused");
    assert!(again.regs.iter().all(|v| matches!(v, Value::Null)), "and comes back all Null");
}
