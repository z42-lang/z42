//! Shared images must never publish another VM's identities or cache cells.
use super::*;
use crate::metadata::image::PackageImage;
use crate::metadata::ir::{
    self, BasicBlock, ClassDesc, ExceptionEntry, FunctionCold, Instruction, Terminator,
};
use crate::vm::runtime::ids::tokens::FnId;
use crate::vm_context::VmContext;
use std::sync::Arc;

fn definition(name: &str, instructions: Vec<Instruction>) -> ir::Function {
    ir::Function {
        name: name.into(),
        param_count: 0,
        ret_type: "long".into(),
        exec_mode: ir::ExecMode::Interp,
        blocks: vec![BasicBlock {
            label: "entry".into(),
            instructions,
            terminator: Terminator::Ret { reg: Some(0) },
        }],
        is_static: true,
        visibility: 0,
        method_flags: 0,
        min_arg: 0,
        params_from: 0xff,
        max_reg: 0,
        cold: None,
        reg_types: Box::new([]),
        block_index: Default::default(),
        branch_targets: Vec::new(),
        frame_meta: None,
    }
}

fn image() -> Arc<PackageImage> {
    let mut probe = definition(
        "Pkg.Probe",
        vec![
            Instruction::IsInstance(Box::new(ir::IsInstanceInsn {
                dst: 1,
                obj: 0,
                class_name: "Pkg.C".into(),
            })),
            Instruction::AsCast(Box::new(ir::AsCastInsn {
                dst: 2,
                obj: 0,
                class_name: "Pkg.C".into(),
            })),
            Instruction::ArrayNewLit(Box::new(ir::ArrayNewLitInsn {
                dst: 3,
                elems: Box::new([]),
                element_type: "long".into(),
                stack_alloc: false,
            })),
            Instruction::ObjNew(Box::new(ir::ObjNewInsn {
                dst: 4,
                class_name: "Pkg.C".into(),
                ctor_name: String::new(),
                args: Box::new([]),
                type_args: Box::new(["long".into()]),
                stack_alloc: false,
                ctor_known: false,
            })),
        ],
    );
    probe.cold = Some(Box::new(FunctionCold {
        exception_table: Box::new([ExceptionEntry {
            try_start: "entry".into(),
            try_end: "exit".into(),
            catch_label: "catch".into(),
            catch_type: Some("Pkg.C".into()),
            catch_reg: 5,
        }]),
        ..Default::default()
    }));
    let class = ClassDesc {
        name: "Pkg.C".into(),
        base_class: None,
        fields: Box::new([]),
        type_params: Box::new([]),
        type_param_constraints: Box::new([]),
        attributes: Box::new([]),
        class_flags: 0,
        visibility: 0,
        class_flags2: 0,
        struct_field_table: Box::new([]),
        iface_slots: Box::new([]),
        static_fields: Box::new([]),
        interfaces: Box::new([]),
        enum_members: Box::new([]),
        iface_methods: Box::new([]),
        struct_layout: None,
        inline_layout: None,
        object_layout: None,
    };
    PackageImage::new(ir::Module {
        name: "Pkg".into(),
        string_pool: vec!["original".into()],
        classes: vec![class],
        functions: vec![
            definition("Pkg.Main", vec![Instruction::ConstI64 { dst: 0, val: 42 }]),
            probe,
        ],
        func_index: Default::default(),
    })
}

#[test]
fn two_vms_share_definitions_and_isolate_all_link_state() {
    let image = image();
    let a = VmContext::with_image(image.clone()).unwrap();
    let b = VmContext::with_image(image.clone()).unwrap();
    let am = a.module().unwrap();
    let bm = b.module().unwrap();
    for i in 0..2 {
        assert!(std::ptr::eq(
            am.functions[i].definition(),
            image.function(i).unwrap()
        ));
        assert!(std::ptr::eq(
            am.functions[i].definition(),
            bm.functions[i].definition()
        ));
        assert_eq!(am.functions[i].id.get(), Some(FnId(i as u32)));
        assert_eq!(bm.functions[i].id.get(), Some(FnId(i as u32)));
    }
    let at = &am.type_registry["Pkg.C"];
    let bt = &bm.type_registry["Pkg.C"];
    assert!(!Arc::ptr_eq(at, bt));
    assert_ne!(at.id, bt.id);
    let af = &am.functions[1];
    let bf = &bm.functions[1];
    af.type_key(0, 0).set(17);
    af.catch_key(0).set(18);
    af.forward.set(Some(19));
    af.owner_init.set(Some(at.clone())).unwrap();
    crate::vm::runtime::resolve::resolve_function_tokens(&am.functions[0], am, &a);
    assert_eq!(bf.type_key(0, 0).get(), None);
    assert_eq!(af.type_key(0, 1).get(), None, "is/as have distinct cells");
    assert_eq!(bf.catch_key(0).get(), None);
    assert_eq!(bf.forward.get(), None);
    assert!(bf.owner_init.get().is_none());
    assert!(bm.functions[0].resolved.get().is_none());
    assert_eq!(&*af.array_type(0, 2), "long");
    assert_eq!(af.object_args(0, 3).as_slice(), &["long"]);
    assert!(!std::ptr::eq(af.type_key(0, 0), bf.type_key(0, 0)));
    assert!(!std::ptr::eq(af.catch_key(0), bf.catch_key(0)));
    for (ctx, module) in [(&a, am), (&b, bm)] {
        let value = crate::interp::run_returning(ctx, module, &module.functions[0], &[]).unwrap();
        assert!(matches!(value, Some(crate::object::Value::I64(42))));
    }
    let worker = VmContext::new_with_core(a.core_arc());
    assert!(Arc::ptr_eq(worker.module().unwrap(), am));
    assert_eq!(
        worker.module().unwrap().functions[1].catch_key(0).get(),
        Some(18)
    );
}

#[test]
fn the_same_image_can_register_at_different_function_ids() {
    use crate::vm::runtime::ids::func_table::FuncTable;
    let image = image();
    let module = Arc::new(Module::from_image(image.clone()));
    let entry = FuncTable::new(Some(module.clone()));
    let lazy = FuncTable::new(None);
    lazy.register_lazy(&Arc::new(Function::new(definition("Other", vec![]))));
    let mut second = Module::from_image(image);
    let function = Arc::new(second.functions.remove(0));
    assert_eq!(lazy.register_lazy(&function), Some(FnId(1)));
    assert_eq!(module.functions[0].id.get(), Some(FnId(0)));
    assert_eq!(entry.id_of("Pkg.Main"), Some(FnId(0)));
    assert!(std::ptr::eq(
        function.definition(),
        module.functions[0].definition()
    ));
}

#[test]
fn link_time_edits_preserve_old_images_and_update_new_snapshots() {
    let original = image();
    let mut module = Module::from_image(original.clone());
    module.functions[1].type_key(0, 0).set(5);
    module.name = "Changed".into();
    module.string_pool[0] = "new".into();
    module.functions[0].blocks[0].instructions[0] = Instruction::ConstI64 { dst: 0, val: 99 };
    module.functions[1].blocks[0].instructions.clear();
    module.functions[0].name = "Changed.Main".into();
    let changed = module.image();
    assert_eq!(original.name, "Pkg");
    assert_eq!(original.string_pool[0], "original");
    assert_eq!(changed.name, "Changed");
    assert_eq!(changed.func_index.get("Changed.Main"), Some(&0));
    assert!(!changed.func_index.contains_key("Pkg.Main"));
    assert_eq!(
        &*changed.function(0).unwrap().frame_name_file().0,
        "Changed.Main()"
    );
    assert_eq!(changed.string_pool[0], "new");
    assert!(matches!(
        original.function(0).unwrap().blocks[0].instructions[0],
        Instruction::ConstI64 { val: 42, .. }
    ));
    assert!(matches!(
        changed.function(0).unwrap().blocks[0].instructions[0],
        Instruction::ConstI64 { val: 99, .. }
    ));
    assert_eq!(
        original.function(1).unwrap().blocks[0].instructions.len(),
        4
    );
    assert!(changed.function(1).unwrap().blocks[0]
        .instructions
        .is_empty());
    assert_eq!(
        Module::from_image(original).functions[1]
            .type_key(0, 0)
            .get(),
        None
    );
}

#[test]
#[should_panic(expected = "a linked function belongs to one VM")]
fn a_linked_function_cannot_be_shared_between_function_tables() {
    use crate::vm::runtime::ids::func_table::FuncTable;
    let function = Arc::new(Function::new(definition("F", vec![])));
    FuncTable::new(None).register_lazy(&function);
    FuncTable::new(None).register_lazy(&function);
}

#[test]
fn debug_overlay_reprepares_static_metadata_without_mutating_the_image() {
    let original = image();
    let mut module = Module::from_image(original.clone());
    crate::metadata::loader::build_block_indices(&mut module);
    assert_eq!(module.functions[0].branch_targets.len(), 1);
    let line = crate::metadata::debug::LineEntry {
        block: 0,
        instr: 0,
        line: 10,
        column: 1,
        file: Some("overlay.z42".into()),
    };
    module.functions[0].cold_mut().line_table = Box::new([line]);
    assert!(module.functions[0].branch_targets.is_empty());
    crate::metadata::loader::build_block_indices(&mut module);
    assert_eq!(module.functions[0].branch_targets.len(), 1);
    assert_eq!(module.functions[0].fused_tails.len(), 1);
    assert_eq!(module.functions[0].max_reg, 1);
    assert_eq!(&*module.functions[0].frame_name_file().1, "overlay.z42");
    assert!(original.function(0).unwrap().line_table().is_empty());
}
