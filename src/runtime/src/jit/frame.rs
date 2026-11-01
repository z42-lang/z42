/// JIT frame and compiled-entry types.
///
/// `JitFrame` is the runtime stack frame passed (as a raw pointer) to every
/// JIT-compiled function, next to the running thread's `VmContext`.
/// `FnEntry` is one compiled function as the shared JIT runtime
/// (`super::runtime::JitRuntime`) hands it out.

use crate::metadata::{Function, Value};
use crate::vm_context::VmContext;

// ── JitFrame ─────────────────────────────────────────────────────────────────

/// Runtime stack frame for a JIT-compiled function.
/// Pure register machine — all variables use integer register IDs, no named slots.
///
/// `#[repr(C)]` because compiled code reads and writes two fields at fixed
/// offsets: the prologue loads `regs_ptr` ([`JIT_FRAME_REGS_PTR_OFFSET`], pinned
/// to 0) and `Ret %r` stores into `ret` / `has_ret` ([`JIT_FRAME_RET_OFFSET`],
/// [`JIT_FRAME_HAS_RET_OFFSET`]). Neither goes through a helper call.
#[repr(C)]
pub struct JitFrame {
    /// `regs.as_mut_ptr()`, set by every constructor once `regs` is sized. The
    /// register file is never resized while the frame exists (sized to the
    /// function's `max_reg + 1` up front; JIT code and helpers only index it —
    /// and the one cross-frame resize, `interp::frame::store_thru_ref`, can't
    /// reach a JIT frame: `LoadLocalAddr` is JIT-unsupported, and a slot index
    /// is always `<= max_reg` anyway), so the buffer never moves and this stays
    /// valid until [`JitFrame::recycle`] hands `regs` back to the pool. Moving the
    /// `JitFrame` itself doesn't move the heap buffer.
    pub regs_ptr: *mut Value,
    /// Return value, written by compiled code at `Ret %r` (a 16-byte copy of the
    /// register slot — `Value` is `Copy`, no refcount / barrier). Meaningful only
    /// when `has_ret != 0`. Not a GC root: nothing reaches a safepoint between
    /// the store and `enter_compiled` taking it out.
    pub ret: Value,
    /// `1` once `Ret %r` stored `ret`; stays `0` for a void `Ret` — the
    /// `Returned(None)` vs `Returned(Some(Null))` distinction.
    pub has_ret: u8,
    /// Register file indexed by SSA register number.
    pub regs: Vec<Value>,
    /// This activation's id for arena slots it allocates (struct values),
    /// `0` until taken: `struct_ops::frame_id_of` takes one from
    /// `VmContext::next_frame_id()` on first use, and an OSR hand-off inherits
    /// the interp frame's (taken or `0`). The id keys the arena staleness guard
    /// exactly like an interp frame's; the arena's LIFO truncation is driven by
    /// the `VmFrame` bases (push/pop_frame), not by the id.
    pub frame_id: u32,
}

impl JitFrame {
    /// Allocate a new frame with `max_reg + 1` register slots from `vm`'s register
    /// pool (all `Null`). The first `args.len()` registers are initialised with
    /// the call arguments.
    pub fn new(vm: &VmContext, max_reg: usize, args: &[Value]) -> Self {
        let size = max_reg + 1;
        let mut regs = vm.reg_pool.take(size);
        for (i, v) in args.iter().enumerate() {
            if i < size {
                regs[i] = v.clone();
            }
        }
        Self::with_regs(regs)
    }

    /// add-osr-loop-tiering: build a JitFrame from an interpreter frame's live
    /// register file at an OSR hand-off. Interp and JIT share the exact same
    /// register model (`Vec<Value>` indexed by IR reg number), so this is a straight
    /// copy of the live state — the native OSR entry resumes the hot loop reading
    /// these values back through `regs_base`. Sized to the JIT function's
    /// `max_reg + 1`; interp slots beyond that are dropped (same function → same
    /// max_reg), missing ones stay `Null`.
    pub fn from_interp_regs(vm: &VmContext, interp_regs: &[Value], max_reg: usize) -> Self {
        let size = max_reg + 1;
        let mut regs = vm.reg_pool.take(size);
        for (i, v) in interp_regs.iter().enumerate() {
            if i < size { regs[i] = v.clone(); }
        }
        Self::with_regs(regs)
    }

    /// Allocate a frame and fill its first registers directly from the caller's
    /// register file, indexed by `arg_indices`. Avoids the intermediate
    /// `Vec<Value>` collect + the resulting double-clone that `new(_, &args)`
    /// incurs on the hot `jit_call` path (perf: per-call malloc/free + one of
    /// two arg clones eliminated; reg Vec still pooled). Each argument is cloned
    /// exactly once (caller reg → callee reg).
    pub fn new_args_from(
        vm: &VmContext, max_reg: usize, caller_regs: &[Value], arg_indices: &[u32],
    ) -> Self {
        let size = max_reg + 1;
        let mut regs = vm.reg_pool.take(size);
        for (i, &r) in arg_indices.iter().enumerate() {
            if i < size {
                regs[i] = caller_regs[r as usize].clone();
            }
        }
        Self::with_regs(regs)
    }

    /// Like `new_args_from`, but for a method call: register 0 is the receiver
    /// (`this`), and registers `1..` are the positional args read from the
    /// caller's register file via `arg_indices`. Avoids the
    /// `vec![obj]` + `append(extra_args)` two-Vec dance on the hot `jit_vcall`
    /// path. The receiver is moved in (already cloned by the caller).
    pub fn new_method_args_from(
        vm: &VmContext, max_reg: usize, receiver: Value, caller_regs: &[Value], arg_indices: &[u32],
    ) -> Self {
        let size = max_reg + 1;
        let mut regs = vm.reg_pool.take(size);
        if size > 0 { regs[0] = receiver; }
        for (i, &r) in arg_indices.iter().enumerate() {
            let slot = i + 1;
            if slot < size {
                regs[slot] = caller_regs[r as usize].clone();
            }
        }
        Self::with_regs(regs)
    }

    /// Wrap an already-sized, already-filled register file. `regs` must not be
    /// resized afterwards (see [`JitFrame::regs_ptr`]).
    #[inline]
    fn with_regs(mut regs: Vec<Value>) -> Self {
        JitFrame {
            regs_ptr: regs.as_mut_ptr(), ret: Value::Null, has_ret: 0,
            regs, frame_id: 0,
        }
    }

    /// The value `Ret %r` stored, or `None` after a void `Ret`.
    #[inline]
    pub fn take_ret(&self) -> Option<Value> {
        if self.has_ret != 0 { Some(self.ret) } else { None }
    }

    /// Hand the register file back to `vm`'s pool once the frame is popped.
    pub fn recycle(self, vm: &VmContext) {
        vm.reg_pool.give(self.regs);
    }
}

/// Byte offset of [`JitFrame::regs_ptr`] (loaded by every compiled prologue).
pub const JIT_FRAME_REGS_PTR_OFFSET: usize = std::mem::offset_of!(JitFrame, regs_ptr);
/// Byte offset of [`JitFrame::ret`] (stored by compiled `Ret %r`).
pub const JIT_FRAME_RET_OFFSET: usize = std::mem::offset_of!(JitFrame, ret);
/// Byte offset of [`JitFrame::has_ret`] (set to 1 by compiled `Ret %r`).
pub const JIT_FRAME_HAS_RET_OFFSET: usize = std::mem::offset_of!(JitFrame, has_ret);
const _: () = assert!(JIT_FRAME_REGS_PTR_OFFSET == 0, "regs_ptr must be JitFrame's first field");
const _: () = assert!(std::mem::size_of::<Value>() == 16 && JIT_FRAME_RET_OFFSET % 8 == 0,
    "Ret copies a Value as two aligned 8-byte words");

// ── FnEntry ──────────────────────────────────────────────────────────────────

/// A compiled native function entry inside the JIT module.
///
/// `Clone` (not `Copy`) because of the shared `owner_init` cell.
#[derive(Clone)]
pub struct FnEntry {
    /// Pointer to the native machine code of the function.
    pub ptr:     *const u8,
    /// Size of the register file needed by this function (`max_reg`).
    pub max_reg: usize,
    /// The function this code was compiled from — the `VmFrame` the call
    /// pushes points at it, stack traces derive name / file / line from it, and
    /// `jit_call`'s barriers read its name. Valid for the whole run: an entry
    /// function lives in the module, a lazily loaded one is kept alive by its
    /// `FuncTable` slot. Null for [`FnEntry::rejected`].
    pub func:    *const Function,
    /// The function's `owner_init` cell (shared, see `Function::owner_init`),
    /// so `jit_call`'s cctor barrier needs no name-based owner lookup.
    pub owner_init: crate::metadata::ir::OwnerInitCell,
    /// `Some(fold token)` when the function is a pure `[Native]` extern wrapper
    /// (`interp::forward::forward_of`); the call helpers then skip the activation and
    /// dispatch the builtin directly (see [`super::forward`]).
    pub forward: Option<u32>,
    /// Call-barrier settle cell (`call.rs::call_compiled`): the barrier epoch at
    /// which this entry's module-init + owner-cctor barrier was last proven it
    /// will keep passing (`cctor.rs::callee_barrier_settled`). Steady state the
    /// gate is one acquire + one relaxed load, same shape as the static-field
    /// gate (`statics.rs`). `0` = not settled. Shared by clones of this entry.
    pub init_ready: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

// Raw pointers — the code is owned by the VM's `JitRuntime` (which holds this
// entry), `func` by the module / `FuncTable`; both outlive every reader.
unsafe impl Send for FnEntry {}
unsafe impl Sync for FnEntry {}

impl FnEntry {
    /// Negative-cache marker of the OSR entry cache: a null-`ptr` entry meaning
    /// "no OSR variant (untranslatable or compile failed) — keep interpreting".
    /// (Per-function slots keep their verdict in `JitSlot`'s state instead.)
    pub fn rejected() -> Self {
        FnEntry {
            ptr: std::ptr::null(), max_reg: 0, func: std::ptr::null(),
            owner_init: Default::default(),
            forward: None,
            init_ready: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
        }
    }
    /// Settle-gate probe: does the recorded epoch still match? (See `init_ready`.)
    #[inline]
    pub fn init_ready_eq(&self, epoch: u32) -> bool {
        self.init_ready.load(std::sync::atomic::Ordering::Acquire) == epoch
    }
    /// Record that the call barrier passed and keeps passing as of `epoch`.
    #[inline]
    pub fn mark_init_ready(&self, epoch: u32) {
        self.init_ready.store(epoch, std::sync::atomic::Ordering::Release);
    }
    #[inline]
    pub fn is_rejected(&self) -> bool { self.ptr.is_null() }
}
