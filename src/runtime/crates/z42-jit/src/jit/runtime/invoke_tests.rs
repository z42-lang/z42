use super::*;
use crate::exec::{frames::VmFrame, stack::stack_tests::test_function};

unsafe extern "C" fn return_first(frame: *mut JitFrame<'_>, _: *const VmContext) -> u8 {
    // SAFETY: enter_compiled passes a live frame with at least one register.
    let frame = unsafe { &mut *frame };
    frame.ret = unsafe { *frame.regs_ptr };
    frame.has_ret = 1;
    0
}

unsafe extern "C" fn throw_first(frame: *mut JitFrame<'_>, vm: *const VmContext) -> u8 {
    // SAFETY: enter_compiled passes a live context and a frame with a first register.
    let (frame, vm) = unsafe { (&*frame, &*vm) };
    vm.exec().set_exception(unsafe { *frame.regs_ptr });
    1
}

#[test]
fn osr_registers_are_recycled_after_return_and_throw() {
    let vm = VmContext::new();
    let function = test_function("osr", &[], Vec::new());
    let parent_regs = vec![Value::I64(42)];
    crate::exec::push_frame(&vm, VmFrame::new(&function, &parent_regs));

    let cases: [(JitFn, bool); 2] = [(return_first, false), (throw_first, true)];
    for (entry, throws) in cases {
        let frame = JitFrame::from_interp_regs(&vm, &parent_regs, 3);
        let buffer = frame.regs.as_ptr();
        assert_ne!(buffer, parent_regs.as_ptr());
        assert_eq!(frame.regs, [Value::I64(42), Value::Null, Value::Null, Value::Null]);
        // SAFETY: the test entry has the JitFn ABI, and the frame, function and
        // context remain live on the driving thread throughout the call.
        let outcome = unsafe { enter_compiled(&vm, entry as *const u8, &function, frame) };
        if throws {
            assert!(matches!(outcome, NativeOutcome::Threw));
            assert_eq!(vm.exec().take_exception(), Some(Value::I64(42)));
        } else {
            assert!(matches!(outcome, NativeOutcome::Returned(Some(Value::I64(42)))));
        }
        assert_eq!(vm.exec().frame_stack_depth(), 1);
        assert_eq!(parent_regs, [Value::I64(42)]);

        let recycled = vm.exec().take_registers(4);
        assert_eq!(recycled.as_ptr(), buffer);
        assert_eq!(recycled, [Value::Null; 4]);
        vm.exec().recycle_registers(recycled);
    }

    vm.exec().pop_frame();
}
