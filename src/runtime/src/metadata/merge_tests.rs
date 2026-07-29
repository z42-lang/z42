use super::*;
use crate::metadata::{BasicBlock, ExecMode, Function, Instruction, Module, Terminator};

fn make_module(name: &str, strings: &[&str], const_str_idx: u32) -> Module {
    make_module_with(name, strings, const_str_idx, vec![], vec![])
}

fn make_module_with(
    name: &str,
    strings: &[&str],
    const_str_idx: u32,
    classes: Vec<crate::metadata::ClassDesc>,
    extra_functions: Vec<Function>,
) -> Module {
    let instr = Instruction::ConstStr { dst: 0, idx: const_str_idx };
    let block = BasicBlock {
        label: "entry".to_string(),
        instructions: vec![instr],
        terminator: Terminator::Ret { reg: None },
    };
    let mut functions = vec![Function {
        name: format!("{}.Main", name),
        param_count: 0,
        ret_type: "void".to_string(),
        exec_mode: ExecMode::Interp,
        blocks: vec![block],
        is_static: false,
        visibility: 0,
        method_flags: 0, min_arg: 0, params_from: 0xFF,
        max_reg: 0,
        cold: None,
        reg_types: Box::new([]),
        block_index: std::collections::HashMap::new(),
        branch_targets: Vec::new(),
        resolved: std::sync::OnceLock::new(),
    }];
    functions.extend(extra_functions);
    Module {
        name: name.to_string(),
        string_pool: strings.iter().map(|s| s.to_string()).collect(),
        classes,
        functions,
        type_registry: std::collections::HashMap::new(),
        type_registry_vec: Vec::new(),
        func_index: std::collections::HashMap::new(),
        func_ref_cache_slots: 0,
        interned_strings: Vec::new(),
    }
}

#[test]
fn merge_single_module_is_identity() {
    let m = make_module("A", &["hello"], 0);
    let merged = merge_modules(vec![m]).unwrap();
    assert_eq!(merged.string_pool, vec!["hello"]);
    // idx unchanged (offset 0)
    assert!(matches!(
        merged.functions[0].blocks[0].instructions[0],
        Instruction::ConstStr { idx: 0, .. }
    ));
}

#[test]
fn merge_two_modules_concatenates_pools_and_remaps() {
    let m0 = make_module("A", &["hello"], 0); // pool=[hello], idx=0
    let m1 = make_module("B", &["world"], 0); // pool=[world], idx=0 → should become 1

    let merged = merge_modules(vec![m0, m1]).unwrap();

    assert_eq!(merged.string_pool, vec!["hello", "world"]);
    assert_eq!(merged.functions.len(), 2);

    // A.Main: idx still 0 (first module, no shift)
    assert!(matches!(
        merged.functions[0].blocks[0].instructions[0],
        Instruction::ConstStr { idx: 0, .. }
    ));
    // B.Main: idx was 0, pool offset = 1 → now 1
    assert!(matches!(
        merged.functions[1].blocks[0].instructions[0],
        Instruction::ConstStr { idx: 1, .. }
    ));
}

#[test]
fn merge_three_modules_offsets_accumulate() {
    let m0 = make_module("A", &["a0", "a1"], 1); // idx=1 → stays 1
    let m1 = make_module("B", &["b0"],       0); // idx=0 → +2 → 2
    let m2 = make_module("C", &["c0", "c1"], 0); // idx=0 → +3 → 3

    let merged = merge_modules(vec![m0, m1, m2]).unwrap();

    assert_eq!(merged.string_pool, vec!["a0", "a1", "b0", "c0", "c1"]);

    let idx = |fi: usize| match merged.functions[fi].blocks[0].instructions[0] {
        Instruction::ConstStr { idx, .. } => idx,
        _ => panic!("expected ConstStr"),
    };
    assert_eq!(idx(0), 1); // A: no shift
    assert_eq!(idx(1), 2); // B: 0 + 2
    assert_eq!(idx(2), 3); // C: 0 + 3
}

#[test]
fn merge_empty_returns_error() {
    assert!(merge_modules(vec![]).is_err());
}

#[test]
fn merge_deduplicates_classes_by_name() {
    use crate::metadata::{ClassDesc, FieldDesc};

    let cls = ClassDesc {
        static_fields: vec![].into(),
        interfaces: vec![].into(),
        enum_members: vec![].into(),
        iface_methods: vec![].into(),
        class_flags: 0,
        name: "Std.Object".to_string(),
        base_class: None,
        fields: vec![FieldDesc { name: "x".to_string(), type_tag: "i32".to_string(), attributes: Box::new([]), visibility: 0 }].into_boxed_slice(),
        type_params: Box::new([]),
        type_param_constraints: Box::new([]),
        attributes: Box::new([]),
    };
    let cls_dup = ClassDesc {
        static_fields: vec![].into(),
        interfaces: vec![].into(),
        enum_members: vec![].into(),
        iface_methods: vec![].into(),
        class_flags: 0,
        name: "Std.Object".to_string(),
        base_class: None,
        fields: Box::new([]),
        type_params: Box::new([]),
        type_param_constraints: Box::new([]),
        attributes: Box::new([]),
    };
    let m0 = make_module_with("A", &["a"], 0, vec![cls], vec![]);
    let m1 = make_module_with("B", &["b"], 0, vec![cls_dup], vec![]);

    let merged = merge_modules(vec![m0, m1]).unwrap();
    // Only one Std.Object, the first one (with field "x")
    let obj_classes: Vec<_> = merged.classes.iter().filter(|c| c.name == "Std.Object").collect();
    assert_eq!(obj_classes.len(), 1);
    assert_eq!(obj_classes[0].fields.len(), 1);
}

#[test]
fn merge_deduplicates_functions_by_name() {
    let dup_func = Function {
        name: "A.Main".to_string(),
        param_count: 0,
        ret_type: "i32".to_string(), // different ret_type to distinguish
        exec_mode: ExecMode::Interp,
        blocks: vec![BasicBlock {
            label: "entry".to_string(),
            instructions: vec![Instruction::ConstStr { dst: 0, idx: 0 }],
            terminator: Terminator::Ret { reg: None },
        }],
        is_static: false,
        visibility: 0,
        method_flags: 0, min_arg: 0, params_from: 0xFF,
        max_reg: 0,
        cold: None,
        reg_types: Box::new([]),
        block_index: std::collections::HashMap::new(),
        branch_targets: Vec::new(),
        resolved: std::sync::OnceLock::new(),
    };
    let m0 = make_module("A", &["hello"], 0);
    let m1 = make_module_with("B", &["world"], 0, vec![], vec![dup_func]);

    let merged = merge_modules(vec![m0, m1]).unwrap();
    // A.Main appears only once (the first one with ret_type "void")
    let a_mains: Vec<_> = merged.functions.iter().filter(|f| f.name == "A.Main").collect();
    assert_eq!(a_mains.len(), 1);
    assert_eq!(a_mains[0].ret_type, "void");
}
