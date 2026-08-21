/// Interpreter backend — tree-walking bytecode execution.
///
/// Implementation is split across submodules:
/// • mod.rs        — public API, Frame, core execution loop
/// • exec_instr.rs — thin per-Instruction dispatcher (exhaustive match → helpers)
/// • exec_value.rs — constants / copy / arith / cmp / logical / unary / bitwise / string
/// • exec_address.rs — LoadLocalAddr / LoadElemAddr / LoadFieldAddr / DefaultOf
/// • exec_call.rs    — Call / Builtin / LoadFn / LoadFnCached / CallIndirect / MkClos
/// • exec_array.rs   — ArrayNew / ArrayNewLit / ArrayGet / ArraySet / ArrayLen
/// • exec_object.rs  — ObjNew / FieldGet / FieldSet / IsInstance / AsCast / Static*
/// • exec_vcall.rs   — VCall + primitive_class_name + is_array_isa (single-op file)
/// • exec_native.rs  — CallNative / CallNativeVtable / PinPtr / UnpinPtr
/// • dispatch.rs   — object dispatch helpers (vtable, ToString, static fields)
/// • ops.rs        — register-level helpers (int_binop, numeric_lt, collect_args, …)

pub(crate) mod dispatch;
pub(crate) mod exec_instr;
mod exec_address;
pub(crate) mod exec_array;                 // add-struct-jit-value-path: JIT reuses try_struct_backed/pack_struct_elem
mod exec_call;
#[cfg(feature = "native-interop")]
mod exec_native;
mod exec_object;
pub(crate) mod exec_struct;   // add-struct-value-semantics: blob value-type instruction exec (JIT helpers reuse the *_val cores)
pub(crate) mod exec_value;
mod exec_vcall;
mod ops;
pub(crate) mod stack_alloc;   // add-escape-analysis-stack-alloc: per-context stack arena
pub(crate) mod struct_arena;  // add-struct-value-semantics: per-context byte arena for value structs
pub(crate) mod transient_arena; // make-value-copy: per-context arena for Ref/PinnedView/StackClosure/StructRefHeap

// Re-export for cross-module callers (notably jit/helpers_object.rs).
pub(crate) use exec_vcall::primitive_class_name;
// JIT primitive-receiver VCall IC (add-jit-primitive-vcall-ic): jit_vcall keys its
// inline cache on primitives via the same synthetic PRIM_TYPE_* ids interp uses.
pub(crate) use exec_vcall::value_synthetic_type_id;
pub(crate) use exec_object::prim_isa;   // fix-boxed-primitive-is-as: JIT is/as 复用

pub use crate::corelib::convert::value_to_str;
use crate::metadata::{BranchTargets, Function, Module, Terminator, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Context, Result};
use std::collections::HashMap;

// ── Execution outcome ────────────────────────────────────────────────────────

/// Outcome of executing a function.
/// User exceptions are value-based (no heap allocation), not anyhow errors.
///
/// Public so embedders (test-runner, REPL) can introspect thrown exception
/// values — necessary for [ShouldThrow<E>] type matching and TestFailure /
/// SkipSignal classification (rewrite-z42-test-runner-compile-time S3,
/// 2026-05-10).
#[derive(Debug)]
pub enum ExecOutcome {
    /// Normal return (with optional return value).
    Returned(Option<Value>),
    /// User exception thrown and not caught within this function.
    Thrown(Value),
}

// ── Public entry points ──────────────────────────────────────────────────────

/// Entry point: run a function with the given arguments.
pub fn run(ctx: &VmContext, module: &Module, func: &Function, args: &[Value]) -> Result<()> {
    match exec_function(ctx, module, func, args)? {
        ExecOutcome::Returned(_) => Ok(()),
        ExecOutcome::Thrown(val) => bail!("{}", crate::exception::format_uncaught(&val, module)),
    }
}

/// Variant of [`run`] that returns the function's return value (if any)
/// instead of discarding it. Used by integration tests and by embedders
/// that need the result of a script entry point. Mirrors `run` in every
/// other respect (errors, exception conversion).
pub fn run_returning(
    ctx: &VmContext,
    module: &Module,
    func: &Function,
    args: &[Value],
) -> Result<Option<Value>> {
    match exec_function(ctx, module, func, args)? {
        ExecOutcome::Returned(v) => Ok(v),
        ExecOutcome::Thrown(val) => bail!("{}", crate::exception::format_uncaught(&val, module)),
    }
}

/// Public-API variant of [`run`] that surfaces both the typed thrown
/// exception value (for type introspection / [ShouldThrow<E>] matching)
/// and the optional return value, instead of collapsing thrown into an
/// anyhow string. For embedders that need exception-aware control flow
/// (rewrite-z42-test-runner-compile-time S3, 2026-05-10).
pub fn run_outcome(
    ctx: &VmContext,
    module: &Module,
    func: &Function,
    args: &[Value],
) -> Result<ExecOutcome> {
    exec_function(ctx, module, func, args)
}

/// Initialise static state: clears static fields then runs ALL
/// `*.__static_init__` functions (both eager-loaded in `module.functions`
/// and lazy-loadable from declared zpkgs).
///
/// Extracted from [`run_with_static_init`] (2026-05-10 R3b) so embedders
/// (test-runner, REPL) can do init once + run multiple functions in
/// sequence (Setup → Test → Teardown) without re-initialising between.
///
/// 2026-04-27 fix-static-field-access: 修前只跑 `{module.name}.__static_init__`
/// (主模块)，导入的 zpkg（如 z42.math 的 `Std.Math.__static_init__`）虽然 link 进
/// merged module 但永不被调用 → `Math.PI` 等常量永远 `null`。
///
/// interp 模式下 stdlib 是 lazy-loaded，启动时除 z42.core 外都不在
/// `module.functions`。所以同时需要：
///   1. 扫主模块 functions（拿到 eagerly-loaded 的 init，含 main 自己 + z42.core）
///   2. 通过 `lazy_loader::declared_namespaces()` 拿到所有声明但未加载的命名空间，
///      调用 `try_lookup_function("<ns>.__static_init__")` 触发 lazy load
///   3. 合并 + 按 FQN 字母序去重 + 逐一调用
///
/// 副作用：所有声明的 stdlib zpkg 都会被 eagerly 加载（不再纯 lazy）。
pub fn init_static_fields(ctx: &VmContext, module: &Module) -> Result<()> {
    ctx.static_fields_clear();

    // 1. Eager-loaded init functions (in main + z42.core).
    let mut eager_inits: Vec<&Function> = module.functions.iter()
        .filter(|f| f.name.ends_with(".__static_init__"))
        .collect();
    eager_inits.sort_by(|a, b| a.name.cmp(&b.name));
    for init_fn in &eager_inits {
        match exec_function(ctx, module, init_fn, &[])? {
            ExecOutcome::Returned(_) => {}
            ExecOutcome::Thrown(val) =>
                bail!("uncaught exception in static init `{}`: {}", init_fn.name, value_to_str(&val)),
        }
    }

    // 2. Lazy-loadable init functions (from declared but not-yet-loaded zpkgs).
    //
    // fix-multi-file-static-init (2026-05-15): the compiler now emits
    // `<ns>.<source-stem>.__static_init__` (one per CU), so a single
    // `try_lookup_function("<ns>.__static_init__")` would never resolve. We
    // force-load every declared zpkg, then enumerate ALL `*.__static_init__`
    // functions via the loader and run each.
    let lazy_init_names = ctx.collect_lazy_static_init_names();
    for init_name in lazy_init_names {
        let Some(init_fn) = ctx.try_lookup_function(&init_name) else { continue };
        match exec_function(ctx, module, init_fn.as_ref(), &[])? {
            ExecOutcome::Returned(_) => {}
            ExecOutcome::Thrown(val) =>
                bail!("uncaught exception in static init `{}`: {}", init_name, value_to_str(&val)),
        }
    }
    Ok(())
}

/// Run with static init: convenience wrapper — calls
/// [`init_static_fields`] then runs `func`. Used by `Vm::run`.
pub fn run_with_static_init(ctx: &VmContext, module: &Module, func: &Function) -> Result<()> {
    init_static_fields(ctx, module)?;
    match exec_function(ctx, module, func, &[])? {
        ExecOutcome::Returned(_) => Ok(()),
        ExecOutcome::Thrown(val) => bail!("{}", crate::exception::format_uncaught(&val, module)),
    }
}

// ── Frame ────────────────────────────────────────────────────────────────────

pub(crate) struct Frame {
    pub regs: Vec<Value>,
    /// 2026-05-02 impl-closure-l3-escape-stack: frame-local arena 持有不逃逸
    /// closure 的 env。`Value::StackClosure { env_idx }` 索引这里。frame drop
    /// 时整个 arena 一并释放（内嵌的 Value 走 normal Drop / GcRef 减引用计数）。
    pub env_arena: Vec<Vec<Value>>,
    /// Spec impl-ref-out-in-runtime (Decision R2 architecture E):
    /// `(param_reg, original_ref_kind)` pairs. When the function was called
    /// with a `ref`/`out`/`in` argument, the entry path deref'd the Ref
    /// (storing the underlying value into `regs[param_reg]` so all
    /// instruction handlers see a normal value) and stashed the original
    /// `RefKind` here. At function exit, every entry's final `regs[param_reg]`
    /// value is stored back through the corresponding Ref to the caller's
    /// lvalue. Net semantic: caller sees the post-call value of its
    /// `ref`/`out` lvalue, identical to true cross-frame refs but without
    /// requiring 80+ instruction handlers to be deref-aware.
    pub ref_writebacks: Vec<(u32, crate::metadata::types::RefKind)>,
    /// add-osr-loop-tiering: loop back-edges taken in THIS activation. When it
    /// reaches `JitModuleCtx.osr_threshold` (jit mode only), the running loop is
    /// hot enough to hand off to native code (OSR). Per-activation (reset each
    /// `Frame::new*`) so "called once, loops a lot" upgrades while "called a lot,
    /// loops a little" does not (that's the call-count path's job).
    pub back_edge_count: u32,
    /// add-escape-analysis-stack-alloc: this frame's monotonic id, stamped at
    /// entry (`exec_function_body`) from `ctx.next_frame_id()`. `ObjNew`/`ArrayNew`
    /// with `stack_alloc` tag their arena slots with it; a `Value::StackObject`/
    /// `StackArray` handle carries it so a stale access (after this frame
    /// truncated the arena) is caught by the frame_id mismatch. 0 = unstamped
    /// (frames that never stack-allocate; the arena is never keyed on 0).
    pub frame_id: u32,
    /// add-generic-methods: resolved concrete FQ type-argument names for the
    /// generic method call that created this frame (from `CallInsn::method_type_args`).
    /// Empty for non-generic calls. Read by `MethodTypeArg` / `MethodDefault` in the
    /// body to materialize `typeof(T)` / `new T()` / `default(T)`. Mirrors the
    /// class-level `Object.type_args` carrier, but on the frame (static methods have
    /// no `this`). Set after construction in `exec_call` from the call instruction.
    pub method_type_args: Box<[String]>,
}

thread_local! {
    /// Per-thread free-list of register-file Vecs (perf-vm-iteration Phase 1).
    /// LIFO reuse across `Frame::new` / `Drop for Frame`. Bounded so deep-then-
    /// shallow recursion doesn't pin memory forever. Thread-local ⇒ no lock, no
    /// GC-root visibility (returned Vecs are cleared before parking, and are
    /// never registered as roots — only a *live* frame's regs are scanned).
    static REGS_POOL: std::cell::RefCell<Vec<Vec<Value>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Max Vecs retained in the per-thread pool. Caps idle memory; excess frees
/// normally.
const REGS_POOL_CAP: usize = 512;

impl Drop for Frame {
    fn drop(&mut self) {
        // Return the register-file allocation to the per-thread pool for reuse.
        // `clear()` drops every held `Value` (Arc<str> refcount dec, GcRef drop
        // is a no-op) BEFORE parking the Vec, so no stale heap ref lingers in an
        // unscanned location. Runs only after `FrameGuard` popped the VmFrame
        // (drop order), so the regs pointer is no longer a GC root here.
        let mut regs = std::mem::take(&mut self.regs);
        regs.clear();
        if regs.capacity() > 0 {
            REGS_POOL.with(|p| {
                let mut pool = p.borrow_mut();
                if pool.len() < REGS_POOL_CAP {
                    pool.push(regs);
                }
            });
        }
    }
}

impl Frame {
    pub fn new(args: &[Value], max_reg: u32) -> Self {
        let size = if max_reg > 0 { max_reg as usize } else { args.len() };
        let need = size.max(args.len());
        // perf-vm-iteration Phase 1 (Decision 3): reuse a register-file Vec from
        // a per-thread free-list instead of `vec![Null; need]` every call. The
        // pool is thread-local (one mutator thread per VmContext today), so it
        // has no GC / cross-thread coupling — unlike the call_stack Mutex. The
        // matching `Drop for Frame` returns the (cleared) Vec to the pool AFTER
        // `FrameGuard` has already popped this frame's VmFrame root (drop order:
        // `_frame_guard`/`_vm_guard` are declared after `frame` in exec_function,
        // so they drop first). Saves one malloc+free per call on call-heavy code.
        let mut regs = REGS_POOL.with(|p| p.borrow_mut().pop()).unwrap_or_default();
        regs.clear();
        regs.resize(need, Value::Null);
        for (i, v) in args.iter().enumerate() {
            regs[i] = v.clone();
        }
        Frame {
            regs,
            env_arena: Vec::new(),
            ref_writebacks: Vec::new(),
            back_edge_count: 0,
            frame_id: 0,
            method_type_args: Box::default(),
        }
    }

    /// perf-vm-iteration Phase 1 (Decision 3): build a frame filling the
    /// register file directly from `caller_regs[arg_indices[i]]` — one clone per
    /// arg, no intermediate args `Vec`. Same pooling as `new`. Returns an error
    /// (not a panic) on an out-of-range register index, matching `collect_args`.
    pub fn new_from_regs(caller_regs: &[Value], arg_indices: &[u32], max_reg: u32) -> Result<Self> {
        let argc = arg_indices.len();
        let size = if max_reg > 0 { max_reg as usize } else { argc };
        let need = size.max(argc);
        let mut regs = REGS_POOL.with(|p| p.borrow_mut().pop()).unwrap_or_default();
        regs.clear();
        regs.resize(need, Value::Null);
        for (i, &r) in arg_indices.iter().enumerate() {
            let v = caller_regs.get(r as usize)
                .ok_or_else(|| anyhow::anyhow!("undefined register %{r}"))?;
            regs[i] = v.clone();
        }
        Ok(Frame {
            regs,
            env_arena: Vec::new(),
            ref_writebacks: Vec::new(),
            back_edge_count: 0,
            frame_id: 0,
            method_type_args: Box::default(),
        })
    }

    /// perf-vm-iteration Phase 1 (Decision 3): like `new_from_regs` but with a
    /// prepended receiver (`this`) in slot 0 — for the virtual-call hot path
    /// (`exec_vcall`), which passes `regs[0] = receiver`, `regs[1+i] = args[i]`.
    /// Eliminates the vcall path's `vec![receiver]` + `collect_args` Vecs and the
    /// arg double-clone; receiver + each arg cloned exactly once.
    pub fn new_from_receiver_regs(
        receiver: &Value, caller_regs: &[Value], arg_indices: &[u32], max_reg: u32,
    ) -> Result<Self> {
        let argc = arg_indices.len();
        let total = argc + 1; // + receiver in slot 0
        let size = if max_reg > 0 { max_reg as usize } else { total };
        let need = size.max(total);
        let mut regs = REGS_POOL.with(|p| p.borrow_mut().pop()).unwrap_or_default();
        regs.clear();
        regs.resize(need, Value::Null);
        regs[0] = receiver.clone();
        for (i, &r) in arg_indices.iter().enumerate() {
            let v = caller_regs.get(r as usize)
                .ok_or_else(|| anyhow::anyhow!("undefined register %{r}"))?;
            regs[i + 1] = v.clone();
        }
        Ok(Frame {
            regs,
            env_arena: Vec::new(),
            ref_writebacks: Vec::new(),
            back_edge_count: 0,
            frame_id: 0,
            method_type_args: Box::default(),
        })
    }

    /// Set a register's raw value (no deref). For ref-aware store-through
    /// (transparently writing through `Value::Ref` to the underlying
    /// caller slot / array elem / object field), use `set_thru_ref`
    /// (spec impl-ref-out-in-runtime).
    /// Write a register. The frame is pre-sized to `max_reg` at construction, so
    /// the in-bounds path is overwhelmingly hot — keep it inlinable into the exec
    /// loop (this is one of the two hottest interp functions, per profiling). The
    /// grow path (a reg beyond the pre-sized file, e.g. hand-built test functions)
    /// is `#[cold]` + out-of-line so it doesn't bloat the hot path.
    #[inline]
    pub fn set(&mut self, reg: u32, val: Value) {
        let idx = reg as usize;
        if idx < self.regs.len() {
            self.regs[idx] = val;
        } else {
            self.set_grow(idx, val);
        }
    }

    #[cold]
    #[inline(never)]
    fn set_grow(&mut self, idx: usize, val: Value) {
        self.regs.resize(idx + 1, Value::Null);
        self.regs[idx] = val;
    }

    /// Get a register's raw value (no deref). For ref-aware read-through
    /// (transparently dereferencing `Value::Ref`), use `get_deref`
    /// (spec impl-ref-out-in-runtime).
    #[inline(always)]
    pub fn get(&self, reg: u32) -> Result<&Value> {
        let idx = reg as usize;
        if idx < self.regs.len() {
            Ok(&self.regs[idx])
        } else {
            anyhow::bail!("undefined register %{reg}")
        }
    }

    /// Spec impl-ref-out-in-runtime (Decision R2): read a register's value
    /// with transparent deref. If the register holds a `Value::Ref`, the
    /// underlying value (from caller frame / array elem / object field) is
    /// returned. Otherwise behaves like `get` but returns owned `Value`.
    /// Use this in instruction handlers that read user-visible values.
    #[allow(dead_code)]
    pub fn get_deref(&self, reg: u32, ctx: &VmContext) -> Result<Value> {
        match self.get(reg)? {
            // make-value-copy: resolve the Ref handle → RefKind via the transient arena,
            // release the arena lock (clone) before deref touches heap / other locks.
            Value::Ref { idx, frame_id } => {
                let kind = ctx.transient_arena.lock().ref_kind(*idx, *frame_id)?;
                deref_ref(&kind, ctx)
            }
            other => Ok(other.clone()),
        }
    }

    /// Spec impl-ref-out-in-runtime (Decision R2): write a register with
    /// transparent store-through. If the register currently holds a
    /// `Value::Ref`, the write is forwarded to the underlying location and
    /// the Ref itself is preserved so subsequent reads/writes still
    /// indirect. Otherwise the register is overwritten.
    #[allow(dead_code)]
    pub fn set_thru_ref(&mut self, reg: u32, val: Value, ctx: &VmContext) -> Result<()> {
        let idx = reg as usize;
        if idx >= self.regs.len() {
            self.regs.resize(idx + 1, Value::Null);
        }
        let kind_to_store = match &self.regs[idx] {
            // make-value-copy: resolve the Ref handle → RefKind via the transient arena.
            Value::Ref { idx: ai, frame_id } => Some(ctx.transient_arena.lock().ref_kind(*ai, *frame_id)?),
            _ => None,
        };
        match kind_to_store {
            Some(kind) => store_thru_ref(&kind, val, ctx),
            None => { self.regs[idx] = val; Ok(()) }
        }
    }
}

/// Spec impl-ref-out-in-runtime: dereference a `Value::Ref { kind }` into
/// the underlying value. Stack kind looks up `ctx.frame_state_at(frame_idx)`
/// (raw pointer, safe under design Decision 9: refs don't escape call
/// stack). Array/Field kinds borrow the held GcRef and read.
pub(crate) fn deref_ref(
    kind: &crate::metadata::types::RefKind, ctx: &VmContext,
) -> Result<Value> {
    use crate::metadata::types::RefKind;
    match kind {
        RefKind::Stack { frame_idx, slot } => {
            let regs_ptr = ctx.frame_state_at(*frame_idx as usize)
                .ok_or_else(|| anyhow::anyhow!(
                    "ref points to popped frame (frame_idx={frame_idx})"))?;
            // SAFETY: spec Decision 9 — refs never escape the call stack;
            // when this deref runs, the target frame is still active.
            let regs = unsafe { &*regs_ptr };
            let v = regs.get(*slot as usize)
                .ok_or_else(|| anyhow::anyhow!(
                    "ref target slot %{slot} out of frame range"))?;
            // Sanity guard: ref-to-ref nesting not supported (codegen never
            // produces it; defend in case of malformed bytecode).
            if let Value::Ref { .. } = v {
                anyhow::bail!("ref-to-ref nesting not supported");
            }
            Ok(v.clone())
        }
        RefKind::Array { gc_ref, idx } => {
            let arr = gc_ref.borrow();
            arr.get(*idx)
                .ok_or_else(|| anyhow::anyhow!(
                    "ref array index {idx} out of bounds (len={})", arr.len()))
        }
        RefKind::Field { gc_ref, field_name } => {
            let obj = gc_ref.borrow();
            let slot = *obj.type_desc.field_index.get(field_name)
                .ok_or_else(|| anyhow::anyhow!(
                    "ref field `{field_name}` not found on type `{}`",
                    obj.type_desc.name))?;
            Ok(obj.field_value(slot))
        }
    }
}

/// Spec impl-ref-out-in-runtime: store a value through a `Value::Ref` to
/// the underlying location. Mirror of `deref_ref` for the write path.
pub(crate) fn store_thru_ref(
    kind: &crate::metadata::types::RefKind, val: Value, ctx: &VmContext,
) -> Result<()> {
    use crate::metadata::types::RefKind;
    match kind {
        RefKind::Stack { frame_idx, slot } => {
            let regs_ptr = ctx.frame_state_at(*frame_idx as usize)
                .ok_or_else(|| anyhow::anyhow!(
                    "ref points to popped frame (frame_idx={frame_idx})"))?;
            // SAFETY: same as deref_ref; the frame is still active.
            // We need *mut here — cast from *const Vec<Value>. The frame's
            // regs Vec is borrowed from `Frame` which is `&mut` during exec
            // of its instructions, so the Vec is uniquely owned by that
            // frame. Cross-frame mutation requires us to coerce to mut.
            let regs = unsafe { &mut *(regs_ptr as *mut Vec<Value>) };
            let slot_idx = *slot as usize;
            if slot_idx >= regs.len() {
                regs.resize(slot_idx + 1, Value::Null);
            }
            regs[slot_idx] = val;
            Ok(())
        }
        RefKind::Array { gc_ref, idx } => {
            let mut arr = gc_ref.borrow_mut();
            if *idx >= arr.len() {
                anyhow::bail!(
                    "ref array index {idx} out of bounds (len={})", arr.len());
            }
            arr.set_boxed(*idx, val);
            Ok(())
        }
        RefKind::Field { gc_ref, field_name } => {
            let mut obj = gc_ref.borrow_mut();
            let slot_opt = obj.type_desc.field_index.get(field_name).copied();
            match slot_opt {
                // unify-object-byte-layout (PR-2): encode into bytes / refs. (No GC
                // write barrier here — parity with the pre-PR-2 store-through-ref path.)
                Some(slot) => {
                    obj.set_field_value(slot, &val);
                    Ok(())
                }
                None => anyhow::bail!(
                    "ref field `{field_name}` not found on type `{}`",
                    obj.type_desc.name),
            }
        }
    }
}

// ── Debug: source line resolution ─────────────────────────────────────────────

/// Resolve `(line, column)` covering a given (block, instr) site by walking
/// the function's line table forward to the latest entry that doesn't
/// overshoot. Column is 0 when the source position predates zbc 1.1 or the
/// emitter didn't capture it (gracefully degraded by `format_stack_trace`
/// — `(file:line)` instead of `(file:line:col)`).
pub(crate) fn resolve_line(table: &[crate::metadata::bytecode::LineEntry], block: u32, instr: u32) -> (u32, u32) {
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

// ── Core execution loop ──────────────────────────────────────────────────────

/// RAII guard ensuring push_frame / pop_frame stay strictly paired even
/// across `?` early-return or panic unwind from `exec_function`.
///
/// 2026-05-10 unify-frame-chain collapsed the previous trio of pops
/// (regs / env_arena / call_frame) into a single `pop_frame()` matching
/// the new single-row VmFrame model.
struct FrameGuard<'a> {
    ctx: &'a VmContext,
}
impl Drop for FrameGuard<'_> {
    fn drop(&mut self) {
        self.ctx.pop_frame();
    }
}

pub(crate) fn exec_function(ctx: &VmContext, module: &Module, func: &Function, args: &[Value]) -> Result<ExecOutcome> {
    // add-gc-safepoint (2026-05-20): every newly-entered z42 function
    // immediately respects a pending GC request. A worker thread spawned
    // mid-collect parks here before touching any roots.
    crate::gc::safepoint::check_safepoint(ctx);
    // runtime-jit-tiering Phase 1.5.2 (mixed-mode invariant backstop): if `func`
    // is already JIT-compiled, run its native code instead of interpreting it.
    // `exec_function` is the SINGLE choke point every non-hot-Call/VCall interp
    // path funnels through — constructors, closures, `ToString` dispatch, the
    // non-IC / vtable / base-fallback vcall paths, cross-zpkg static calls, and
    // builtin callbacks. Diverting here guarantees Decision 5's invariant — "a
    // compiled function is never interp-executed" — for ALL of them at once
    // (present and future), which is the hard precondition for Phase 2 IR reclaim.
    // The idx-based per-site hooks (`try_native_static_call` /
    // `try_native_method_call`) remain the hot-path fast lane; the two
    // `exec_function_from_*regs` variants are only reached from those hooked paths
    // (cold callees only), so this name-based backstop completes the coverage.
    if let Some(outcome) = try_native_exec(ctx, func, args) {
        return outcome;
    }
    let frame = Frame::new(args, func.max_reg);
    exec_function_body(ctx, module, func, frame)
}

/// add-reflective-invoke: like [`exec_function`] but threads method-level generic
/// `method_type_args` into the callee frame, so a reflectively-invoked *constructed*
/// generic method (`MethodInfo.MakeGenericMethod(..).Invoke(..)`) materializes
/// `typeof(T)`/`new T()`/`default(T)` via the M1 `frame.method_type_args` slot —
/// identical to a direct `Foo<T>()` call. An empty slice is behaviourally identical
/// to `exec_function` (same JIT backstop + frame), so non-generic reflective invokes
/// keep their exact prior path. Generic methods are never JIT-compiled (M1's
/// `jit_unsupported_reason`), so the `try_native_exec` fast lane is only consulted
/// for the empty (non-generic) case.
pub(crate) fn exec_function_with_type_args(
    ctx: &VmContext,
    module: &Module,
    func: &Function,
    args: &[Value],
    method_type_args: &[String],
) -> Result<ExecOutcome> {
    crate::gc::safepoint::check_safepoint(ctx);
    if method_type_args.is_empty() {
        if let Some(outcome) = try_native_exec(ctx, func, args) {
            return outcome;
        }
    }
    let mut frame = Frame::new(args, func.max_reg);
    if !method_type_args.is_empty() {
        frame.method_type_args = method_type_args.into();
    }
    exec_function_body(ctx, module, func, frame)
}

/// runtime-jit-tiering Phase 1.5.2: name-based mixed-mode divert used as the
/// universal backstop at `exec_function`. Returns `None` (→ interpret) when no JIT
/// ctx is published (interp-only run), when an argument is a `Ref` (a stack address
/// can't cross into native code — see `exec_call::try_native_static_call`; a
/// compiled fn never has ref params anyway, so this is defensive), or when the
/// function is cold / untranslatable (`resolve_fn_by_name_tiered` → None). On a
/// compiled hit it runs the native code and marshals the result into an
/// `ExecOutcome`, mirroring `try_native_static_call`.
#[cfg(feature = "jit")]
fn try_native_exec(ctx: &VmContext, func: &Function, args: &[Value]) -> Option<Result<ExecOutcome>> {
    let p = ctx.jit_ctx_ptr();
    if p == 0 { return None; }
    if args.iter().any(|a| matches!(a, Value::Ref { .. })) { return None; }
    let jit_ctx = p as *const crate::jit::frame::JitModuleCtx;
    // SAFETY: `jit_ctx` is valid for the whole `JitModule::run_fn` (set/cleared in
    // lockstep with `vm_ctx`). Copy the small entry fields out before the native
    // call so no borrow of `*jit_ctx` is held across it.
    // Phase 1.5.2: peek (already-compiled?) — NOT the tiered resolve. The divert
    // only ROUTES already-hot functions to native; tier-up counting belongs to the
    // primary call sites. The tiered resolve here double-counted a cold callee
    // (jit_call's counter, then this fallback's) — halving the effective threshold.
    let (max_reg, ptr, name, file) = {
        let entry = unsafe { (*jit_ctx).resolve_fn_by_name_peek(&func.name) }?;
        (entry.max_reg, entry.ptr, entry.name.clone(), entry.file.clone())
    };
    ctx.counters().jit_native_from_interp.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut callee = crate::jit::frame::JitFrame::new(max_reg, args);
    let jit_fn: crate::jit::helpers::JitFn = unsafe { std::mem::transmute(ptr) };
    ctx.push_frame(crate::exception::VmFrame::new(
        name, file, &callee.regs as *const _, &callee.env_arena as *const _));
    let r = unsafe { jit_fn(&mut callee, jit_ctx) };
    ctx.pop_frame();
    if r != 0 {
        callee.recycle();
        return Some(Ok(ExecOutcome::Thrown(ctx.take_exception().unwrap_or(Value::Null))));
    }
    let ret = callee.ret.take();
    callee.recycle();
    Some(Ok(ExecOutcome::Returned(ret)))
}

#[cfg(not(feature = "jit"))]
#[inline]
fn try_native_exec(_ctx: &VmContext, _func: &Function, _args: &[Value]) -> Option<Result<ExecOutcome>> {
    None
}

/// add-osr-loop-tiering: on a hot loop back-edge, hand the running interp
/// activation over to native code (On-Stack Replacement). Called at every backward
/// branch; bumps the per-activation back-edge counter and, **exactly when** it
/// reaches `osr_threshold`, compiles (or reuses) an OSR entry at `loop_header` and
/// resumes there with the live register state. Returns `Some(outcome)` if OSR took
/// over (the function ran to completion natively), `None` to keep interpreting.
///
/// OSR only applies to translatable functions (guaranteed no `ref`/`out` params →
/// no `LoadLocalAddr` → no exit copy-out to skip), so returning here without the
/// interpreter's normal exit path is correct. `frame.regs` is cloned into the OSR
/// frame; block `0..K` results the interpreter already computed live there.
#[cfg(feature = "jit")]
fn try_osr(ctx: &VmContext, frame: &mut Frame, func: &Function, loop_header: usize)
    -> Option<Result<ExecOutcome>>
{
    let p = ctx.jit_ctx_ptr();
    if p == 0 { return None; }                        // interp-only mode: no OSR
    frame.back_edge_count = frame.back_edge_count.wrapping_add(1);
    let jit_ctx = p as *const crate::jit::frame::JitModuleCtx;
    // SAFETY: jit_ctx is valid for the whole JitModule::run_fn (set/cleared in
    // lockstep with vm_ctx). Only touched through &-methods / Copy field reads.
    let threshold = unsafe { (*jit_ctx).osr_threshold };
    if frame.back_edge_count != threshold { return None; }   // fire exactly once
    // v1: OSR only merged functions — resolve this function's merged id by name.
    let id = unsafe { (*(*jit_ctx).module).func_index.get(&func.name).copied() }?;
    let entry = unsafe { (*jit_ctx).resolve_osr_entry(id, loop_header) }?; // owned FnEntry
    ctx.counters().jit_native_from_interp.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut osr = crate::jit::frame::JitFrame::from_interp_regs(&frame.regs, entry.max_reg);
    // add-struct-jit-value-path (P5): OSR continues the SAME logical activation, so
    // the native frame must inherit the interp frame's id — any StructRef the loop
    // already allocated (frame_id = interp's) must still deref after hand-off, and
    // new struct allocs in OSR code stay consistent with them.
    osr.frame_id = frame.frame_id;
    let jit_fn: crate::jit::helpers::JitFn = unsafe { std::mem::transmute(entry.ptr) };
    // NB v1 simplification: the interpreter's own VmFrame for this activation is
    // still on the stack; we push a second one for the OSR native frame. GC scans
    // both — the interp regs are clones of the OSR regs (same heap refs), so the
    // double-scan is conservatively correct. A crash trace shows the frame twice
    // (cosmetic). Popped here; the interp frame's guard pops on the `return` below.
    ctx.push_frame(crate::exception::VmFrame::new(
        entry.name, entry.file, &osr.regs as *const _, &osr.env_arena as *const _));
    let r = unsafe { jit_fn(&mut osr, jit_ctx) };
    ctx.pop_frame();
    if r != 0 {
        osr.recycle();
        return Some(Ok(ExecOutcome::Thrown(ctx.take_exception().unwrap_or(Value::Null))));
    }
    let ret = osr.ret.take();
    osr.recycle();
    Some(Ok(ExecOutcome::Returned(ret)))
}

#[cfg(not(feature = "jit"))]
#[inline]
fn try_osr(_ctx: &VmContext, _frame: &mut Frame, _func: &Function, _loop_header: usize)
    -> Option<Result<ExecOutcome>> { None }

/// perf-vm-iteration Phase 1 (Decision 3): hot direct-call entry that fills the
/// callee register file **directly** from the caller's registers + argument
/// indices — no intermediate `collect_args` `Vec<Value>` alloc, and each arg is
/// cloned **once** (caller reg → callee reg) instead of twice (caller reg →
/// args Vec → callee reg). Mirrors the JIT's `JitFrame::new_args_from`
/// (jit/helpers/call.rs). Used by the non-virtual `Call` path (exec_call::call),
/// which passes plain register indices with no receiver prepend.
pub(crate) fn exec_function_from_regs(
    ctx: &VmContext, module: &Module, func: &Function,
    caller_regs: &[Value], arg_indices: &[u32],
    // add-generic-methods: resolved FQ type-arg names for a generic call (empty
    // for non-generic). Stored on the callee frame for MethodTypeArg/MethodDefault.
    method_type_args: &[String],
) -> Result<ExecOutcome> {
    crate::gc::safepoint::check_safepoint(ctx);
    let mut frame = Frame::new_from_regs(caller_regs, arg_indices, func.max_reg)?;
    if !method_type_args.is_empty() { frame.method_type_args = method_type_args.into(); }
    exec_function_body(ctx, module, func, frame)
}

/// perf-vm-iteration Phase 1 (Decision 3): virtual-call hot-path entry. Fills
/// `regs[0] = receiver`, `regs[1+i] = caller_regs[arg_indices[i]]` directly —
/// no `vec![receiver]` / `collect_args` Vecs, each value cloned once. Used by
/// the `exec_vcall` object/primitive IC fast path.
pub(crate) fn exec_function_from_receiver_regs(
    ctx: &VmContext, module: &Module, func: &Function,
    receiver: &Value, caller_regs: &[Value], arg_indices: &[u32],
    method_type_args: &[String],   // add-generic-methods: see exec_function_from_regs
) -> Result<ExecOutcome> {
    crate::gc::safepoint::check_safepoint(ctx);
    let mut frame = Frame::new_from_receiver_regs(receiver, caller_regs, arg_indices, func.max_reg)?;
    if !method_type_args.is_empty() { frame.method_type_args = method_type_args.into(); }
    exec_function_body(ctx, module, func, frame)
}

fn exec_function_body(ctx: &VmContext, module: &Module, func: &Function, mut frame: Frame) -> Result<ExecOutcome> {
    // perf-lazy-resolve-tokens (2026-08-18): populate this function's per-site
    // dispatch caches on first execution. `resolve_module` (Vm::run) only walks
    // the *entry* module; lazily-loaded packages (all of z42c.semantics /
    // z42c.syntax during a self-compile) never pass through it, so without this
    // their VCall PIC / FieldIC / builtin-id / static-id / call-token caches
    // stay dead and every dispatch falls back to string hashing. One relaxed
    // atomic load on the hot path (already re-read by the instruction loop);
    // the resolve body runs once per function (OnceLock-gated). `module` is the
    // entry module (invariant threaded from Vm::run), matching the runtime
    // dispatch module so `method_tokens` indices stay valid.
    if func.resolved.get().is_none() {
        crate::metadata::resolver::resolve_function_tokens(func, module, ctx);
    }
    // Spec impl-ref-out-in-runtime (Decision R2 architecture E):
    // 入口 copy-in：扫描 params，对每个持 Value::Ref 的 reg：
    //   1. 通过 RefKind 解引用得到底层值
    //   2. 将 reg[i] 替换为底层值（callee 体内所有指令读到的是普通 Value）
    //   3. 把原 RefKind 存入 frame.ref_writebacks，留给出口 copy-out 用
    // 这样 callee 的 80+ 指令 handler 完全不需要感知 Ref —— 透明性由
    // 入口/出口完成，仅一次 Vec 分配代价。
    // 注意：deref 必须在 push_frame_state 之前完成，因为此时 callee 自己的
    // frame 尚未入栈，Ref::Stack { frame_idx } 指向 caller 的栈位置，
    // ctx.frame_state_at 能找到正确 regs 指针。
    for i in 0..(func.param_count as usize).min(frame.regs.len()) {
        // make-value-copy: Ref is a transient-arena handle — resolve its RefKind via the
        // arena (the handle was created in the caller's frame, still live here).
        if let Value::Ref { idx, frame_id } = frame.regs[i] {
            let kind_clone = ctx.transient_arena.lock().ref_kind(idx, frame_id)?;
            let underlying = deref_ref(&kind_clone, ctx)?;
            frame.regs[i] = underlying;
            frame.ref_writebacks.push((i as u32, kind_clone));
        }
    }

    // 2026-05-10 unify-frame-chain: single push enrolling this frame as
    // GC root + stack-trace metadata in one VmFrame entry. file taken
    // from the line_table's first entry; falls back to empty when the
    // emitter omits redundant file references.
    //
    // SAFETY: regs / env_arena Vec live inside `frame` on the Rust call
    // stack; raw pointers stay valid until this function returns.
    // FrameGuard's Drop pops on every exit path (`?` propagation, panic
    // unwind, normal return).
    // perf-frame-name-precompute: clone the load-time precomputed (name, file)
    // Arc<str> pair — O(1) refcount bumps — instead of re-formatting the frame
    // name (String alloc + format) + cloning the file string on every call
    // (was 40–60% of call-heavy interp time). Hand-built test functions have no
    // precomputed meta (`None`) → fall back to formatting on the fly.
    let (frame_name, frame_file) = match &func.frame_meta {
        Some((name, file)) => (name.clone(), file.clone()),
        None => {
            let file = func.line_table().first()
                .and_then(|e| e.file.clone())
                .unwrap_or_default();
            (
                std::sync::Arc::from(crate::metadata::bytecode::format_frame_name(func)),
                std::sync::Arc::from(file),
            )
        }
    };
    // add-escape-analysis-stack-alloc: stamp this frame's monotonic id (keys any
    // stack-allocated objects/arrays it creates, for stale-handle diagnostics).
    frame.frame_id = ctx.next_frame_id();
    ctx.push_frame(crate::exception::VmFrame::new(
        frame_name,
        frame_file,
        &frame.regs as *const Vec<Value>,
        &frame.env_arena as *const Vec<Vec<Value>>,
    ));
    let _frame_guard = FrameGuard { ctx };

    // Spec C2: scope `CURRENT_VM` to this z42 frame so `z42_*` extern
    // entry points fired by native callbacks can locate the active VM.
    // The guard nests safely if a native callback re-enters z42 through
    // `exec_function`; on exit the previous pointer is restored.
    //
    // 2026-05-12 add-platform-wasm Stage 0: only relevant when
    // `native-interop` is enabled — wasm builds have no native callbacks
    // to dispatch into z42, so the guard is omitted.
    #[cfg(feature = "native-interop")]
    let _vm_guard = crate::native::exports::VmGuard::enter(ctx);

    // unify-gc-heap PR-4 (D11): scope this frame's heap as the ambient GC heap so
    // heap-less `Str::new` / `.into()` sites can allocate GC string blocks. Nests
    // safely (restored on exit); covers nested JIT calls, which run under this
    // guard without re-installing one.
    let _heap_guard = crate::gc::ambient::HeapGuard::enter(ctx.heap());

    let block_map = &func.block_index;
    let mut block_idx = 0usize;

    'exec: loop {
        let block = func
            .blocks
            .get(block_idx)
            .with_context(|| format!("block index {block_idx} out of range"))?;

        // interp-superinstr-fusion: if this block ends in a recognized fused tail
        // (v1: `cmp`+`BrCond` → `CmpBr`), run all-but-the-last instruction in the
        // loop below, then the fused step after it — skipping one instruction
        // dispatch + the bool reload per hot-loop iteration. Empty `fused_tails`
        // (hand-built test fns) ⇒ `None` ⇒ normal execution.
        let fused = func.fused_tails.get(block_idx).and_then(|o| o.as_ref());
        let n_instr = block.instructions.len() - if fused.is_some() { 1 } else { 0 };

        for (instr_idx, instr) in block.instructions[..n_instr].iter().enumerate() {
            // exec_instr returns:
            //   Ok(None)       — normal instruction completion
            //   Ok(Some(val))  — a callee threw an exception (value-based propagation)
            //   Err(e)         — internal VM error
            //
            // (block_idx, instr_idx, func) are passed through for the
            // introduce-method-token Phase 4 dispatch fast path: helpers
            // that need cache lookup index `func.resolved.site_index[block_idx]
            // [instr_idx]` to find their per-kind cache slot.
            match exec_instr::exec_instr(ctx, module, &mut frame, func, block_idx, instr_idx, instr) {
                Ok(None) => {}
                Ok(Some(thrown_val)) => {
                    // 2026-05-10 exception-stack-trace: callees may throw
                    // via JIT's set_exception path which doesn't run
                    // Terminator::Throw — populate here too. Idempotent
                    // (null-check skips already-populated objects).
                    crate::exception::populate_stack_trace(&thrown_val, ctx, module);

                    // User exception from a callee — try to find a local handler
                    if let Some(entry_idx) = find_handler(
                        ctx, func, block_idx, block_map, &module.type_registry, &thrown_val,
                    ) {
                        let entry = &func.exception_table()[entry_idx];
                        // Phase 2 D3+D6: callee-thrown + caller-caught is
                        // an unwind of exactly 1 frame (the callee). Throw
                        // was already counted/emitted at its origin
                        // (Terminator::Throw or JIT set_exception bridge,
                        // tracked separately when the JIT path also wires
                        // in Phase 2.x).
                        fire_exception_caught(ctx, module, &thrown_val, 1);
                        frame.set(entry.catch_reg, thrown_val);
                        block_idx = *block_map.get(entry.catch_label.as_str())
                            .with_context(|| format!("undefined block `{}`", entry.catch_label))?;
                        continue 'exec;
                    }
                    // No handler — propagate up as ExecOutcome::Thrown (no anyhow allocation)
                    // Spec impl-ref-out-in-runtime: writebacks still run on
                    // throw paths so any modifications callee made to ref/out
                    // params before the throw are visible to caller (matches
                    // C# DA model: caller in catch block sees mutations).
                    run_ref_writebacks(&frame, ctx)?;
                    return Ok(ExecOutcome::Thrown(thrown_val));
                }
                Err(e) => {
                    // Internal error — enrich with source location.
                    // (consolidate-vm-state, 2026-04-28: removed legacy
                    // UserException sentinel branch — JIT helpers now report
                    // exceptions via `ctx.set_exception` + extern-C return code,
                    // not via `anyhow::Error` wrapping.)
                    //
                    // fix-vm-error-display-loses-cause (2026-05-24): inline the
                    // original error message into the location string instead
                    // of `e.context(...)`. anyhow's `.context()` makes the
                    // location the *new* topmost message, and downstream
                    // consumers that print via `Display` (`{e}`) show ONLY the
                    // topmost — losing the actual bug. Pre-fix, every "VM
                    // error" in test output read `"  at <fn> (line X)"` with
                    // no clue what blew up. Format change: include the cause
                    // first, then location, separated by `\n  at`.
                    let (line, col) = resolve_line(func.line_table(), block_idx as u32, instr_idx as u32);
                    let loc_str = if col > 0 { format!("line {line}, col {col}") } else { format!("line {line}") };
                    return Err(anyhow::anyhow!("{}\n  at {} ({})", e, func.name, loc_str));
                }
            }
        }

        // interp-superinstr-fusion: run the recognized fused tail in place of the
        // last instruction + terminator, using the SHARED `ops::eval_cmp` (same
        // primitive the standalone cmp handlers use). Preserves the back-edge
        // safepoint + OSR hand-off exactly as the `BrCond` terminator does.
        if let Some(crate::metadata::superinstr::SuperInstr::CmpBr { op, a, b, dst, t_blk, f_blk, typed }) = fused {
            // interp-typed-superinstr: `typed` ⇒ both operands statically I64 →
            // unchecked i64 compare (no discriminant branch). Else the dynamic
            // `eval_cmp` (same primitive the standalone cmp handlers use).
            let cond = if *typed {
                ops::eval_cmp_i64(*op, &frame.regs, *a, *b)
            } else {
                ops::eval_cmp(*op, &frame.regs, *a, *b)?
            };
            frame.set(*dst, Value::Bool(cond)); // keep dst written — safe for any other reader
            let target = if cond { *t_blk } else { *f_blk };
            if target <= block_idx {
                crate::gc::safepoint::check_safepoint(ctx);
                if let Some(outcome) = try_osr(ctx, &mut frame, func, target) {
                    return outcome;
                }
            }
            block_idx = target;
            continue 'exec;
        }

        match &block.terminator {
            Terminator::Ret { reg: None }      => {
                run_ref_writebacks(&frame, ctx)?;
                return Ok(ExecOutcome::Returned(None));
            }
            Terminator::Ret { reg: Some(r) }   => {
                let ret_val = frame.get(*r)?.clone();
                run_ref_writebacks(&frame, ctx)?;
                return Ok(ExecOutcome::Returned(Some(ret_val)));
            }
            Terminator::Br  { label }          => {
                // perf-vm-iteration: jump by pre-resolved index (no per-branch
                // SipHash on the label); fall back to the label map if the
                // targets weren't precomputed (hand-built test functions).
                let target = match func.branch_targets.get(block_idx) {
                    Some(BranchTargets::Br(t)) => *t,
                    _ => *block_map.get(label.as_str())
                        .with_context(|| format!("undefined block `{label}`"))?,
                };
                // add-gc-safepoint (2026-05-20): backward branch heuristic
                // — block index decreasing is a loop back-edge. Check
                // safepoint so long-running loops park promptly.
                if target <= block_idx {
                    crate::gc::safepoint::check_safepoint(ctx);
                    // add-osr-loop-tiering: hot loop → hand off to native (OSR).
                    if let Some(outcome) = try_osr(ctx, &mut frame, func, target) {
                        return outcome;
                    }
                }
                block_idx = target;
            }
            Terminator::BrCond { cond, true_label, false_label } => {
                let go_true = match frame.get(*cond)? {
                    Value::Bool(b) => *b,
                    other => bail!("BrCond expects bool, got {:?}", other),
                };
                let target = match func.branch_targets.get(block_idx) {
                    Some(BranchTargets::BrCond(t, f)) => if go_true { *t } else { *f },
                    _ => {
                        let label = if go_true { true_label } else { false_label };
                        *block_map.get(label.as_str())
                            .with_context(|| format!("undefined block `{label}`"))?
                    }
                };
                if target <= block_idx {
                    crate::gc::safepoint::check_safepoint(ctx);
                    // add-osr-loop-tiering: hot loop → hand off to native (OSR).
                    if let Some(outcome) = try_osr(ctx, &mut frame, func, target) {
                        return outcome;
                    }
                }
                block_idx = target;
            }
            Terminator::Throw { reg } => {
                let val = frame.get(*reg)?.clone();
                // 2026-05-10 exception-stack-trace: stamp the throwing
                // frame's current line so the snapshot's top entry shows
                // the throw site (not whatever the previous Call left).
                // Throw is a block terminator; instr_idx isn't a meaningful
                // intra-block offset, so use end-of-block (block.instructions.len()).
                let (throw_line, throw_col) = resolve_line(
                    func.line_table(),
                    block_idx as u32,
                    block.instructions.len() as u32,
                );
                // add-offline-symbolication: stamp line/col + throw-site offset
                // (end-of-block terminator slot) in one lock so stripped traces resolve.
                ctx.update_top_frame_pos(throw_line, throw_col,
                    func.linear_offset(block_idx as u32, block.instructions.len() as u32));
                crate::exception::populate_stack_trace(&val, ctx, module);

                // Phase 2 D3+D6 wiring (2026-05-26): count + emit the throw
                // BEFORE handler lookup so even immediately-caught exceptions
                // are visible in the event stream.
                fire_exception_thrown(ctx, module, &val);

                if let Some(entry_idx) = find_handler(
                    ctx, func, block_idx, block_map, &module.type_registry, &val,
                ) {
                    let entry = &func.exception_table()[entry_idx];
                    // Same-frame catch — 0 frames unwound.
                    fire_exception_caught(ctx, module, &val, 0);
                    frame.set(entry.catch_reg, val);
                    block_idx = *block_map.get(entry.catch_label.as_str())
                        .with_context(|| format!("undefined block `{}`", entry.catch_label))?;
                } else {
                    // No local handler — propagate via value, not anyhow
                    run_ref_writebacks(&frame, ctx)?;
                    return Ok(ExecOutcome::Thrown(val));
                }
            }
        }
    }
}

/// Spec impl-ref-out-in-runtime: copy-out for `ref`/`out` params. Iterate
/// `frame.ref_writebacks`; for each `(reg, original_ref_kind)`, take the
/// callee's final value of that reg and store it through the original Ref
/// to the caller's lvalue. Runs before every function-exit return path
/// (normal return + uncaught throw).
/// Phase 2 D3+D6 (2026-05-26): increment `exceptions_thrown` counter and
/// fire `RuntimeEvent::ExceptionThrown`. Reads class name + message from
/// the thrown value if it's an Exception subclass; otherwise stamps both
/// as `"<non-exception-value>"`. Message truncated to 256 chars to keep
/// the event firehose bounded.
fn fire_exception_thrown(ctx: &VmContext, module: &crate::metadata::Module, val: &crate::metadata::Value) {
    use std::sync::atomic::Ordering;
    ctx.counters().exceptions_thrown.fetch_add(1, Ordering::Relaxed);
    let (class, mut message) = exception_class_and_message(val, module);
    if message.len() > 256 {
        message.truncate(256);
        message.push_str("…");
    }
    ctx.fire_runtime_event(&crate::observer::RuntimeEvent::ExceptionThrown { class, message });
}

/// Phase 2 D3+D6 sibling: increment `exceptions_caught` + fire
/// `RuntimeEvent::ExceptionCaught`. `frames_unwound` = 0 for same-frame
/// catch; 1 for callee-thrown + caller-caught; >1 for deeper unwind.
fn fire_exception_caught(
    ctx: &VmContext, module: &crate::metadata::Module,
    val: &crate::metadata::Value, frames_unwound: u32,
) {
    use std::sync::atomic::Ordering;
    ctx.counters().exceptions_caught.fetch_add(1, Ordering::Relaxed);
    let (class, _) = exception_class_and_message(val, module);
    ctx.fire_runtime_event(&crate::observer::RuntimeEvent::ExceptionCaught { class, frames_unwound });
}

fn exception_class_and_message(
    val: &crate::metadata::Value, module: &crate::metadata::Module,
) -> (String, String) {
    use crate::metadata::Value;
    let class = match val {
        Value::Object(rc) => rc.type_desc().name.clone(),
        _ => "<non-exception-value>".to_string(),
    };
    let message = crate::exception::read_message(val, module).unwrap_or_default();
    (class, message)
}

fn run_ref_writebacks(frame: &Frame, ctx: &VmContext) -> Result<()> {
    for (reg, kind) in &frame.ref_writebacks {
        let final_val = frame.regs.get(*reg as usize)
            .cloned()
            .unwrap_or(Value::Null);
        store_thru_ref(kind, final_val, ctx)?;
    }
    Ok(())
}

/// Find the index into `func.exception_table` of the first handler whose try
/// region covers `block_idx` AND whose declared `catch_type` matches the thrown
/// value's class (with subclass walk via the type registry).
///
/// catch-by-generic-type (2026-05-06): catch_type semantics —
///   None       — wildcard (user wrote `catch { }` / `catch (e)`); always matches.
///   Some("*")  — synthetic finally fallthrough catchall (compiler-generated
///                when there is no user catch but a finally block exists).
///   Some(t)    — typed catch; matches when the thrown value is an instance of
///                class `t` or any of its subclasses (sibling lineages skipped).
///
/// Source-order is preserved: exception_table entries are written in catch-clause
/// order by FunctionEmitterStmts; this loop scans them in that order and returns
/// the first match — matching C# / Java first-source-match-wins semantics.
///
/// `thrown` is expected to be a `Value::Object` (z42 throw is restricted to
/// Exception-derived class instances); non-object throws fall through to the
/// untyped catches via the wildcard branches above.
fn find_handler(
    ctx: &VmContext,
    func: &Function,
    block_idx: usize,
    block_map: &HashMap<String, usize>,
    type_registry: &rustc_hash::FxHashMap<String, std::sync::Arc<crate::metadata::TypeDesc>>,
    thrown: &Value,
) -> Option<usize> {
    let thrown_class: Option<String> = match thrown {
        Value::Object(rc) => Some(rc.type_desc().name.clone()),
        _                 => None,
    };

    for (i, entry) in func.exception_table().iter().enumerate() {
        let start_idx = *block_map.get(&entry.try_start)?;
        let end_idx   = *block_map.get(&entry.try_end)?;
        if !(block_idx >= start_idx && block_idx < end_idx) { continue; }

        match entry.catch_type.as_deref() {
            None      => return Some(i),                   // user untyped catch
            Some("*") => return Some(i),                   // synthetic finally fallthrough
            Some(target) => {
                if let Some(ref derived) = thrown_class {
                    if dispatch::is_subclass_or_eq_td(ctx, type_registry, derived, target) {
                        return Some(i);
                    }
                }
                // type mismatch — try next entry
            }
        }
    }
    None
}
