//! Tests for interpreter support helpers on hand-built IR.

use super::*;
use crate::metadata::bytecode::{BasicBlock, ExceptionEntry, FunctionCold, Instruction, Terminator};
use crate::metadata::types::ExecMode;

fn block(label: &str) -> BasicBlock {
    BasicBlock { label: label.to_string(), instructions: Vec::<Instruction>::new(), terminator: Terminator::Ret { reg: None } }
}

fn entry(try_start: &str, try_end: &str, catch_label: &str) -> ExceptionEntry {
    ExceptionEntry {
        try_start: try_start.to_string(),
        try_end: try_end.to_string(),
        catch_label: catch_label.to_string(),
        catch_type: None,
        catch_reg: 0,
        catch_key: Default::default(),
    }
}

#[test]
fn find_handler_skips_an_entry_whose_try_end_does_not_resolve() {
    // z42c never emits this shape; the zbc reader produces it for an
    // out-of-range `try_end`. The unresolved first entry must not hide the
    // second one, which covers b0.
    let func = Function {
        name: "T.f".to_string(),
        param_count: 0,
        ret_type: "void".to_string(),
        exec_mode: ExecMode::Interp,
        blocks: vec![block("b0"), block("b1"), block("b2")],
        is_static: true,
        visibility: 0,
        method_flags: 0, min_arg: 0, params_from: 0xFF,
        max_reg: 1,
        cold: Some(Box::new(FunctionCold {
            exception_table: vec![entry("b0", "block_3", "b1"), entry("b0", "b1", "b2")].into_boxed_slice(),
            ..Default::default()
        })),
        reg_types: Box::new([]),
        block_index: HashMap::new(),
        branch_targets: Vec::new(),
        fused_tails: Vec::new(),
        frame_meta: None,
        resolved: std::sync::OnceLock::new(),
        owner_init: Default::default(),
        id: Default::default(),
    };
    let block_map: HashMap<String, usize> =
        [("b0", 0), ("b1", 1), ("b2", 2)].into_iter().map(|(l, i)| (l.to_string(), i)).collect();
    let ctx = VmContext::new();
    let found = find_handler(&ctx, &func, 0, &block_map, &rustc_hash::FxHashMap::default(), &Value::I64(1));
    assert_eq!(found, Some(1));
}
