//! Shared activation records; raw function/register pointers live until the paired pop.
use std::cell::Cell;
use crate::object::Value;
use crate::vm::runtime::Function;

/// One unified per-frame entry — single source of truth for
/// (a) GC root scanning, (b) stack-trace formatting, and (c) interp
/// `RefKind::Stack` cross-frame deref.
///
/// unify-frame-chain replaces three previously-parallel
/// vectors (`exec_stack`, `env_arena_stack`, `call_stack`) with a single
/// `Vec<VmFrame>`. Push and pop happen in lockstep — no caller can
/// "forget half" and leak a partial frame.
///
/// The frame is kept thin (40 B, no refcounted fields) because one is pushed
/// per call: it records only *which* function runs and *where* it is (`pc`).
/// The display name, file and line/column are derived from `func` when a
/// stack trace is built ([`VmFrame::snapshot`]), never on the call path.
///
/// # Safety / lifetime
///
/// `func` / `regs` are raw pointers. `regs` points into a `JitFrame` or
/// interp `Frame` on the Rust call stack; `func`
/// points at the executing `Function`, which the caller (interp) or the
/// JIT runtime (merged module or a lazily-loaded function kept
/// alive by its lazy slot) holds for at least the activation. All are valid
/// for the duration of the corresponding `exec_function` / native call —
/// RAII (`FrameGuard` for interp, `jit::runtime::invoke::enter_compiled` for JIT)
/// guarantees the pop runs before the owning frame's stack slot goes away.
///
/// `pc` is mutable via [`Cell`] so callers can stamp the current call-site
/// position just before invoking a callee, without re-borrowing the
/// surrounding `Vec<VmFrame>`.
#[derive(Debug)]
pub struct VmFrame {
    /// The executing function — source of the frame's name, file and line table.
    pub func:      *const Function,
    /// Pointer to the frame's register file. The Vec content is the
    /// canonical place where this frame's z42 values live.
    pub regs:      *const Vec<Value>,
    /// add-offline-symbolication: packed code offset of the frame's current
    /// site, `Function::linear_offset(block, instr)` = `block << 16 | instr`.
    /// Stamped by the Call / VCall / CallIndirect sites and the throw path
    /// (both backends). [`PC_UNSET`] = never stamped. Line/column are resolved
    /// from it at snapshot time; a stripped frame (no line info) prints it as
    /// `+0x<offset>` — the offline-resolvable key for `z42d symbolicate`.
    pub pc:        Cell<u32>,
    /// add-escape-analysis-stack-alloc: the per-context stack-arena lengths when
    /// this frame was pushed. `pop_frame` truncates the arena back to these,
    /// bulk-freeing this frame's stack-allocated objects/arrays (LIFO). Stamped
    /// by `push_frame` (not the `new()` call sites) → all frame kinds get it for
    /// free; JIT frames never stack-allocate so their truncate is a no-op.
    /// Arena slot indices are `u32` (`stack_alloc_obj` etc.), so a length fits.
    pub stack_obj_base: u32,
    pub stack_arr_base: u32,
    /// add-struct-value-semantics: value-struct byte-arena length when this frame
    /// was pushed; `pop_frame` truncates back to it (LIFO-frees this frame's blobs).
    /// Stamped by `push_frame`.
    pub struct_base: u32,
    /// make-value-copy: transient-arena length when this frame was pushed; `pop_frame`
    /// truncates back to it (LIFO-frees this frame's Ref/PinnedView/StructRefHeap
    /// payloads). Stamped by `push_frame`.
    pub transient_base: u32,
}

/// [`VmFrame::pc`] value of a frame that has not stamped a site yet.
pub const PC_UNSET: u32 = u32::MAX;

// SAFETY (add-multithreading-foundation Phase 3):
// `VmFrame` holds raw pointers (`func` / `regs`) valid for the
// frame's lifetime, which is enclosed by `FrameGuard` RAII / `enter_compiled`.
// The GC scanner is the only cross-thread reader (mark phase invoked from a
// possible GC worker thread); it only ever reads these pointers while the
// owning thread is paused at a safepoint. The `Cell<u32>` `pc` is covered by
// the same single-writer invariant.
unsafe impl Send for VmFrame {}
unsafe impl Sync for VmFrame {}

impl VmFrame {
    pub fn new(func: *const Function, regs: *const Vec<Value>) -> Self {
        Self {
            func, regs,
            pc: Cell::new(PC_UNSET),
            // Arena bases are overwritten by push_frame.
            stack_obj_base: 0, stack_arr_base: 0,
            struct_base: 0, transient_base: 0,
        }
    }

    /// The executing function.
    #[inline]
    pub fn func(&self) -> &Function {
        // SAFETY: see the type-level note — `func` outlives the frame.
        unsafe { &*self.func }
    }

    /// Source `(line, column)` of the frame's current site, `(0, 0)` when no
    /// site was stamped or the function carries no line info. No allocation
    /// (the crash-signal handler calls it).
    pub fn line_col(&self) -> (u32, u32) {
        let pc = self.pc.get();
        if pc == PC_UNSET { return (0, 0); }
        crate::metadata::debug::resolve_line(self.func().line_table(), pc >> 16, pc & 0xffff)
    }


}

