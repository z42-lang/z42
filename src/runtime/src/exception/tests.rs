use super::*;

// ──  retire-z-codes: make_stdlib_exception ────────────────────────

#[cfg(test)]
mod make_stdlib_exception_tests {
    use super::*;
    use crate::vm::runtime::Module;
    use crate::vm::runtime::ids::tokens::TypeId;
    use crate::object::FieldSlot;
    use crate::vm_context::VmContext;
    
    use std::sync::Arc;

    fn empty_module() -> Module {
        Module::new(crate::metadata::image::ModuleData {
            name: "test".into(),
            string_pool: vec![],
            classes: vec![],
            func_index: rustc_hash::FxHashMap::default(),
        }, vec![], rustc_hash::FxHashMap::default())
    }

    fn exception_type_desc(name: &str, base: Option<&str>) -> Arc<TypeDesc> {
        let fields = vec![
            FieldSlot { name: "Message".into(),        type_tag: "str".into(), visibility: 0 },
            FieldSlot { name: "StackTrace".into(),     type_tag: "str".into(), visibility: 0 },
            FieldSlot { name: "InnerException".into(), type_tag: "Std.Exception".into(), visibility: 0 },
        ];
        let mut field_index = crate::metadata::NameIndex::new();
        for (i, f) in fields.iter().enumerate() {
            field_index.insert(f.name.to_string(), i);
        }
        let own_fields_box: Box<[FieldSlot]> = fields.clone().into();
        Arc::new(TypeDesc {
            class_flags: 0,
            visibility: 0,
            name:                   name.into(),
            id:                     TypeId::UNRESOLVED,
            base_name:              base.map(str::to_owned),
            fields,
            field_index,
            vtable:                 vec![],
            vtable_index:           crate::metadata::NameIndex::new(),
            cold: Some(Box::new(crate::object::TypeDescCold {
                own_fields: own_fields_box,
                ..Default::default()
            })),
        })
    }

    #[test]
    fn make_invalid_marshal_exception_sets_message_and_leaves_trace_null() {
        let mut module = empty_module();
        module.type_registry.insert(
            "Std.Exception".into(), exception_type_desc("Std.Exception", None));
        module.type_registry.insert(
            "Std.InvalidMarshalException".into(),
            exception_type_desc("Std.InvalidMarshalException", Some("Std.Exception")));
        let ctx = VmContext::new();

        let val = make_stdlib_exception(
            &ctx, &module, "Std.InvalidMarshalException", "boom".into(),
        ).expect("constructs");

        // Helper paths (read_message / read_stack_trace) drive the assertion
        // so the test exercises the same surface a real throw site would.
        assert_eq!(read_message(&val, &ctx, &module).as_deref(), Some("boom"));
        assert!(read_stack_trace(&val, &ctx, &module).is_none(),
            "StackTrace must stay null until populate_stack_trace runs at throw site");

        // populate_stack_trace fills the field given the current (empty) call
        // stack. Even with zero frames the resulting trace string is empty —
        // important: the field becomes a non-null Str so re-throws don't
        // overwrite it.
        populate_stack_trace(&val, &ctx, &module);
        let trace = read_stack_trace(&val, &ctx, &module);
        assert!(trace.is_some() || matches!(&val, Value::Object(rc)
            if matches!(rc.borrow().field_value(1), Value::Str(_))),
            "StackTrace populated as Value::Str (even if empty for an empty call stack)");
    }

    #[test]
    fn make_stdlib_exception_errors_when_type_not_registered() {
        let module = empty_module();
        let ctx = VmContext::new();
        let err = make_stdlib_exception(
            &ctx, &module, "Std.InvalidMarshalException", "any".into(),
        ).expect_err("stdlib not loaded → fallback");
        assert!(err.to_string().contains("Std.InvalidMarshalException"),
            "err = {err}");
        assert!(err.to_string().contains("not loaded") || err.to_string().contains("no `Message`"),
            "err = {err}");
    }
}
