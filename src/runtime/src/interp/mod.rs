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
/// • vcall_resolve.rs — VCall target resolution (shared with jit/helpers/vcall.rs)
/// • exec_vcall.rs   — VCall invoke side + primitive_class_name + is_array_isa
/// • exec_native.rs  — CallNative / CallNativeVtable / PinPtr / UnpinPtr
/// • dispatch.rs   — object dispatch helpers (vtable, ToString, static fields)
/// • ops.rs        — register-level helpers (int_binop, collect_args, …)

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
pub(crate) mod vcall_resolve;   // unify-vcall-resolution: shared with jit/helpers/vcall.rs
mod ops;
pub(crate) mod stack_alloc;   // add-escape-analysis-stack-alloc: per-context stack arena
pub(crate) mod struct_arena;  // add-struct-value-semantics: per-context byte arena for value structs
pub(crate) mod transient_arena; // make-value-copy: per-context arena for Ref/PinnedView/StackClosure/StructRefHeap

// Re-export for cross-module callers (notably jit/helpers_object.rs).
pub(crate) use exec_vcall::primitive_class_name;
// JIT primitive-receiver VCall IC (add-jit-primitive-vcall-ic): jit_vcall keys its
// inline cache on primitives via the same synthetic PRIM_TYPE_* ids interp uses.
pub(crate) use exec_object::prim_isa;   // fix-boxed-primitive-is-as: JIT is/as 复用

pub use crate::corelib::convert::value_to_str;
use crate::metadata::{BranchTargets, Function, Module, Terminator, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Context, Result};

// ── Execution outcome ────────────────────────────────────────────────────────

// refactor-split-interp-mod（2026-09-03）：入口 / Frame / 执行支撑三块搬到子模块，本文件只留执行主循环。
mod entry;
mod frame;
mod exec_support;
pub use entry::{ExecOutcome, run, run_returning, run_outcome, init_static_fields, run_with_static_init};
pub(crate) use frame::*;
pub(crate) use exec_support::*;

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
