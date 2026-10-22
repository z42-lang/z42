/// Interpreter backend — register-based bytecode interpreter: walks a function's
/// basic blocks and dispatches each `Instruction` through one `match`
/// (`exec_instr`), with operands addressed as `frame.regs[reg]`.
///
/// Implementation is split across submodules:
/// • mod.rs        — public API, Frame, core execution loop
/// • exec_instr.rs — thin per-Instruction dispatcher (exhaustive match → helpers)
/// • exec_body_cold.rs — the loop's out-of-line slow paths (ref copy-in, unwinding, error location)
/// • exec_value.rs — constants / copy / arith / cmp / logical / unary / bitwise / string
/// • exec_address.rs — LoadLocalAddr / LoadElemAddr / LoadFieldAddr / DefaultOf
/// • exec_call.rs    — Call / Builtin / LoadFn / CallIndirect / MkClos
/// • exec_array.rs   — ArrayNew / ArrayNewLit / ArrayGet / ArraySet / ArrayLen
/// • exec_object.rs  — ObjNew / FieldGet / FieldSet / IsInstance / AsCast / Static*
/// • vcall_resolve.rs — VCall target resolution (shared with jit/helpers/vcall.rs)
/// • obj_new_resolve.rs — ObjNew class / ctor resolution + site cache (shared with jit_obj_new)
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
pub(crate) mod forward;         // runtime-audit P1-8: `[Native]` stub folding, shared with the JIT
pub(crate) mod obj_new_resolve; // P1-2 PR 5: ObjNew class / ctor + site cache, shared with jit_obj_new
mod ops;
pub(crate) mod stack_alloc;   // add-escape-analysis-stack-alloc: per-context stack arena
pub(crate) mod struct_arena;  // add-struct-value-semantics: per-context byte arena for value structs
pub(crate) mod transient_arena; // make-value-copy: per-context arena for Ref/PinnedView/StructRefHeap

// Re-export for cross-module callers (notably jit/helpers_object.rs).
pub(crate) use exec_vcall::primitive_class_name;
#[cfg(feature = "jit")]
pub(crate) use exec_call::bind_callee;   // P1-2: jit_call binds by name through the same path
// P1-2 PR 8: function-reference targets bind through one path (LoadFn / MkClos in both
// backends, `__make_closure`).
pub(crate) use exec_call::fn_ref_id;
// JIT primitive-receiver VCall IC (add-jit-primitive-vcall-ic): jit_vcall keys its
// inline cache on primitives via the same synthetic PRIM_TYPE_* ids interp uses.
pub(crate) use exec_object::prim_isa;   // fix-boxed-primitive-is-as: JIT is/as 复用

pub use crate::corelib::convert::value_to_str;
use crate::metadata::{BranchTargets, Function, Module, Terminator, Value};
use crate::vm_context::VmContext;
use anyhow::{bail, Context, Result};

// ── Execution outcome ────────────────────────────────────────────────────────

// refactor-split-interp-mod：入口 / Frame / 执行支撑三块搬到子模块，本文件只留执行主循环。
mod entry;
mod frame;
mod exec_support;
mod exec_body_cold;
use exec_body_cold::{callee_threw, copy_in_ref_param, label_target, locate_error, throw_here, Unwind};
pub use entry::{ExecOutcome, run, run_returning, run_outcome, init_static_fields, run_with_static_init};
pub(crate) use frame::*;
pub(crate) use exec_support::*;

fn exec_function_body(ctx: &VmContext, module: &Module, func: &Function, mut frame: Frame) -> Result<ExecOutcome> {
    // Spec impl-ref-out-in-runtime (Decision R2 architecture E): entry copy-in — each
    // param register holding a `Value::Ref` is replaced by the underlying value (so no
    // instruction handler sees a `Ref`), its kind kept for the exit copy-out. Must run
    // before `enter_frame` (see `copy_in_ref_param`).
    for i in 0..(func.param_count as usize).min(frame.regs.len()) {
        if let Value::Ref { idx, frame_id } = frame.regs[i] {
            copy_in_ref_param(ctx, &mut frame, i, idx, frame_id)?;
        }
    }

    // Enrol `frame` as GC root + stack-trace row; the guard pops it on every exit
    // path. `enter_frame` also runs the (fatal) stack-overflow check, after the push.
    // The VM / heap thread-locals (`CURRENT_VM`, the ambient heap) are scoped by the
    // context while its stack is non-empty — the bottom frame's push installs them
    // (`vm_context/engine_guards.rs`), so a nested frame touches no thread-local.
    let _frame_guard = enter_frame(ctx, func, &mut frame)?;

    // add-gc-safepoint : every newly-entered z42 function immediately respects a
    // pending GC request — a worker thread spawned mid-collect parks here before touching any
    // roots.
    //
    // **fix-callee-entry-safepoint-drops-args **: this check used to sit at the
    // four `exec_function*` entry points, *before* the frame existed. The callee's arguments
    // live only in a caller-side temporary there — for `new T(..)` that temporary holds the
    // sole reference to the just-allocated receiver — so a collection at that point swept
    // values the mutator was still using. The frame is the thing that makes the callee's
    // registers a GC root, so the safepoint has to come after `push_frame`, never before.
    //
    // Nothing between `Frame::new` and this line can collect: the ref/out copy-in above only
    // reads, and `resolve_function_tokens` (which *can* run z42 static initialisers, and
    // therefore reach a safepoint) is deliberately below.
    crate::gc::safepoint::check_safepoint(ctx);

    // perf-lazy-resolve-tokens : populate this function's per-site dispatch caches
    // on first execution. `resolve_module` (Vm::run) only walks the *entry* module; lazily
    // loaded packages (all of z42c.semantics / z42c.syntax during a self-compile) never pass
    // through it, so without this their VCall PIC / FieldIC / builtin-id / static-id /
    // call-token caches stay dead and every dispatch falls back to string hashing. One relaxed
    // atomic load on the hot path (already re-read by the instruction loop); the resolve body
    // runs once per function (OnceLock-gated). `module` is the entry module (invariant threaded
    // from `Vm::run`), matching the runtime dispatch module so `method_tokens` indices stay
    // valid.
    //
    // Runs *after* the frame is enrolled (fix-callee-entry-safepoint-drops-args): it drains the
    // static-initialiser queue, which executes z42 code and so can reach a safepoint — with the
    // frame already a root, this callee's arguments survive that collection.
    if func.resolved.get().is_none() {
        // defer-class-initialization (T3): `resolve_function_tokens` 内部会在发布
        // `resolved` 之前排空「静态字段所属类」队列并跑完它们的初始化器，
        // 所以这里不需要再排空一次（放在那里才对并发安全，见 resolver.rs 的注释）。
        crate::metadata::resolver::resolve_function_tokens(func, module, ctx);
    }

    // Read once per activation (a `OnceLock`: once set — at the latest by the resolve
    // above — it never changes), not once per instruction.
    let resolved = func.resolved.get();
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
            // (resolved, block_idx, instr_idx) are passed through for the
            // introduce-method-token Phase 4 dispatch fast path: helpers
            // that need cache lookup index `resolved.site_index[block_idx]
            // [instr_idx]` to find their per-kind cache slot.
            match exec_instr::exec_instr(ctx, module, &mut frame, func, resolved, block_idx, instr_idx, instr) {
                Ok(None) => {}
                Ok(Some(thrown_val)) => {
                    // User exception from a callee: an unwind of exactly 1 frame (the callee)
                    // if a local handler catches it.
                    match callee_threw(ctx, module, func, &mut frame, block_idx, thrown_val)? {
                        Unwind::Catch(target) => { block_idx = target; continue 'exec; }
                        Unwind::Propagate(val) => return Ok(ExecOutcome::Thrown(val)),
                    }
                }
                Err(e) => return Err(locate_error(e, func, block_idx, instr_idx)),
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
                    _ => label_target(&func.block_index, label)?,
                };
                // add-gc-safepoint : backward branch heuristic
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
                    _ => label_target(&func.block_index, if go_true { true_label } else { false_label })?,
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
                match throw_here(ctx, module, func, &mut frame, block_idx, val)? {
                    Unwind::Catch(target) => block_idx = target,
                    Unwind::Propagate(val) => return Ok(ExecOutcome::Thrown(val)),
                }
            }
        }
    }
}
