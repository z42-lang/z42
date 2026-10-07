//! add-struct-jit-value-path (P5): unit tests for the JIT struct helpers.
//!
//! These drive the `extern "C"` helpers against a minimal `JitModuleCtx`
//! (module pointer dangles — struct helpers only touch `vm_ctx.struct_arena` /
//! `vm_ctx.try_lookup_type`, never the module), covering the JIT-specific surface:
//! reg-file read/write, lazy `frame_id` assignment, and the exception path.
//!
//! The base-polymorphic dispatch (heap `Object` inline field / `StructRefHeap`
//! array element), reference leaves, and unbox share the interp `*_val` cores
//! (interp-tested) and are exercised end-to-end by the `--mode jit` golden
//! `src/tests/types/struct_jit.z42`.

use super::*;
use super::super::super::frame::{JitFrame, JitModuleCtx};
use crate::metadata::types::{Value, TAG_I32};
use crate::vm_context::VmContext;

/// Build a minimal JIT ctx whose only live field is `vm_ctx` (module dangles).
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
        stack_limit: 0,
    }
}

/// symbolic-struct-field-access P2：字段访问改带 `(root 类型名, 字段序号路径)`，
/// 解析**必须**拿到该类型的布局 ⇒ 这些测试不能再用「没注册的 `"T"`」了。
///
/// 注册一个 `Demo.Pair { int a; int b; }`：两条 i32 叶子 @0/@4，字段表与 `fields` **同长**
/// （zbc 1.45 的平行性约定，载入期有断言守着）。
const PAIR: &str = "Demo.Pair";

fn register_pair(vm: &VmContext) {
    use crate::metadata::types::{StructFieldLayout, StructTypeLayout, TypeDesc, TypeDescCold, FieldSlot};
    use crate::metadata::tokens::TypeId;
    use crate::metadata::name_index::NameIndex;
    let layout = StructTypeLayout {
        size: 8,
        ref_offsets: Box::new([]),
        ref_kinds: Box::new([]),
        fields: Box::new([
            StructFieldLayout { offset: 0, size: 4, kind: 0 },
            StructFieldLayout { offset: 4, size: 4, kind: 0 },
        ]),
    };
    let td = std::sync::Arc::new(TypeDesc {
        name: PAIR.into(),
        class_flags: crate::metadata::bytecode::CLASS_FLAG_STRUCT,
        fields: vec![
            FieldSlot { name: "a".into(), type_tag: "int".into(), visibility: 0 },
            FieldSlot { name: "b".into(), type_tag: "int".into(), visibility: 0 },
        ],
        field_index: NameIndex::new(),
        vtable: Vec::new(),
        vtable_index: NameIndex::new(),
        base_name: None,
        visibility: 0,
        cold: Some(Box::new(TypeDescCold {
            struct_layout: Some(std::sync::Arc::new(layout)),
            ..Default::default()
        })),
        id: TypeId::UNRESOLVED,
    });
    vm.install_lazy_loader(None, 0);
    if let Some(l) = vm.core.lazy_loader.write().as_mut() {
        l.insert_type(PAIR.to_string(), td);
    }
}

/// Allocate the registered `Demo.Pair` (8 bytes, two i32 leaves) into `regs[dst]`.
unsafe fn alloc_pair(frame: *mut JitFrame, ctx: *const JitModuleCtx, dst: u32) {
    jit_struct_alloc(frame, ctx, dst, PAIR.as_ptr(), PAIR.len(), 8);
}

/// 驱动符号化 helper：`path` 是字段序号（`Demo.Pair` 的 a=0 / b=1）。
unsafe fn jit_set(frame: *mut JitFrame, ctx: *const JitModuleCtx, base: u32, path: &[u16], kind: u8, val: u32) -> u8 {
    jit_struct_field_set_prim(frame, ctx, base, PAIR.as_ptr(), PAIR.len() as u64,
                              path.as_ptr(), path.len() as u64, kind, val)
}
unsafe fn jit_get(frame: *mut JitFrame, ctx: *const JitModuleCtx, dst: u32, base: u32, path: &[u16], kind: u8) -> u8 {
    jit_struct_field_get_prim(frame, ctx, dst, base, PAIR.as_ptr(), PAIR.len() as u64,
                              path.as_ptr(), path.len() as u64, kind)
}

#[test]
fn alloc_stamps_nonzero_frame_id_and_struct_ref() {
    let vm = VmContext::new();
    register_pair(&vm);
    let ctx = make_jit_ctx(&vm);
    let mut frame = JitFrame::new(&vm, 4, &[]);
    assert_eq!(frame.frame_id, 0, "fresh frame starts unassigned");
    unsafe { alloc_pair(&mut frame, &ctx, 0) };
    assert_ne!(frame.frame_id, 0, "allocating a struct lazily assigns a real id");
    assert!(matches!(frame.regs[0], Value::StructRef { .. }), "dst holds a StructRef handle");
    // A second alloc in the same frame keeps the same id (stable within a frame).
    let fid = frame.frame_id;
    unsafe { alloc_pair(&mut frame, &ctx, 1) };
    assert_eq!(frame.frame_id, fid, "frame id is stable across allocations");
}

#[test]
fn field_set_get_prim_round_trips() {
    let vm = VmContext::new();
    register_pair(&vm);
    let ctx = make_jit_ctx(&vm);
    let mut frame = JitFrame::new(&vm, 8, &[]);
    unsafe {
        alloc_pair(&mut frame, &ctx, 0);
        // regs[5] = 42 → write leaf@0; regs[6] = -7 → write leaf@4.
        frame.regs[5] = Value::I64(42);
        assert_eq!(jit_set(&mut frame, &ctx, 0, &[0], TAG_I32, 5), 0);
        frame.regs[6] = Value::I64(-7);
        assert_eq!(jit_set(&mut frame, &ctx, 0, &[1], TAG_I32, 6), 0);
        // read both leaves back
        assert_eq!(jit_get(&mut frame, &ctx, 7, 0, &[0], TAG_I32), 0);
        assert_eq!(jit_get(&mut frame, &ctx, 8, 0, &[1], TAG_I32), 0);
    }
    assert_eq!(frame.regs[7], Value::I64(42));
    assert_eq!(frame.regs[8], Value::I64(-7));
}

#[test]
fn copy_is_value_independent() {
    let vm = VmContext::new();
    register_pair(&vm);
    let ctx = make_jit_ctx(&vm);
    let mut frame = JitFrame::new(&vm, 8, &[]);
    unsafe {
        alloc_pair(&mut frame, &ctx, 0); // src
        alloc_pair(&mut frame, &ctx, 1); // dst
        frame.regs[5] = Value::I64(100);
        assert_eq!(jit_set(&mut frame, &ctx, 0, &[0], TAG_I32, 5), 0);
        // dst = src (blob copy)
        assert_eq!(jit_struct_copy(&mut frame, &ctx, 1, 0, 8), 0);
        // mutate src leaf@0 = 999
        frame.regs[5] = Value::I64(999);
        assert_eq!(jit_set(&mut frame, &ctx, 0, &[0], TAG_I32, 5), 0);
        // dst still 100 (independent copy)
        assert_eq!(jit_get(&mut frame, &ctx, 7, 1, &[0], TAG_I32), 0);
    }
    assert_eq!(frame.regs[7], Value::I64(100), "dst is an independent value copy");
}

#[test]
fn field_op_on_non_struct_base_raises() {
    let vm = VmContext::new();
    register_pair(&vm);
    let ctx = make_jit_ctx(&vm);
    let mut frame = JitFrame::new(&vm, 4, &[]);
    frame.regs[0] = Value::I64(5); // not a struct handle
    frame.regs[1] = Value::I64(1);
    unsafe {
        // set/get on a non-struct base → return 1 (+ pending exception), no UB.
        assert_eq!(jit_set(&mut frame, &ctx, 0, &[0], TAG_I32, 1), 1);
        assert_eq!(jit_get(&mut frame, &ctx, 2, 0, &[0], TAG_I32), 1);
    }
}

#[test]
fn stale_struct_ref_is_caught() {
    let vm = VmContext::new();
    register_pair(&vm);
    let ctx = make_jit_ctx(&vm);
    let mut frame = JitFrame::new(&vm, 4, &[]);
    // A handle into an arena slot that was never allocated for this id → the
    // staleness guard must reject it (return 1), not silently read garbage.
    frame.regs[0] = Value::StructRef { idx: 999, frame_id: 424242 };
    unsafe {
        assert_eq!(jit_get(&mut frame, &ctx, 1, 0, &[0], TAG_I32), 1);
    }
}
