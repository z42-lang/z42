//! Views of the active frame chain for roots, traces, profiling and crash reports.
//! Callers retain the owner-thread or parked-owner read scope of FrameStack.
use crate::object::Value;
use crate::vm::exec::call::frames::VmFrame;

/// Frozen view of a [`VmFrame`] suitable for formatting / passing across
/// borrow scopes.
#[derive(Debug, Clone)]
pub struct FrameSnapshot {
    pub func_name: std::sync::Arc<str>,
    pub file:      std::sync::Arc<str>,
    pub line:      u32,
    pub column:    u32,
    /// add-offline-symbolication: linearized code offset (`u32::MAX` = unset).
    pub offset:    u32,
}


impl VmFrame {
    /// Snapshot used at throw time — name / file / line / column computed from
    /// `func` + `pc` here. Strips the raw pointers — snapshots are not
    /// GC-root-scanner targets.
    pub fn snapshot(&self) -> FrameSnapshot {
        let (func_name, file) = self.func().frame_name_file();
        let (line, column) = self.line_col();
        FrameSnapshot { func_name, file, line, column, offset: self.pc.get() }
    }
}

/// Format a captured trace as a multi-line string.
///
/// Frames are presented in caller-to-throw order — the throwing function
/// is **last**, matching .NET / Java convention (most-recent at the bottom).
/// Each line: `  at <func> (<file>:<line>[:<col>])`.
///
/// `column` (zbc 1.1+) appears only when > 0; legacy frames without column
/// gracefully degrade to `(file:line)`. When both file and column are
/// missing the trailing `(...)` is omitted entirely.
pub fn format_stack_trace(frames: &[FrameSnapshot]) -> String {
    let mut out = String::new();
    for f in frames.iter().rev() {
        out.push_str("  at ");
        out.push_str(&f.func_name);
        if !f.file.is_empty() {
            out.push_str(" (");
            out.push_str(&f.file);
            if f.line > 0 {
                out.push(':');
                out.push_str(&f.line.to_string());
                if f.column > 0 {
                    out.push(':');
                    out.push_str(&f.column.to_string());
                }
            }
            out.push(')');
        } else if f.line > 0 {
            out.push_str(" (line ");
            out.push_str(&f.line.to_string());
            if f.column > 0 {
                out.push_str(", col ");
                out.push_str(&f.column.to_string());
            }
            out.push(')');
        } else if f.offset != u32::MAX {
            // add-offline-symbolication: no line info (release-stripped, no
            // adjacent .zsym merged) — emit the linearized code offset as an
            // offline-resolvable key. `z42d symbolicate <trace> --syms <.zsym>`
            // maps `+0x<offset>` back to file:line:col via the archived sidecar.
            out.push_str(" +0x");
            out.push_str(&format!("{:x}", f.offset));
        }
        out.push('\n');
    }
    out.trim_end().to_string()
}

/// Freeze the live chain while its owner read scope is held.
pub fn snapshots(frames: &[VmFrame]) -> Vec<FrameSnapshot> {
    frames.iter().map(VmFrame::snapshot).collect()
}

/// Visit register roots without taking a lock or copying their storage.
///
/// # Safety
/// Every record and its register file must remain alive and unchanged for the
/// call. The caller is their owner, or holds the GC pause while owners are parked.
#[inline]
pub(crate) unsafe fn visit_roots(frames: &[VmFrame], mut visit: impl FnMut(&Value)) {
    for frame in frames {
        // SAFETY: the caller keeps the owning activation and read scope alive.
        for value in unsafe { &*frame.regs }.iter() {
            visit(value);
        }
    }
}

#[cfg(all(test, z42_test_facade))]
#[path = "stack_tests.rs"]
pub mod stack_tests;
