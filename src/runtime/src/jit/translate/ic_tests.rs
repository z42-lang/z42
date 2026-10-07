//! Site-token readers used at translate time.

use super::method_id_at;
use crate::metadata::bytecode::{BasicBlock, Function, Terminator};
use crate::metadata::resolver::ResolvedTokens;
use crate::metadata::tokens::UNRESOLVED;
use crate::metadata::types::ExecMode;
use std::sync::atomic::AtomicU32;

fn with_call_tokens(tokens: &[u32]) -> Function {
    let f = Function {
        name: "f".to_string(),
        param_count: 0,
        ret_type: "void".to_string(),
        exec_mode: ExecMode::Jit,
        blocks: vec![BasicBlock { label: "entry".to_string(), instructions: Vec::new(), terminator: Terminator::Ret { reg: None } }],
        is_static: true,
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
    };
    let _ = f.resolved.set(ResolvedTokens {
        method_tokens: tokens.iter().map(|&t| AtomicU32::new(t)).collect(),
        site_index: vec![(0..tokens.len() as u32).collect()],
        ..Default::default()
    });
    f
}

/// A `Call` token at or past `merged_len` is a lazily loaded function's `FnId` —
/// not a JIT merged id (it would alias the JIT's synthetic lazy-slot ids), so it
/// bakes as `UNRESOLVED` and `jit_call` binds by name.
#[test]
fn method_id_at_bakes_only_merged_ids() {
    let f = with_call_tokens(&[3, 9, 10, UNRESOLVED]);
    let merged_len = 10;
    assert_eq!(method_id_at(&f, 0, 0, merged_len), 3);
    assert_eq!(method_id_at(&f, 0, 1, merged_len), 9);
    assert_eq!(method_id_at(&f, 0, 2, merged_len), UNRESOLVED);
    assert_eq!(method_id_at(&f, 0, 3, merged_len), UNRESOLVED);
}
