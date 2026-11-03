use super::*;
use crate::vm::runtime::Function;
use crate::vm::exec::call::frames::{VmFrame, PC_UNSET};

use crate::metadata::ir::FunctionCold;
use crate::metadata::debug::LineEntry;

fn line(block: u32, instr: u32, line: u32, column: u32, file: Option<&str>) -> LineEntry {
    LineEntry { block, instr, line, column, file: file.map(str::to_string) }
}

/// A hand-built function (no loader post-processing → `frame_meta: None`)
/// with the given parameter types and line table. Shared with other tests
/// that need a function for a `VmFrame` to point at.
pub(crate) fn test_function(name: &str, param_types: &[&str], line_table: Vec<LineEntry>) -> Function {
    Function::new(crate::metadata::ir::Function {
        name: name.to_string(),
        param_count: param_types.len(),
        ret_type: "void".to_string(),
        exec_mode: crate::metadata::ir::ExecMode::Interp,
        blocks: Vec::new(),
        is_static: true,
        visibility: 0,
        method_flags: 0,
        min_arg: 0,
        params_from: 0xFF,
        max_reg: 0,
        cold: Some(Box::new(FunctionCold {
            param_types: param_types.iter().map(|t| t.to_string()).collect(),
            line_table: line_table.into_boxed_slice(),
            ..Default::default()
        })),
        reg_types: Box::new([]),
        block_index: std::collections::HashMap::new(),
        branch_targets: Vec::new(),
        frame_meta: None,
    })
}

#[test]
fn vm_frame_stays_thin() {
    // One VmFrame is pushed per call: func + regs + pc + 4 u32 bases.
    assert!(std::mem::size_of::<VmFrame>() <= 40, "VmFrame is {} B", std::mem::size_of::<VmFrame>());
}

#[test]
fn snapshot_resolves_position_from_pc() {
    let func = test_function("Foo", &["int", "string"], vec![
        line(0, 0, 3, 5, Some("f.z42")),
        line(0, 4, 7, 13, Some("f.z42")),
        line(1, 0, 9, 2, Some("f.z42")),
    ]);
    let f = VmFrame::new(&func, std::ptr::null());
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
    let snap = VmFrame::new(&func, std::ptr::null()).snapshot();
    assert_eq!((snap.line, snap.column, snap.offset), (0, 0, PC_UNSET));
    assert_eq!(format_stack_trace(&[snap]), "  at Init() (f.z42)");
}

#[test]
fn snapshot_of_stripped_frame_keeps_offset() {
    // No line table (release-stripped): only the offset survives → `+0x<offset>`.
    let func = test_function("Std.List.Add", &["?"], Vec::new());
    let f = VmFrame::new(&func, std::ptr::null());
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


#[test]
fn register_roots_keep_frame_and_register_order() {
    let a = vec![Value::I64(7), Value::Null];
    let b = vec![Value::Bool(true)];
    let frames = [
        VmFrame::new(std::ptr::null(), &a),
        VmFrame::new(std::ptr::null(), &b),
    ];
    let mut roots = Vec::new();
    // SAFETY: this owner keeps both register files and records alive for the walk.
    unsafe { visit_roots(&frames, |v| roots.push(*v)); }
    assert_eq!(roots.len(), 3);
    assert!(matches!(roots[0], Value::I64(7)));
    assert!(matches!(roots[1], Value::Null));
    assert!(matches!(roots[2], Value::Bool(true)));
}

#[test]
fn empty_stack_has_no_roots_or_snapshots() {
    // SAFETY: no records to dereference.
    unsafe { visit_roots(&[], |_| panic!("an empty stack has no roots")); }
    assert!(snapshots(&[]).is_empty());
}
