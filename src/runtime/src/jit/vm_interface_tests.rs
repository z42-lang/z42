//! Unit tests for the `JitVm` read-only metadata contract.
//!
//! Two-pronged coverage:
//! 1. `impl JitVm for Module` forwards correctly to each `Module` field.
//! 2. A minimal `MockMetadata` struct implements `JitVm` independently,
//!    proving the trait is mockable for tests that don't want to construct
//!    a full `Module` (one of the motivating ROI points for the trait —
//!    see `vm_interface.rs` module docs).

use super::*;
use crate::metadata::bytecode::{BasicBlock, ClassDesc, Function, Module, Terminator};
use crate::metadata::tokens::TypeId;
use crate::metadata::{NameIndex, TypeDesc};
use crate::metadata::types::ExecMode;
use std::sync::Arc;

// ── Module impl forwards correctly ───────────────────────────────────────────

fn empty_function(name: &str) -> Function {
    Function {
        name: name.to_string(),
        param_count: 0,
        ret_type: "void".to_string(),
        exec_mode: ExecMode::Interp,
        blocks: vec![BasicBlock {
            label: "entry".to_string(),
            instructions: Vec::new(),
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
        fused_tails: Vec::new(),
        frame_meta: None,
        resolved: std::sync::OnceLock::new(),
    }
}

fn empty_type_desc(name: &str, id: TypeId) -> Arc<TypeDesc> {
    Arc::new(TypeDesc {
        class_flags: 0,
        visibility: 0,
        name: name.to_string(),
        base_name: None,
        id,
        fields: Vec::new(),
        field_index: NameIndex::new(),
        vtable: Vec::new(),
        vtable_index: NameIndex::new(),
        cold: None,
    })
}

fn module_with(
    name: &str,
    fn_names: &[&str],
    pool: &[&str],
    classes: &[&str],
) -> Module {
    let functions: Vec<Function> = fn_names.iter().map(|n| empty_function(n)).collect();
    let func_index = functions.iter().enumerate()
        .map(|(i, f)| (f.name.clone(), i))
        .collect();
    let string_pool: Vec<String> = pool.iter().map(|s| s.to_string()).collect();
    let mut type_registry = std::collections::HashMap::new();
    let mut type_registry_vec = Vec::new();
    for (idx, class_name) in classes.iter().enumerate() {
        let td = empty_type_desc(class_name, TypeId(idx as u32));
        type_registry.insert((*class_name).to_string(), td.clone());
        type_registry_vec.push(td);
    }
    Module {
        name: name.to_string(),
        string_pool,
        classes: Vec::new(),
        functions,
        type_registry,
        type_registry_vec,
        func_index,
        func_ref_cache_slots: 0,
        interned_strings: Vec::new(),
    }
}

#[test]
fn jit_vm_module_name_round_trips() {
    let m = module_with("Demo.App", &[], &[], &[]);
    assert_eq!(m.module_name(), "Demo.App");
}

#[test]
fn jit_vm_functions_preserves_order_and_count() {
    let m = module_with("Mod", &["a", "b", "c"], &[], &[]);
    let fns = m.functions();
    assert_eq!(fns.len(), 3);
    assert_eq!(fns[0].name, "a");
    assert_eq!(fns[1].name, "b");
    assert_eq!(fns[2].name, "c");
}

#[test]
fn jit_vm_string_pool_returns_borrowed_slice() {
    let m = module_with("Mod", &[], &["alpha", "beta"], &[]);
    let pool = m.string_pool();
    assert_eq!(pool.len(), 2);
    assert_eq!(pool[0], "alpha");
    assert_eq!(pool[1], "beta");
}

#[test]
fn jit_vm_type_lookup_finds_registered_class() {
    let m = module_with("Mod", &[], &[], &["Demo.Point", "Demo.Vector"]);
    let td = m.type_lookup("Demo.Point").expect("registered class must resolve");
    assert_eq!(td.name, "Demo.Point");
}

#[test]
fn jit_vm_type_lookup_misses_unknown_class() {
    let m = module_with("Mod", &[], &[], &["Demo.Point"]);
    assert!(m.type_lookup("Demo.Missing").is_none());
    assert!(m.type_lookup("").is_none());
}

// ── Mockable: minimal JitVm impl without building a Module ──────────────────

/// Tiny synthetic implementor that proves the trait is decoupled from
/// `Module`'s concrete shape. Useful for unit tests in future Phase 2 work
/// that wants to drive a single trait method without building a real module.
struct MockMetadata {
    name: String,
    funcs: Vec<Function>,
    pool: Vec<String>,
    interned: Vec<Arc<str>>,
    types: std::collections::HashMap<String, Arc<TypeDesc>>,
}

impl JitVm for MockMetadata {
    fn functions(&self) -> &[Function] { &self.funcs }
    fn string_pool(&self) -> &[String] { &self.pool }
    fn interned_strings(&self) -> &[Arc<str>] { &self.interned }
    fn module_name(&self) -> &str { &self.name }
    fn type_lookup(&self, class_name: &str) -> Option<&Arc<TypeDesc>> {
        self.types.get(class_name)
    }
}

#[test]
fn mock_metadata_satisfies_trait() {
    let mut types = std::collections::HashMap::new();
    types.insert("Demo.Mock".to_string(), empty_type_desc("Demo.Mock", TypeId::UNRESOLVED));
    let mock = MockMetadata {
        name: "Synthetic".to_string(),
        funcs: vec![empty_function("entry")],
        pool: vec!["one".into(), "two".into()],
        interned: vec![Arc::from("one"), Arc::from("two")],
        types,
    };
    // Drive every trait method through a `&dyn JitVm` reference to prove
    // dyn-dispatch works (future AOT entry points may pass `&dyn JitVm`).
    let dyn_ref: &dyn JitVm = &mock;
    assert_eq!(dyn_ref.module_name(), "Synthetic");
    assert_eq!(dyn_ref.functions().len(), 1);
    assert_eq!(dyn_ref.string_pool().len(), 2);
    assert!(dyn_ref.type_lookup("Demo.Mock").is_some());
    assert!(dyn_ref.type_lookup("Demo.Absent").is_none());
}

#[test]
fn unused_classes_field_initializer_still_satisfies_trait() {
    // Sanity: the `classes` field on Module (ClassDesc list used only at
    // loader time) is not part of the trait surface, but its presence in
    // the struct shouldn't interfere with trait-method dispatch.
    let _unused: ClassDesc; // suppress unused-import noise if any
    let m = module_with("X", &["f"], &["p"], &["T"]);
    assert_eq!(m.module_name(), "X");
    assert_eq!(m.functions().len(), 1);
    assert!(m.type_lookup("T").is_some());
}
