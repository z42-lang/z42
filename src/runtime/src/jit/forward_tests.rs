//! `builtin_forward` recognition: only an exact `[Native]` extern wrapper
//! (one `builtin` over all params in order, then `ret` of its result) qualifies.
//! End-to-end behaviour (result / void / thrown error through the short-circuit)
//! is covered by the golden suite under `--mode jit`.

use super::builtin_forward;
use crate::metadata::bytecode::{BasicBlock, BuiltinInsn, CallInsn, Function, Instruction, Terminator};
use crate::metadata::types::ExecMode;

fn wrapper(name: &str, params: usize, args: &[u32], ret: Option<u32>) -> Function {
    Function {
        name: "T.C.F".to_string(),
        param_count: params,
        ret_type: "i64".to_string(),
        exec_mode: ExecMode::Jit,
        blocks: vec![BasicBlock {
            label: "entry".to_string(),
            instructions: vec![Instruction::Builtin(Box::new(BuiltinInsn {
                dst: params as u32, name: name.to_string(), args: args.into(),
            }))],
            terminator: Terminator::Ret { reg: ret },
        }],
        is_static: false,
        visibility: 0,
        method_flags: 0, min_arg: 0, params_from: 0xFF,
        max_reg: 0,
        cold: None,
        reg_types: Box::new([]),
        block_index: std::collections::HashMap::new(),
        branch_targets: Vec::new(),
        fused_tails: Vec::new(),
        frame_meta: None,
        resolved: std::sync::OnceLock::new(),
        owner_init: Default::default(),
        id: Default::default(),
    }
}

#[test]
fn exact_wrapper_forwards_to_its_builtin() {
    let id = crate::corelib::builtin_id_of("__str_char_at").expect("builtin exists").0;
    assert_eq!(builtin_forward(&wrapper("__str_char_at", 2, &[0, 1], Some(2))), Some(id));
    // void wrapper (`builtin …; ret`) also qualifies.
    assert_eq!(builtin_forward(&wrapper("__str_char_at", 2, &[0, 1], None)), Some(id));
}

#[test]
fn anything_but_an_exact_wrapper_is_rejected() {
    // Arguments reordered / dropped / extra.
    assert_eq!(builtin_forward(&wrapper("__str_char_at", 2, &[1, 0], Some(2))), None);
    assert_eq!(builtin_forward(&wrapper("__str_char_at", 2, &[0], Some(2))), None);
    assert_eq!(builtin_forward(&wrapper("__str_char_at", 1, &[0, 1], Some(1))), None);
    // Returns something other than the builtin's result.
    assert_eq!(builtin_forward(&wrapper("__str_char_at", 2, &[0, 1], Some(0))), None);
    // Unknown builtin name: no id to dispatch → keep the ordinary call.
    assert_eq!(builtin_forward(&wrapper("__no_such_builtin", 2, &[0, 1], Some(2))), None);
    // Extra instruction in the body.
    let mut f = wrapper("__str_char_at", 2, &[0, 1], Some(2));
    f.blocks[0].instructions.push(Instruction::Builtin(Box::new(BuiltinInsn {
        dst: 3, name: "__str_char_at".to_string(), args: vec![0, 1].into(),
    })));
    assert_eq!(builtin_forward(&f), None);
}

fn call_wrapper(name: &str, target: &str, params: usize, args: &[u32]) -> Function {
    let mut f = wrapper("__str_char_at", params, args, Some(params as u32));
    f.name = name.to_string();
    f.blocks[0].instructions = vec![Instruction::Call(Box::new(CallInsn {
        dst: params as u32, func: target.to_string(), args: args.into(), method_type_args: Box::new([]),
    }))];
    f
}

#[test]
fn chained_wrapper_forwards_only_within_its_own_type() {
    use super::chained_forward;
    let f = call_wrapper("Std.String.get_Item", "Std.String.CharAt", 2, &[0, 1]);
    assert_eq!(chained_forward(&f, |n| (n == "Std.String.CharAt").then_some(7)), Some(7));
    // Target is not a forwarder.
    assert_eq!(chained_forward(&f, |_| None), None);
    // Target in another type: its static-constructor barrier would be skipped.
    let g = call_wrapper("Std.String.get_Item", "Std.Other.CharAt", 2, &[0, 1]);
    assert_eq!(chained_forward(&g, |_| Some(7)), None);
    // Overload keys (`$arity$sig`) still compare by owning type.
    let h = call_wrapper("Std.String.Sub$1$int", "Std.String.Substring$2$int$int", 2, &[0, 1]);
    assert_eq!(chained_forward(&h, |_| Some(7)), Some(7));
    // Arguments not forwarded verbatim.
    let k = call_wrapper("Std.String.get_Item", "Std.String.CharAt", 2, &[1, 0]);
    assert_eq!(chained_forward(&k, |_| Some(7)), None);
}
