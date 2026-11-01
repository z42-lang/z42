//! Source-facing frame names, shared by traces and the signal-safe writer.

use crate::metadata::ir::Function;

/// Format a function's stack-trace display name with parameter signature.
/// Returns `<name>(<t1>,<t2>,...)` (e.g. `Demo.Greeter.greet(str)`).
/// Empty signature is `<name>()`. Stack traces show it so overloads are
/// distinguishable (1.3 split-debug-symbols Phase 4).
pub fn format_frame_name(func: &Function) -> String {
    let mut out = String::with_capacity(func.name.len() + 2 + func.param_count * 4);
    for_each_frame_name_piece(func, |piece| out.push_str(piece));
    out
}

/// The pieces of [`format_frame_name`], in order, without allocating — the
/// crash-signal handler writes them straight to the fd.
pub fn for_each_frame_name_piece(func: &Function, mut emit: impl FnMut(&str)) {
    emit(&func.name);
    emit("(");
    for (i, t) in func.param_types().iter().enumerate() {
        if i > 0 { emit(","); }
        emit(t);
    }
    // When SIGS lacks per-param types (older artifacts or null source), fall
    // back to "?" placeholders matching `param_count` so the shape is
    // recognizable.
    if func.param_types().is_empty() && func.param_count > 0 {
        for i in 0..func.param_count {
            if i > 0 { emit(","); }
            emit("?");
        }
    }
    emit(")");
}

/// Source file of a function's frames: the file of its line table's first
/// entry, empty when the emitter omitted it.
pub fn frame_file(func: &Function) -> &str {
    func.line_table().first().and_then(|e| e.file.as_deref()).unwrap_or("")
}

