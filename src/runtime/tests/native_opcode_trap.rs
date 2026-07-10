//! C1-scaffold trap behaviour: the four native interop opcodes are decoded
//! cleanly but always trap with a clear error pointing at the implementing
//! spec when actually executed.
//!
//! Once specs C2/C4/C5 land, the corresponding test below should be flipped
//! from "expects trap" to "expects success".

use std::collections::HashMap;

use z42::metadata::{
    BasicBlock, CallNativeInsn, ExecMode, FieldGetInsn, Function, Instruction, Module, Terminator, Value,
};
use z42::vm_context::VmContext;

fn module_with_single_instr(name: &str, instr: Instruction) -> Module {
    let func = Function {
        name: format!("{name}.Main"),
        param_count: 0,
        ret_type: "void".to_string(),
        exec_mode: ExecMode::Interp,
        blocks: vec![BasicBlock {
            label: "entry".to_string(),
            instructions: vec![instr],
            terminator: Terminator::Ret { reg: None },
        }],
        is_static: true,
        visibility: 0, method_flags: 0, min_arg: 0, params_from: 0xFF,        max_reg: 4,
        cold: None,
        reg_types: Box::new([]),
        block_index: HashMap::new(),
        resolved: std::sync::OnceLock::new(),
    };

    Module {
        name: name.to_string(),
        string_pool: vec![],
        classes: vec![],
        functions: vec![func],
        type_registry: HashMap::new(),
        type_registry_vec: Vec::new(),
        func_index: HashMap::new(),
        func_ref_cache_slots: 0,
        interned_strings: Vec::new(),
    }
}

fn run(module: &Module) -> anyhow::Result<()> {
    let ctx = VmContext::new();
    let func = &module.functions[0];
    z42::interp::run(&ctx, module, func, &[] as &[Value])
}

fn assert_trap_with(err: anyhow::Error, fragment: &str) {
    let msg = format!("{err:#}");
    assert!(
        msg.contains(fragment),
        "expected error to contain `{fragment}`, got: {msg}"
    );
}

#[test]
fn call_native_unknown_type_traps() {
    // C2 (`impl-tier1-c-abi`) flipped CallNative from a blanket trap to
    // real registry+libffi dispatch; the failure mode now is "unknown
    // native type" because no library has registered numz42::Tensor.
    let m = module_with_single_instr(
        "call_native_unknown_type",
        Instruction::CallNative(Box::new(CallNativeInsn {
            dst: 0,
            module: "numz42".into(),
            type_name: "Tensor".into(),
            symbol: "__shim_Tensor_dot".into(),
            args: vec![].into(),
        })),
    );
    let err = run(&m).expect_err("CallNative must fail when type is unregistered");
    assert_trap_with(err, "unknown native type");
}

#[test]
fn call_native_vtable_traps_with_spec_pointer() {
    let m = module_with_single_instr(
        "call_native_vtable_test",
        Instruction::CallNativeVtable {
            dst: 0,
            recv: 1,
            vtable_slot: 7,
            args: vec![].into(),
        },
    );
    let err = run(&m).expect_err("CallNativeVtable must trap in C1");
    assert_trap_with(err, "spec C5");
}

#[test]
fn pin_ptr_non_str_traps() {
    // C4 (`impl-pinned-block`) flipped PinPtr from a blanket trap to real
    // dispatch on `Value::Str` / `Value::Array`. Other source variants
    // throw Std.InvalidMarshalException; in a stdlib-less test module the
    // runtime surfaces the marshal-failure path as "stdlib type
    // `Std.InvalidMarshalException` not loaded; cannot construct exception".
    let m = module_with_single_instr(
        "pin_ptr_non_str",
        Instruction::PinPtr { dst: 0, src: 1 },
    );
    // r1 is uninitialised → defaults to Value::Null in Frame, which falls
    // into the catch-all bail.
    let err = run(&m).expect_err("PinPtr Null source must fail");
    assert_trap_with(err, "InvalidMarshalException");
}

#[test]
fn unpin_ptr_non_view_traps() {
    // C4: UnpinPtr is a hard error when its argument isn't a PinnedView.
    let m = module_with_single_instr(
        "unpin_ptr_non_view",
        Instruction::UnpinPtr { pinned: 1 },
    );
    let err = run(&m).expect_err("UnpinPtr non-view must fail");
    assert_trap_with(err, "UnpinPtr expects PinnedView");
}
