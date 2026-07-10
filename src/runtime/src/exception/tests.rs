use super::*;

#[test]
fn snapshot_freezes_line_and_column() {
    let f = FrameInfo::new("Foo".into(), "f.z42".into(), std::ptr::null(), std::ptr::null());
    f.line.set(7);
    f.column.set(13);
    let snap = f.snapshot();
    f.line.set(99);     // mutates after snapshot
    f.column.set(40);
    assert_eq!(snap.line,   7);
    assert_eq!(snap.column, 13);
    assert_eq!(&*snap.func_name, "Foo");
    assert_eq!(&*snap.file, "f.z42");
}

#[test]
fn format_orders_caller_to_throw_last() {
    // call_stack pushed in chrono order: Main → A → B (B is the throwing frame)
    let frames = vec![
        FrameSnapshot { func_name: "Main".into(), file: "f.z42".into(), line: 3,  column: 9 },
        FrameSnapshot { func_name: "A".into(),    file: "f.z42".into(), line: 7,  column: 5 },
        FrameSnapshot { func_name: "B".into(),    file: "f.z42".into(), line: 12, column: 1 },
    ];
    let out = format_stack_trace(&frames);
    let lines: Vec<&str> = out.lines().collect();
    // First line (most recent / throwing frame) is B
    assert_eq!(lines[0], "  at B (f.z42:12:1)");
    assert_eq!(lines[1], "  at A (f.z42:7:5)");
    assert_eq!(lines[2], "  at Main (f.z42:3:9)");
}

#[test]
fn format_drops_column_when_zero() {
    // zbc < 1.1 (or hand-rolled IR) → column = 0 → degrade to (file:line).
    let frames = vec![
        FrameSnapshot { func_name: "Foo".into(), file: "f.z42".into(), line: 5, column: 0 },
    ];
    assert_eq!(format_stack_trace(&frames), "  at Foo (f.z42:5)");
}

#[test]
fn format_omits_file_when_empty() {
    let frames = vec![
        FrameSnapshot { func_name: "Anon".into(), file: "".into(), line: 5, column: 8 },
    ];
    assert_eq!(format_stack_trace(&frames), "  at Anon (line 5, col 8)");
}

#[test]
fn format_omits_line_when_zero() {
    let frames = vec![
        FrameSnapshot { func_name: "Init".into(), file: "f.z42".into(), line: 0, column: 0 },
    ];
    assert_eq!(format_stack_trace(&frames), "  at Init (f.z42)");
}

#[test]
fn format_handles_no_position_info() {
    let frames = vec![
        FrameSnapshot { func_name: "Bare".into(), file: "".into(), line: 0, column: 0 },
    ];
    assert_eq!(format_stack_trace(&frames), "  at Bare");
}

// ── 2026-05-11 retire-z-codes: make_stdlib_exception ────────────────────────

#[cfg(test)]
mod make_stdlib_exception_tests {
    use super::*;
    use crate::metadata::bytecode::Module;
    use crate::metadata::tokens::TypeId;
    use crate::metadata::types::FieldSlot;
    use crate::vm_context::VmContext;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn empty_module() -> Module {
        Module {
            name: "test".into(),
            string_pool: vec![],
            classes: vec![],
            functions: vec![],
            type_registry: HashMap::new(),
            type_registry_vec: Vec::new(),
            func_index: HashMap::new(),
            func_ref_cache_slots: 0,
            interned_strings: Vec::new(),
        }
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
            name:                   name.into(),
            id:                     TypeId::UNRESOLVED,
            base_name:              base.map(str::to_owned),
            fields,
            field_index,
            vtable:                 vec![],
            vtable_index:           crate::metadata::NameIndex::new(),
            cold: Some(Box::new(crate::metadata::types::TypeDescCold {
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
        assert_eq!(read_message(&val, &module).as_deref(), Some("boom"));
        assert!(read_stack_trace(&val, &module).is_none(),
            "StackTrace must stay null until populate_stack_trace runs at throw site");

        // populate_stack_trace fills the field given the current (empty) call
        // stack. Even with zero frames the resulting trace string is empty —
        // important: the field becomes a non-null Str so re-throws don't
        // overwrite it.
        populate_stack_trace(&val, &ctx, &module);
        let trace = read_stack_trace(&val, &module);
        assert!(trace.is_some() || matches!(&val, Value::Object(rc)
            if matches!(rc.borrow().slots[1], Value::Str(_))),
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
