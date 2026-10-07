//! fix-jit-array-data-stackarray: unit tests for the JIT array-data fast-path helpers.
//!
//! 驱动 `extern "C"` helper 打一个最小 `JitModuleCtx`（module 指针悬空——本组 helper
//! 只碰 `vm_ctx.stack_arena`，不碰 module），手法同 `struct_ops_tests.rs`。
//!
//! **为什么不用 e2e golden**：触发这个缺陷要三者同时成立——数组不逃逸（栈分配）、
//! 元素是基元（JIT 走内联快路）、循环够热到 **OSR** 在解释器已建好 StackArray 之后接手。
//! 第三条在 golden 里不可靠：解释器起跑不计数 tier-up（见 `add-jit-tierup-counting`，
//! 旋钮默认关），实测写出来的 e2e 用例在**有 bug 的 VM 下照样全绿**——等于没测。
//! 直接打 helper 才是确定性的。

use super::*;
use super::super::super::frame::{JitFrame, JitModuleCtx};
use crate::metadata::types::{ArrayObj, Value};
use crate::vm_context::VmContext;

/// 最小 JIT ctx：只有 `vm_ctx` 是活的（module 悬空）。
fn make_jit_ctx(vm_ctx: &VmContext) -> JitModuleCtx {
    let mut c = JitModuleCtx::new(std::ptr::null(), std::ptr::null(), 1, 10_000);
    c.vm_ctx = vm_ctx as *const VmContext as *mut VmContext;
    c
}

/// 在 ctx 的栈 arena 里放一个 `int[]`，返回对应的 `Value::StackArray` 句柄。
fn stack_int_array(vm: &VmContext, frame_id: u32, elems: Vec<i64>) -> Value {
    let arr = ArrayObj::stack_typed(crate::metadata::types::ElemType::intern("int"), elems.into_iter().map(Value::I64).collect());
    let idx = vm.stack_alloc_arr(frame_id, arr);
    Value::StackArray { idx, frame_id }
}

fn data(frame: &mut JitFrame, ctx: &JitModuleCtx) -> (*const Value, i64, i64) {
    let mut ptr: *const Value = 1usize as *const Value;   // 预置非 null，确保 helper 确实写了
    let mut len: i64 = -1;
    let mut width: i64 = -1;
    unsafe { jit_array_data_opt(frame, ctx, 0, &mut ptr, &mut len, &mut width) };
    (ptr, len, width)
}

/// 回归：栈上数组进打包快路必须**报「无快路」而非抛异常**（width 0 ⇒ 内联回落 `jit_array_get`，
/// 由慢路按 arena 解析）。实测曾表现为「解释器能跑、一 tier-up 就崩」：
/// `ArrayGet: expected array, got StackArray`。
#[test]
fn array_data_on_stack_array_reports_no_fastpath() {
    let vm = VmContext::new();
    let ctx = make_jit_ctx(&vm);
    let mut frame = JitFrame::new(&vm, 4, &[]);
    frame.regs[0] = stack_int_array(&vm, 7, vec![10, 20, 30]);
    let (ptr, _, width) = data(&mut frame, &ctx);
    assert!(ptr.is_null(), "无 packed 快路 → ptr 必须是 null");
    assert_eq!(width, 0, "width=0 是内联回落慢路 jit_array_get 的信号（见 jit/translate/array.rs）");
}

/// 非数组 / null 同样只报「无快路」、不抛：异常统一由慢路 helper（objops）在真实访问点给出，
/// 文本与 interp 相同。此前这里自带一条 `ArrayGet: expected array, got Null`（ArraySet 也报 ArrayGet）。
#[test]
fn array_data_on_null_or_non_array_never_throws() {
    let vm = VmContext::new();
    let ctx = make_jit_ctx(&vm);
    let mut frame = JitFrame::new(&vm, 4, &[]);
    for v in [Value::Null, Value::I64(42)] {
        frame.regs[0] = v;
        let (ptr, _, width) = data(&mut frame, &ctx);
        assert!(ptr.is_null());
        assert_eq!(width, 0);
        assert!(crate::jit::helpers::take_exception(&vm).is_none(), "快路取数不得置异常");
    }
}

/// 慢路：null 数组读 → 置异常、返回 1；栈数组读走 arena 拿到正确元素。
#[test]
fn array_get_slow_path_handles_null_and_stack_array() {
    let vm = VmContext::new();
    let ctx = make_jit_ctx(&vm);
    let mut frame = JitFrame::new(&vm, 4, &[]);
    frame.regs[0] = Value::Null;
    frame.regs[1] = Value::I64(0);
    assert_eq!(unsafe { jit_array_get(&mut frame, &ctx, 2, 0, 1) }, 1);
    // 无 stdlib ⇒ 退化成字符串异常，文本 = `<类名>: <消息>`，与 interp 的内部错误文本同一条。
    match crate::jit::helpers::take_exception(&vm) {
        Some(Value::Str(s)) => assert_eq!(&*s,
            "Std.NullReferenceException: cannot read an element of a null array"),
        other => panic!("expected a string exception, got {other:?}"),
    }

    frame.regs[0] = stack_int_array(&vm, 9, vec![1, 2]);
    frame.regs[1] = Value::I64(1);
    assert_eq!(unsafe { jit_array_get(&mut frame, &ctx, 2, 0, 1) }, 0);
    assert!(matches!(frame.regs[2], Value::I64(2)));
}
