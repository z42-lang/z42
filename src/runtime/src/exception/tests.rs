use super::*;

use crate::metadata::bytecode::{FunctionCold, LineEntry};

fn line(block: u32, instr: u32, line: u32, column: u32, file: Option<&str>) -> LineEntry {
    LineEntry { block, instr, line, column, file: file.map(str::to_string) }
}

/// A hand-built function (no loader post-processing → `frame_meta: None`)
/// with the given parameter types and line table. Shared with other tests
/// that need a function for a `VmFrame` to point at.
pub(crate) fn test_function(name: &str, param_types: &[&str], line_table: Vec<LineEntry>) -> Function {
    Function {
        name: name.to_string(),
        param_count: param_types.len(),
        ret_type: "void".to_string(),
        exec_mode: crate::metadata::types::ExecMode::Interp,
        blocks: Vec::new(),
        is_static: true,
        visibility: 0,
        method_flags: 0, min_arg: 0, params_from: 0xFF,
        max_reg: 0,
        cold: Some(Box::new(FunctionCold {
            param_types: param_types.iter().map(|t| t.to_string()).collect(),
            line_table: line_table.into_boxed_slice(),
            ..Default::default()
        })),
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
fn vm_frame_stays_thin() {
    // One VmFrame is pushed per call: func + regs + env_arena + pc + 4 u32 bases.
    assert!(std::mem::size_of::<VmFrame>() <= 48, "VmFrame is {} B", std::mem::size_of::<VmFrame>());
}

#[test]
fn snapshot_resolves_position_from_pc() {
    let func = test_function("Foo", &["int", "string"], vec![
        line(0, 0, 3, 5, Some("f.z42")),
        line(0, 4, 7, 13, Some("f.z42")),
        line(1, 0, 9, 2, Some("f.z42")),
    ]);
    let f = VmFrame::new(&func, std::ptr::null(), std::ptr::null());
    f.pc.set(func.linear_offset(0, 6)); // inside the entry starting at (0, 4)
    let snap = f.snapshot();
    f.pc.set(func.linear_offset(1, 0)); // mutates after snapshot
    assert_eq!((snap.line, snap.column), (7, 13));
    assert_eq!(snap.offset, 6);
    assert_eq!(&*snap.func_name, "Foo(int,string)");
    assert_eq!(&*snap.file, "f.z42");
    assert_eq!(f.line_col(), (9, 2));
}

#[test]
fn snapshot_of_unstamped_frame_has_no_position() {
    let func = test_function("Init", &[], vec![line(0, 0, 3, 5, Some("f.z42"))]);
    let snap = VmFrame::new(&func, std::ptr::null(), std::ptr::null()).snapshot();
    assert_eq!((snap.line, snap.column, snap.offset), (0, 0, PC_UNSET));
    assert_eq!(format_stack_trace(&[snap]), "  at Init() (f.z42)");
}

#[test]
fn snapshot_of_stripped_frame_keeps_offset() {
    // No line table (release-stripped): only the offset survives → `+0x<offset>`.
    let func = test_function("Std.List.Add", &["?"], Vec::new());
    let f = VmFrame::new(&func, std::ptr::null(), std::ptr::null());
    f.pc.set(func.linear_offset(0, 0x2c));
    assert_eq!(format_stack_trace(&[f.snapshot()]), "  at Std.List.Add(?) +0x2c");
}

#[test]
fn snapshot_prefers_precomputed_frame_meta() {
    let mut func = test_function("Foo", &["int"], vec![line(0, 0, 3, 5, Some("f.z42"))]);
    func.frame_meta = Some(("Foo(int)".into(), "f.z42".into()));
    let (name, file) = func.frame_name_file();
    assert!(std::sync::Arc::ptr_eq(&name, &func.frame_meta.as_ref().unwrap().0));
    assert_eq!((&*name, &*file), ("Foo(int)", "f.z42"));
}

#[test]
fn format_orders_caller_to_throw_last() {
    // call_stack pushed in chrono order: Main → A → B (B is the throwing frame)
    let frames = vec![
        FrameSnapshot { func_name: "Main".into(), file: "f.z42".into(), line: 3,  column: 9, offset: u32::MAX },
        FrameSnapshot { func_name: "A".into(),    file: "f.z42".into(), line: 7,  column: 5, offset: u32::MAX },
        FrameSnapshot { func_name: "B".into(),    file: "f.z42".into(), line: 12, column: 1, offset: u32::MAX },
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
        FrameSnapshot { func_name: "Foo".into(), file: "f.z42".into(), line: 5, column: 0, offset: u32::MAX },
    ];
    assert_eq!(format_stack_trace(&frames), "  at Foo (f.z42:5)");
}

#[test]
fn format_omits_file_when_empty() {
    let frames = vec![
        FrameSnapshot { func_name: "Anon".into(), file: "".into(), line: 5, column: 8, offset: u32::MAX },
    ];
    assert_eq!(format_stack_trace(&frames), "  at Anon (line 5, col 8)");
}

#[test]
fn format_omits_line_when_zero() {
    let frames = vec![
        FrameSnapshot { func_name: "Init".into(), file: "f.z42".into(), line: 0, column: 0, offset: u32::MAX },
    ];
    assert_eq!(format_stack_trace(&frames), "  at Init (f.z42)");
}

#[test]
fn format_handles_no_position_info() {
    let frames = vec![
        FrameSnapshot { func_name: "Bare".into(), file: "".into(), line: 0, column: 0, offset: u32::MAX },
    ];
    assert_eq!(format_stack_trace(&frames), "  at Bare");
}

// add-offline-symbolication: a release-stripped frame (no line table → line 0,
// file empty) but with a recorded code offset prints `+0x<offset>` — the
// offline-resolvable key that `z42d symbolicate` maps back to file:line:col.
#[test]
fn format_emits_offset_when_line_stripped() {
    let frames = vec![
        FrameSnapshot { func_name: "Std.List.Add".into(), file: "".into(), line: 0, column: 0, offset: 0x2c },
        FrameSnapshot { func_name: "Program.Main".into(), file: "".into(), line: 0, column: 0, offset: 0x10 },
    ];
    // Caller-to-throw order (throwing frame last); each stripped frame carries +0x.
    assert_eq!(
        format_stack_trace(&frames),
        "  at Program.Main +0x10\n  at Std.List.Add +0x2c"
    );
}

// Line info present (debug / sidecar merged) must still win over offset — the
// offset branch only fires when there is no resolved line.
#[test]
fn format_prefers_line_over_offset() {
    let frames = vec![
        FrameSnapshot { func_name: "Foo".into(), file: "f.z42".into(), line: 5, column: 9, offset: 0x2c },
    ];
    assert_eq!(format_stack_trace(&frames), "  at Foo (f.z42:5:9)");
}

// ── 2026-05-11 retire-z-codes: make_stdlib_exception ────────────────────────

#[cfg(test)]
mod make_stdlib_exception_tests {
    use super::*;
    use crate::metadata::bytecode::Module;
    use crate::metadata::tokens::TypeId;
    use crate::metadata::types::FieldSlot;
    use crate::vm_context::VmContext;
    
    use std::sync::Arc;

    fn empty_module() -> Module {
        Module {
            name: "test".into(),
            string_pool: vec![],
            classes: vec![],
            functions: vec![],
            type_registry: rustc_hash::FxHashMap::default(),
            func_index: rustc_hash::FxHashMap::default(),
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
            visibility: 0,
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
