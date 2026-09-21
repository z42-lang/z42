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
    JitModuleCtx {
        fn_entries_by_id: Vec::new(),
        module:           std::ptr::null(),
        lazy:             std::ptr::null(),
        merged_len:       0,
        lazy_table:       std::sync::Mutex::new(crate::jit::frame::LazyTable::default()),
        vm_ctx:           vm_ctx as *const VmContext as *mut VmContext,
        call_counts:      Vec::new(),
        jit_threshold:    1,
        osr_entries:      std::sync::Mutex::new(std::collections::HashMap::new()),
        osr_threshold:    10_000,
    }
}

/// 在 ctx 的栈 arena 里放一个 `int[]`，返回对应的 `Value::StackArray` 句柄。
fn stack_int_array(vm: &VmContext, frame_id: u32, elems: Vec<i64>) -> Value {
    let arr = ArrayObj::stack_typed("int", elems.into_iter().map(Value::I64).collect());
    let idx = vm.stack_alloc_arr(frame_id, arr);
    Value::StackArray { idx, frame_id }
}

/// 回归：栈上数组进 `jit_array_data` 必须**报「无快路」而非抛异常**。
///
/// 此前本 helper 只认 `Value::Array`，`other =>` 一律 bail ⇒
/// `ArrayGet: expected array, got StackArray`。解释器 `exec_array::array_get` 与慢路
/// `jit_array_get` 都有 StackArray 分支（后者由 fix-jit-osr-stackarray 补），唯独本快路漏了——
/// 于是「解释器能跑、一 tier-up 就崩」。实测表现：给 `z42c.syntax` 的任一 AST 类加一个字段，
/// `xtask build stdlib` 即在编译该包时崩，`Z42_STACKALLOC=off` 则绿。
#[test]
fn array_data_on_stack_array_reports_no_fastpath_instead_of_throwing() {
    let vm = VmContext::new();
    let ctx = make_jit_ctx(&vm);
    let mut frame = JitFrame::new(4, &[]);
    frame.regs[0] = stack_int_array(&vm, 7, vec![10, 20, 30]);

    let mut ptr: *const Value = 1usize as *const Value;   // 预置非 null，确保 helper 确实写了
    let mut len: i64 = -1;
    let mut width: i64 = -1;
    let rc = unsafe { jit_array_data(&mut frame, &ctx, 0, &mut ptr, &mut len, &mut width) };

    assert_eq!(rc, 0, "StackArray 不得走异常路径（rc=1 即回归）");
    assert!(ptr.is_null(), "无 packed 快路 → ptr 必须是 null");
    assert_eq!(width, 0, "width=0 是内联回落慢路 jit_array_get 的信号（见 jit/translate/array.rs）");
}

/// 对照：真正的非数组值仍然抛异常——本修复只放行 StackArray，不放宽其它形态。
#[test]
fn array_data_on_non_array_still_throws() {
    let vm = VmContext::new();
    let ctx = make_jit_ctx(&vm);
    let mut frame = JitFrame::new(4, &[]);
    frame.regs[0] = Value::I64(42);

    let mut ptr: *const Value = std::ptr::null();
    let mut len: i64 = 0;
    let mut width: i64 = 0;
    let rc = unsafe { jit_array_data(&mut frame, &ctx, 0, &mut ptr, &mut len, &mut width) };

    assert_eq!(rc, 1, "非数组仍须报异常");
}

/// `jit_array_data_opt`（非抛版）对 StackArray 本就正确——钉住它，防止两者再次分叉。
#[test]
fn array_data_opt_on_stack_array_reports_null_ptr() {
    let vm = VmContext::new();
    let ctx = make_jit_ctx(&vm);
    let mut frame = JitFrame::new(4, &[]);
    frame.regs[0] = stack_int_array(&vm, 9, vec![1, 2]);

    let mut ptr: *const Value = 1usize as *const Value;
    let mut len: i64 = -1;
    let mut width: i64 = -1;
    unsafe { jit_array_data_opt(&mut frame, &ctx, 0, &mut ptr, &mut len, &mut width) };

    assert!(ptr.is_null());
    assert_eq!(width, 0);
}
