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


/// Resolve `(line, column)` covering a given (block, instr) site by walking
/// the function's line table forward to the latest entry that doesn't
/// overshoot. Column is 0 when the source position predates zbc 1.1 or the
/// emitter didn't capture it (gracefully degraded by `format_stack_trace`
/// — `(file:line)` instead of `(file:line:col)`).
pub(crate) fn resolve_line(table: &[crate::metadata::debug::LineEntry], block: u32, instr: u32) -> (u32, u32) {
    // S2a (perf-interp-hot-paths): the line table is emitted in non-decreasing
    // (block, instr) order — the previous linear forward-scan already relied on
    // that (it `break`s on the first overshoot). Binary-search the last entry
    // whose (block, instr) <= the target site: O(log n) instead of O(n) per
    // Call / VCall / throw. Behaviour is byte-identical to the linear scan for a
    // sorted table (which is the only shape the emitter produces).
    let hi = table.partition_point(|e| (e.block, e.instr) <= (block, instr));
    match hi.checked_sub(1).and_then(|i| table.get(i)) {
        Some(e) => (e.line, e.column),
        None    => (0, 0),
    }
}

