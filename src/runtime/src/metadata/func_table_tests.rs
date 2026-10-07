use super::*;
use crate::metadata::bytecode::{BasicBlock, Terminator};
use crate::metadata::types::ExecMode;

fn stub_function(name: &str) -> Function {
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
    }
}

/// An entry module whose `func_index` is filled the way the loader fills it.
fn entry_module(names: &[&str]) -> Arc<Module> {
    let functions: Vec<Function> = names.iter().map(|n| stub_function(n)).collect();
    let func_index = names.iter().enumerate().map(|(i, n)| (n.to_string(), i)).collect();
    Arc::new(Module {
        name: "Entry".to_string(),
        string_pool: vec![],
        classes: vec![],
        functions,
        type_registry: FxHashMap::default(),
        func_index,
    })
}

#[test]
fn entry_ids_equal_module_indices() {
    let m = entry_module(&["App.Main$0", "App.Helper$1", "App.Other$0"]);
    let t = FuncTable::new(Some(Arc::clone(&m)));
    assert_eq!(t.len(), 3);
    assert_eq!(t.entry_len(), 3);
    for (i, f) in m.functions.iter().enumerate() {
        let id = FnId(i as u32);
        assert_eq!(f.id.get(), Some(id), "Function.id is set at registration");
        assert!(std::ptr::eq(t.get(id).unwrap(), f), "entry slot borrows the module's function");
        assert_eq!(t.id_of(&f.name), Some(id));
    }
    assert!(t.get(FnId(3)).is_none());
    assert!(t.id_of("App.Missing$0").is_none());
}

#[test]
fn table_without_entry_module_starts_empty() {
    let t = FuncTable::new(None);
    assert!(t.is_empty());
    assert_eq!(t.entry_len(), 0);
    assert!(t.get(FnId(0)).is_none());
}

#[test]
fn lazy_registration_appends_after_entry_functions() {
    let t = FuncTable::new(Some(entry_module(&["App.Main$0", "App.Helper$0"])));
    let a = Arc::new(stub_function("Pkg.A$0"));
    let b = Arc::new(stub_function("Pkg.B$0"));
    assert_eq!(t.register_lazy(&a), Some(FnId(2)));
    assert_eq!(t.register_lazy(&b), Some(FnId(3)));
    assert_eq!(t.len(), 4);
    assert_eq!(a.id.get(), Some(FnId(2)));
    assert!(std::ptr::eq(t.get(FnId(3)).unwrap(), Arc::as_ptr(&b)));
    assert_eq!(t.id_of("Pkg.A$0"), Some(FnId(2)));
}

#[test]
fn duplicate_lazy_name_keeps_the_first_id() {
    let t = FuncTable::new(None);
    let first = Arc::new(stub_function("Pkg.F$0"));
    let second = Arc::new(stub_function("Pkg.F$0"));
    assert_eq!(t.register_lazy(&first), Some(FnId(0)));
    assert_eq!(t.register_lazy(&second), None, "first-wins: no new id");
    assert_eq!(t.len(), 1);
    assert!(std::ptr::eq(t.get(FnId(0)).unwrap(), Arc::as_ptr(&first)));
    assert_eq!(second.id.get(), None, "the losing duplicate is not registered");
}

#[test]
fn lazy_function_shadowed_by_an_entry_name_gets_its_own_id() {
    let t = FuncTable::new(Some(entry_module(&["Shared.F$0"])));
    let lazy = Arc::new(stub_function("Shared.F$0"));
    assert_eq!(t.register_lazy(&lazy), Some(FnId(1)));
    assert_eq!(t.id_of("Shared.F$0"), Some(FnId(0)), "name lookup prefers the entry module");
    assert_eq!(t.lazy_id("Shared.F$0"), Some(FnId(1)));
    assert!(Arc::ptr_eq(&t.lazy_fn("Shared.F$0").unwrap(), &lazy));
}

/// A reader thread polls an id that is not published yet; once `get` returns
/// it, the function must be complete — right name, `Function.id` already set.
#[test]
fn get_from_another_thread_sees_a_published_function() {
    let t = Arc::new(FuncTable::new(Some(entry_module(&["App.Main$0"]))));
    let reader = {
        let t = Arc::clone(&t);
        std::thread::spawn(move || {
            for k in 1..=2000u32 {
                let f = loop {
                    if let Some(f) = t.get(FnId(k)) { break f; }
                    std::hint::spin_loop();
                };
                assert_eq!(f.name, format!("Pkg.F{k}$0"));
                assert_eq!(f.id.get(), Some(FnId(k)));
            }
        })
    };
    for k in 1..=2000u32 {
        t.register_lazy(&Arc::new(stub_function(&format!("Pkg.F{k}$0"))));
    }
    reader.join().unwrap();
    assert_eq!(t.len(), 2001);
}

#[test]
fn reset_lazy_names_forgets_names_but_keeps_slots() {
    let t = FuncTable::new(None);
    let old = Arc::new(stub_function("Pkg.F$0"));
    assert_eq!(t.register_lazy(&old), Some(FnId(0)));
    t.reset_lazy_names();
    assert_eq!(t.lazy_name_count(), 0);
    assert!(t.id_of("Pkg.F$0").is_none(), "a reinstalled loader starts empty");
    assert!(std::ptr::eq(t.get(FnId(0)).unwrap(), Arc::as_ptr(&old)), "the old id stays valid");
    let new = Arc::new(stub_function("Pkg.F$0"));
    assert_eq!(t.register_lazy(&new), Some(FnId(1)), "ids are never reused");
    assert_eq!(t.id_of("Pkg.F$0"), Some(FnId(1)));
}

#[test]
fn is_entry_is_identity_not_equality() {
    let m = entry_module(&["App.Main$0"]);
    let t = FuncTable::new(Some(Arc::clone(&m)));
    assert!(t.is_entry(&m));
    let copy = entry_module(&["App.Main$0"]);
    assert!(!t.is_entry(&copy));
    assert!(!FuncTable::new(None).is_entry(&m));
}
