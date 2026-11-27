//! Generic call sites pass the bound method arguments to both tiers.
//! Direct reads and forwarded method arguments both compile.

use crate::exec::outcome::ExecOutcome;
use crate::metadata::{Function, Module};
use crate::zpkg::ir::{BasicBlock, CallInsn, Instruction, Terminator, VCallInsn};
use crate::zpkg::ir::ExecMode;
use crate::object::Value;
use crate::components::bridge::VmContext;
use super::translate::{jit_unsupported_reason, unsupported_reason};

fn func(name: &str, params: usize, is_static: bool, instructions: Vec<Instruction>, ret: Option<u32>) -> Function {
    Function::new(crate::zpkg::ir::Function {
        name: name.to_string(),
        param_count: params,
        ret_type: "object".to_string(),
        exec_mode: ExecMode::Interp,
        blocks: vec![BasicBlock {
            label: "entry".to_string(),
            instructions,
            terminator: Terminator::Ret { reg: ret },
        }],
        is_static,
        visibility: 0,
        method_flags: 0,
        min_arg: 0,
        params_from: 0xFF,
        max_reg: 8,
        cold: None,
        reg_types: Box::new([]),
        block_index: std::collections::HashMap::new(),
        branch_targets: Vec::new(),
        frame_meta: None,
    })
}

fn mta(names: &[&str]) -> Box<[String]> { names.iter().map(|s| s.to_string()).collect() }

fn call(dst: u32, callee: &str, args: &[u32], type_args: &[&str]) -> Instruction {
    Instruction::Call(Box::new(CallInsn {
        dst, func: callee.to_string(), args: args.into(), method_type_args: mta(type_args),
    }))
}

fn vcall(dst: u32, obj: u32, method: &str, type_args: &[&str]) -> Instruction {
    Instruction::VCall(Box::new(VCallInsn {
        dst, obj, method: method.to_string(), args: Box::default(), method_type_args: mta(type_args),
    }))
}

/// `T.Def<T>() { return default(T); }` — reads its type arg.
fn def_body() -> Function {
    func("T.Def", 0, true, vec![Instruction::MethodDefault { dst: 0, param_index: 0 }], Some(0))
}

/// `Std.Int32.Def<T>() { return default(T); }` — the same, as an instance method on `int`
/// (a primitive receiver resolves `Std.Int32.<m>` without any class metadata).
fn int_def_body() -> Function {
    func("Std.Int32.Def", 1, false, vec![Instruction::MethodDefault { dst: 1, param_index: 0 }], Some(1))
}

/// `T.Id<T>(T x) { return x; }` — generic in the signature, blind to its type arg: compiles.
fn id_body() -> Function { func("T.Id", 1, true, vec![], Some(0)) }

fn module_of(functions: Vec<Function>) -> Module {
    let func_index = functions.iter().enumerate().map(|(i, f)| (f.name.clone(), i)).collect();
    Module::new(crate::zpkg::image::ModuleData {
        name: "M".into(),
        string_pool: Vec::new(),
        classes: Vec::new(),
        func_index,
    }, functions, rustc_hash::FxHashMap::default())
}

fn test_module() -> Module {
    module_of(vec![
        def_body(),
        int_def_body(),
        id_body(),
        // %0 = call T.Def<Std.Boolean>(); ret %0
        func("T.CallDef", 0, true, vec![call(0, "T.Def", &[], &["Std.Boolean"])], Some(0)),
        // %0 = 7; %1 = call T.Id<Std.Int32>(%0); ret %1
        func("T.CallId", 0, true, vec![
            Instruction::ConstI64 { dst: 0, val: 7 },
            call(1, "T.Id", &[0], &["Std.Int32"]),
        ], Some(1)),
        // %0 = 5; %1 = vcall %0.Def<Std.Boolean>(); ret %1
        func("T.VCallDef", 0, true, vec![
            Instruction::ConstI64 { dst: 0, val: 5 },
            vcall(1, 0, "Def", &["Std.Boolean"]),
        ], Some(1)),
        func("T.Fwd", 0, true, vec![call(0, "T.Def", &[], &["$mta:0"])], Some(0)),
        func("T.CallFwd", 0, true, vec![call(0, "T.Fwd", &[], &["Std.Boolean"])], Some(0)),
        func("T.VFwd", 0, true, vec![
            Instruction::ConstI64 { dst: 0, val: 5 },
            vcall(1, 0, "Def", &["$mta:0"]),
        ], Some(1)),
        func("T.CallVFwd", 0, true, vec![call(0, "T.VFwd", &[], &["Std.Boolean"])], Some(0)),
        func("T.CallMissing", 0, true, vec![call(0, "T.Def", &[], &[])], Some(0)),
    ])
}

fn idx(m: &Module, name: &str) -> usize { m.func_index[name] }

fn interp(vm: &VmContext, m: &Module, name: &str) -> Value {
    match crate::jit::test_support::invoke_baseline(vm, m, &m.functions[idx(m, name)], &[]).expect("runs") {
        ExecOutcome::Returned(v) => v.unwrap_or(Value::Null),
        ExecOutcome::Thrown(exc) => panic!("threw {exc:?}"),
    }
}

#[test]
fn type_arg_reads_and_forwarding_compile() {
    let m = test_module();
    assert_eq!(jit_unsupported_reason(&m.functions[idx(&m, "T.CallDef")]), None, "generic Call site");
    assert_eq!(jit_unsupported_reason(&m.functions[idx(&m, "T.VCallDef")]), None, "generic VCall site");
    assert_eq!(jit_unsupported_reason(&m.functions[idx(&m, "T.Id")]), None, "a generic body blind to its type args");
    assert_eq!(jit_unsupported_reason(&m.functions[idx(&m, "T.Def")]), None);
    assert_eq!(jit_unsupported_reason(&m.functions[idx(&m, "T.Fwd")]), None);
    assert_eq!(unsupported_reason(&vcall(0, 0, "M", &["$mta:1"])), None);
}

/// Without a JIT the interpreter threads the site's type args (the baseline both engines
/// must match).
#[test]
fn the_interpreter_threads_a_sites_type_args() {
    let m = test_module();
    let vm = VmContext::new();
    assert_eq!(interp(&vm, &m, "T.CallDef"), Value::Bool(false), "default(T) with T = bool");
    assert_eq!(interp(&vm, &m, "T.VCallDef"), Value::Bool(false));
    assert_eq!(interp(&vm, &m, "T.CallId"), Value::I64(7));
}

#[cfg(feature = "jit")]
fn jit(vm: &VmContext, m: &Module, name: &str) -> Value {
    use crate::jit::runtime::invoke::{call_entry, NativeOutcome};
    let rt = crate::jit::test_support::install(vm, m, z42::config::runtime_config().jit_config()).expect("install");
    for i in 0..m.functions.len() { rt.resolve_fn_by_id(vm, i); }
    let entry = rt.resolve_fn_by_id(vm, idx(m, name)).expect("the caller compiles");
    match unsafe { call_entry(vm, entry, crate::jit::runtime::frame::JitFrame::new(vm, entry.max_reg, &[])) } {
        NativeOutcome::Returned(v) => v.unwrap_or(Value::Null),
        NativeOutcome::Threw => panic!("threw {:?}", vm.exec().take_exception()),
    }
}

/// Both native call helpers supply the type arguments to the callee.
#[cfg(feature = "jit")]
#[test]
fn compiled_static_and_virtual_sites_pass_bound_type_args() {
    let m = test_module();
    let vm = VmContext::new();
    assert_eq!(jit(&vm, &m, "T.CallDef"), Value::Bool(false));
    assert!(crate::jit::compile::state::runtime(&vm).unwrap().resolve_fn_by_id(&vm, idx(&m, "T.Def")).is_some());

    let vm = VmContext::new();
    assert_eq!(jit(&vm, &m, "T.VCallDef"), Value::Bool(false));
    assert!(crate::jit::compile::state::runtime(&vm).unwrap().resolve_fn_by_id(&vm, idx(&m, "Std.Int32.Def")).is_some());
}

#[cfg(feature = "jit")]
#[test]
fn native_calls_bind_forwarded_type_args_and_preserve_missing_args() {
    let m = test_module();
    let vm = VmContext::new();
    assert_eq!(jit(&vm, &m, "T.CallFwd"), Value::Bool(false));
    assert!(crate::jit::compile::state::runtime(&vm).unwrap().resolve_fn_by_id(&vm, idx(&m, "T.Fwd")).is_some());

    let vm = VmContext::new();
    assert_eq!(jit(&vm, &m, "T.CallVFwd"), Value::Bool(false));
    assert!(crate::jit::compile::state::runtime(&vm).unwrap().resolve_fn_by_id(&vm, idx(&m, "T.VFwd")).is_some());

    let vm = VmContext::new();
    assert_eq!(jit(&vm, &m, "T.CallMissing"), Value::Null);
}

#[cfg(feature = "jit")]
#[test]
fn a_compiled_generic_site_calls_a_compiled_callee_natively() {
    let m = test_module();
    let vm = VmContext::new();
    assert_eq!(jit(&vm, &m, "T.CallId"), Value::I64(7));
    let rt = crate::jit::compile::state::runtime(&vm).expect("installed");
    assert!(rt.resolve_fn_by_id(&vm, idx(&m, "T.Id")).is_some(), "the type-arg-blind generic body compiled");
}

/// The interpreter's mixed-mode divert now covers a generic call: with the callee compiled,
/// an interpreted generic site runs it natively (`jit_native_from_interp` counts it).
#[cfg(feature = "jit")]
#[test]
fn the_interpreter_diverts_a_generic_call_to_a_compiled_callee() {
    use std::sync::atomic::Ordering;
    let m = test_module();
    let vm = VmContext::new();
    let rt = crate::jit::test_support::install(&vm, &m, z42::config::runtime_config().jit_config()).expect("install");
    assert!(rt.resolve_fn_by_id(&vm, idx(&m, "T.Id")).is_some(), "callee compiled up front");
    let before = vm.counters().jit_native_from_interp.load(Ordering::Relaxed);
    assert_eq!(interp(&vm, &m, "T.CallId"), Value::I64(7));
    assert_eq!(vm.counters().jit_native_from_interp.load(Ordering::Relaxed), before + 1,
        "the generic call site diverted to the compiled callee");
    let before = vm.counters().jit_native_from_interp.load(Ordering::Relaxed);
    assert!(rt.resolve_fn_by_id(&vm, idx(&m, "T.Def")).is_some());
    assert!(rt.resolve_fn_by_id(&vm, idx(&m, "Std.Int32.Def")).is_some());
    assert_eq!(interp(&vm, &m, "T.CallDef"), Value::Bool(false));
    assert_eq!(interp(&vm, &m, "T.VCallDef"), Value::Bool(false));
    assert_eq!(vm.counters().jit_native_from_interp.load(Ordering::Relaxed), before + 2);
}

#[cfg(feature = "jit")]
#[test]
fn method_parameter_array_compiles_with_bound_and_missing_args() {
    use crate::zpkg::ir::ArrayNewInsn;
    use crate::object::ElemType;

    let allocation = Instruction::ArrayNew(Box::new(ArrayNewInsn {
        dst: 1,
        size: 0,
        elem_tag: 0,
        element_type: String::new(),
        stack_alloc: false,
        type_param_kind: 1,
        type_param_index: 0,
    }));
    let caller = |name, args: &[&str]| func(name, 0, true, vec![
        Instruction::ConstI64 { dst: 0, val: 1 },
        call(1, "T.Allocate", &[0], args),
    ], Some(1));
    let module = module_of(vec![
        func("T.Allocate", 1, true, vec![allocation], Some(1)),
        caller("T.IntArray", &["Std.Int32"]),
        caller("T.MissingArray", &[]),
    ]);
    for (name, default, element_type) in [
        ("T.IntArray", Value::I64(0), ElemType::intern("Std.Int32")),
        ("T.MissingArray", Value::Null, ElemType::empty()),
    ] {
        let vm = VmContext::new();
        for value in [interp(&vm, &module, name), jit(&vm, &module, name)] {
            let Value::Array(array) = value else { panic!("expected array, got {value:?}") };
            let array = array.borrow();
            assert_eq!(array.len(), 1);
            assert_eq!(array.get_boxed(0), default);
            assert_eq!(array.element_type(), element_type);
        }
        assert!(crate::jit::compile::state::runtime(&vm).unwrap().resolve_fn_by_id(&vm, idx(&module, "T.Allocate")).is_some());
    }
}

fn witness_module() -> Module {
    let body = vec![
        Instruction::LoadMethodWitness { dst: 1, slot: 1 },
        Instruction::CallIndirect { dst: 2, callee: 1, args: Box::new([0]) },
    ];
    let invoke = |name, payload: &[&str]| func(name, 0, true, vec![
        Instruction::ConstI64 { dst: 0, val: 7 },
        call(1, "T.SharedWitness", &[0], payload),
    ], Some(1));
    module_of(vec![
        func("T.SharedWitness", 1, true, body, Some(2)),
        func("T.WitnessA", 1, true, vec![], Some(0)),
        func("T.WitnessB", 1, true, vec![Instruction::ConstI64 { dst: 1, val: 99 }], Some(1)),
        invoke("T.UseWitnessA", &["int", "$witness:T.WitnessA"]),
        invoke("T.UseWitnessB", &["int", "$witness:T.WitnessB"]),
        func("T.ForwardWitness", 1, true, vec![
            call(1, "T.SharedWitness", &[0], &["$mta:0", "$mta:1"]),
        ], Some(1)),
        func("T.UseForwardedWitness", 0, true, vec![
            Instruction::ConstI64 { dst: 0, val: 7 },
            call(1, "T.ForwardWitness", &[0], &["int", "$witness:T.WitnessA"]),
        ], Some(1)),
        invoke("T.MissingWitness", &["int"]),
        invoke("T.MalformedWitness", &["int", "T.WitnessA"]),
        invoke("T.UnresolvedWitness", &["int", "$witness:T.Unresolved"]),
    ])
}

#[test]
fn interpreter_loads_the_witness_chosen_for_each_activation() {
    let m = witness_module();
    let vm = VmContext::new();
    assert_eq!(interp(&vm, &m, "T.UseWitnessA"), Value::I64(7));
    assert_eq!(interp(&vm, &m, "T.UseWitnessB"), Value::I64(99));
    assert_eq!(interp(&vm, &m, "T.UseForwardedWitness"), Value::I64(7));
}

#[cfg(feature = "jit")]
#[test]
fn jit_loads_the_witness_chosen_for_each_activation() {
    let m = witness_module();
    let vm = VmContext::new();
    assert_eq!(jit(&vm, &m, "T.UseWitnessA"), Value::I64(7));
    assert!(crate::jit::compile::state::runtime(&vm).unwrap()
        .resolve_fn_by_id(&vm, idx(&m, "T.SharedWitness")).is_some());
    assert_eq!(jit(&VmContext::new(), &m, "T.UseWitnessB"), Value::I64(99));
    assert_eq!(jit(&VmContext::new(), &m, "T.UseForwardedWitness"), Value::I64(7));
}

#[cfg(feature = "jit")]
#[test]
fn malformed_or_missing_witnesses_throw_in_both_engines() {
    use crate::jit::runtime::{frame::JitFrame, invoke::{call_entry, NativeOutcome}};
    let m = witness_module();
    let cases = [
        ("T.MissingWitness", "missing method witness in slot 1"),
        ("T.MalformedWitness", "missing method witness in slot 1"),
        ("T.UnresolvedWitness", "undefined function `T.Unresolved`"),
    ];
    for (name, expected) in cases {
        let vm = VmContext::new();
        let outcome = crate::jit::test_support::invoke_baseline(&vm, &m, &m.functions[idx(&m, name)], &[]).unwrap();
        match outcome {
            ExecOutcome::Thrown(exc) => assert_eq!(exc, Value::Str(expected.into())),
            _ => panic!("expected {name} to throw"),
        }

        let vm = VmContext::new();
        let rt = crate::jit::test_support::install(&vm, &m, z42::config::runtime_config().jit_config()).unwrap();
        rt.resolve_fn_by_id(&vm, idx(&m, "T.SharedWitness")).expect("witness load compiles");
        let entry = rt.resolve_fn_by_id(&vm, idx(&m, name)).expect("caller compiles");
        let outcome = unsafe { call_entry(&vm, entry, JitFrame::new(&vm, entry.max_reg, &[])) };
        assert!(matches!(outcome, NativeOutcome::Threw), "expected {name} to throw");
        assert_eq!(vm.exec().take_exception(), Some(Value::Str(expected.into())));
    }
}

fn method_struct_module() -> Module {
    let invoke = |name, ty: &[&str]| func(name, 0, true, vec![call(0, "T.MethodStruct", &[], ty)], Some(0));
    module_of(vec![
        func("T.MethodStruct", 0, true, vec![
            Instruction::MethodStructAlloc { dst: 0, param_index: 0 },
            Instruction::IsInstance(Box::new(crate::zpkg::ir::IsInstanceInsn {
                dst: 1, obj: 0, class_name: "Demo.SmallBlob".into(),
            })),
        ], Some(1)),
        invoke("T.SmallBlob", &["Demo.SmallBlob"]),
        invoke("T.LargeBlob", &["Demo.LargeBlob"]),
        invoke("T.MissingStructArg", &[]),
        invoke("T.UnknownStructArg", &["Demo.NotLoaded"]),
        invoke("T.ClassStructArg", &["Demo.PlainClass"]),
    ])
}

fn install_method_struct_types(vm: &VmContext) {
    use crate::metadata::ids::tokens::TypeId;
    use crate::object::{StructFieldLayout, StructTypeLayout, TypeDesc, TypeDescCold};
    use crate::zpkg::index::name::NameIndex;
    use std::sync::Arc;
    vm.loader().install_lazy_loader(None, 0);
    let mut loader = vm.core().loader().lazy_loader.write();
    let loader = loader.as_mut().unwrap();
    for (name, size) in [("Demo.SmallBlob", Some(8)), ("Demo.LargeBlob", Some(24)), ("Demo.PlainClass", None)] {
        let td = TypeDesc {
            name: name.into(),
            class_flags: if size.is_some() { crate::zpkg::ir::CLASS_FLAG_STRUCT } else { 0 },
            fields: Vec::new(),
            field_index: NameIndex::new(),
            vtable: Vec::new(),
            vtable_index: NameIndex::new(),
            base_name: None,
            visibility: 0,
            cold: size.map(|size| Box::new(TypeDescCold {
                struct_layout: Some(Arc::new(StructTypeLayout { float_field_ranges: Default::default(),
                    size, ref_offsets: Box::new([]), ref_kinds: Box::new([]),
                    fields: Box::new([StructFieldLayout { offset: size as u32 - 4, size: 4, kind: 0 }]),
                })),
                ..Default::default()
            })),
            id: TypeId::UNRESOLVED,
        };
        loader.insert_type(name.into(), Arc::new(td));
    }
}

#[test]
fn interpreter_allocates_the_current_method_struct_type() {
    let vm = VmContext::new();
    install_method_struct_types(&vm);
    let module = method_struct_module();
    assert_eq!(interp(&vm, &module, "T.SmallBlob"), Value::Bool(true));
    assert_eq!(interp(&vm, &module, "T.LargeBlob"), Value::Bool(false));
}

#[cfg(feature = "jit")]
#[test]
fn jit_allocates_the_current_method_struct_type() {
    let module = method_struct_module();
    for (name, expected) in [("T.SmallBlob", true), ("T.LargeBlob", false)] {
        let vm = VmContext::new();
        install_method_struct_types(&vm);
        assert_eq!(jit(&vm, &module, name), Value::Bool(expected));
        assert!(crate::jit::compile::state::runtime(&vm).unwrap()
            .resolve_fn_by_id(&vm, idx(&module, "T.MethodStruct")).is_some());
    }
}

#[cfg(feature = "jit")]
#[test]
fn method_struct_alloc_rejects_missing_or_unusable_type_arguments_in_both_engines() {
    use crate::jit::runtime::{frame::JitFrame, invoke::{call_entry, NativeOutcome}};
    let module = method_struct_module();
    let cases = [
        ("T.MissingStructArg", "missing method type argument in slot 0"),
        ("T.UnknownStructArg", "unknown struct type `Demo.NotLoaded` for method type argument 0"),
        ("T.ClassStructArg", "method type argument `Demo.PlainClass` has no struct layout"),
    ];
    for (name, expected) in cases {
        let vm = VmContext::new();
        install_method_struct_types(&vm);
        let outcome = crate::jit::test_support::invoke_baseline(&vm, &module, &module.functions[idx(&module, name)], &[]).unwrap();
        assert!(matches!(outcome, ExecOutcome::Thrown(exc) if exc == Value::Str(expected.into())));

        let vm = VmContext::new();
        install_method_struct_types(&vm);
        let rt = crate::jit::test_support::install(&vm, &module, z42::config::runtime_config().jit_config()).unwrap();
        rt.resolve_fn_by_id(&vm, idx(&module, "T.MethodStruct")).expect("struct allocation compiles");
        let entry = rt.resolve_fn_by_id(&vm, idx(&module, name)).expect("caller compiles");
        let outcome = unsafe { call_entry(&vm, entry, JitFrame::new(&vm, entry.max_reg, &[])) };
        assert!(matches!(outcome, NativeOutcome::Threw), "expected {name} to throw");
        assert_eq!(vm.exec().take_exception(), Some(Value::Str(expected.into())));
    }
}

fn dynamic_struct_copy_module() -> Module {
    use crate::object::TAG_I32;
    use crate::zpkg::ir::{StructAllocInsn, StructFieldGetInsn, StructFieldSetInsn};
    let caller = |name: &str, ty: &str, size: u32, value: i32, result_type: &str, result_size: u32| {
        let alloc = |dst, type_name: &str, size| Instruction::StructAlloc(Box::new(StructAllocInsn {
            dst, type_name: type_name.into(), size,
        }));
        func(name, 0, true, vec![
            alloc(0, ty, size),
            alloc(1, result_type, result_size),
            Instruction::ConstI32 { dst: 2, val: value },
            Instruction::StructFieldSetPrim(Box::new(StructFieldSetInsn {
                base: 0, root_type: ty.into(), path: Box::new([0]), kind: TAG_I32, val: 2,
            })),
            call(3, "T.CopyStruct", &[0, 1], &[ty]),
            Instruction::StructFieldGetPrim(Box::new(StructFieldGetInsn {
                dst: 4, base: 1, root_type: result_type.into(), path: Box::new([0]), kind: TAG_I32,
            })),
        ], Some(4))
    };
    module_of(vec![
        func("T.CopyStruct", 2, true, vec![
            Instruction::MethodStructAlloc { dst: 2, param_index: 0 },
            Instruction::StructCopyDynamic { dst: 2, src: 0 },
            Instruction::StructCopyDynamic { dst: 1, src: 2 },
        ], None),
        caller("T.CopySmall", "Demo.SmallBlob", 8, 17, "Demo.SmallBlob", 8),
        caller("T.CopyLarge", "Demo.LargeBlob", 24, 31, "Demo.LargeBlob", 24),
        caller("T.CopyMismatch", "Demo.SmallBlob", 8, 42, "Demo.LargeBlob", 24),
    ])
}

#[test]
fn interpreter_executes_dynamic_struct_copy_across_frames() {
    let module = dynamic_struct_copy_module();
    let vm = VmContext::new();
    install_method_struct_types(&vm);
    assert_eq!(interp(&vm, &module, "T.CopySmall"), Value::I64(17));
    assert_eq!(interp(&vm, &module, "T.CopyLarge"), Value::I64(31));
    let function = &module.functions[idx(&module, "T.CopyMismatch")];
    let outcome = crate::jit::test_support::invoke_baseline(&vm, &module, function, &[]).unwrap();
    assert!(matches!(outcome, ExecOutcome::Thrown(Value::Str(s)) if s.starts_with("StructCopyDynamic: incompatible layouts")));
}

#[cfg(feature = "jit")]
#[test]
fn jit_executes_dynamic_struct_copy_across_frames() {
    use crate::jit::runtime::{frame::JitFrame, invoke::{call_entry, NativeOutcome}};
    let module = dynamic_struct_copy_module();
    let vm = VmContext::new();
    install_method_struct_types(&vm);
    let rt = crate::jit::test_support::install(&vm, &module, z42::config::runtime_config().jit_config()).unwrap();
    rt.resolve_fn_by_id(&vm, idx(&module, "T.CopyStruct")).expect("shared copy compiles");
    for (name, expected) in [("T.CopySmall", 17), ("T.CopyLarge", 31)] {
        let entry = rt.resolve_fn_by_id(&vm, idx(&module, name)).expect("caller compiles");
        let outcome = unsafe { call_entry(&vm, entry, JitFrame::new(&vm, entry.max_reg, &[])) };
        assert!(matches!(outcome, NativeOutcome::Returned(Some(Value::I64(actual))) if actual == expected));
    }
    let entry = rt.resolve_fn_by_id(&vm, idx(&module, "T.CopyMismatch")).expect("caller compiles");
    let outcome = unsafe { call_entry(&vm, entry, JitFrame::new(&vm, entry.max_reg, &[])) };
    assert!(matches!(outcome, NativeOutcome::Threw));
    assert!(matches!(vm.exec().take_exception(), Some(Value::Str(s)) if s.starts_with("StructCopyDynamic: incompatible layouts")));
}

#[cfg(feature = "jit")]
#[test]
fn native_vcall_keeps_the_source_interface() {
    use crate::jit::runtime::frame::JitFrame;
    use crate::jit::runtime::invoke::{NativeOutcome, call_entry};
    use crate::object::{InterfaceRef, NativeData, TypeDesc};
    use z42::object::tokens::{TypeId, alloc_type_id_block};

    let module = module_of(vec![
        func("Sink.Put", 1, false, vec![Instruction::ConstI32 { dst: 1, val: 1 }], Some(1)),
        func("Sink.PutCat", 1, false, vec![Instruction::ConstI32 { dst: 1, val: 2 }], Some(1)),
        func("Call", 1, true, vec![vcall(1, 0, "ISink<@Animal>#Put", &[])], Some(1)),
    ]);
    let vm = VmContext::new();
    let rt = crate::jit::test_support::install(&vm, &module, z42::config::runtime_config().jit_config())
        .expect("install");
    let entry = rt.resolve_fn_by_id(&vm, idx(&module, "Call")).expect("the caller compiles");
    let td = std::sync::Arc::new(TypeDesc {
        id: TypeId(alloc_type_id_block(1)),
        name: "Sink".to_owned(),
        class_flags: 0,
        visibility: 0,
        base_name: None,
        fields: Vec::new(),
        field_index: Default::default(),
        vtable: vec![("Put".to_owned(), "Sink.Put".to_owned()),
                     ("PutCat".to_owned(), "Sink.PutCat".to_owned())],
        vtable_index: [("Put".to_owned(), 0), ("PutCat".to_owned(), 1),
                       ("ISink<@Animal>#Put".to_owned(), 0), ("ISink<@Cat>#Put".to_owned(), 1),
                       ("ISink#Put".to_owned(), 0)].into_iter().collect(),
        cold: None,
    });
    let object = vm.heap().alloc_object(td, Vec::new(), NativeData::None);
    let Value::Object(receiver) = object else { panic!("object") };
    let source = vm.heap().alloc_str("ISink<@Cat>");
    let view = Value::InterfaceRef(InterfaceRef::new(vm.heap(), receiver, source).unwrap());

    for (value, expected) in [(object, 1), (view, 2), (object, 1), (view, 2)] {
        let args = [value];
        match unsafe { call_entry(&vm, entry, JitFrame::new(&vm, entry.max_reg, &args)) } {
            NativeOutcome::Returned(Some(Value::I64(n))) => assert_eq!(n, expected),
            NativeOutcome::Threw => panic!("threw {:?}", vm.exec().take_exception()),
            _ => panic!("expected an integer result"),
        }
    }
}
