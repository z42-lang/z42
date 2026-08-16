/// Cranelift IR translation: z42 SSA bytecode → native machine code.
///
/// One z42 basic block maps to one Cranelift block.
/// All value-level operations are dispatched to `extern "C"` helper functions
/// (see `helpers/`). Only branches, jumps, and function entry/exit are
/// emitted as inline Cranelift instructions.

use crate::metadata::{Function, Instruction, Terminator};
use crate::metadata::{
    AsCastInsn, BuiltinInsn, CallInsn, CallNativeInsn, FieldGetInsn, FieldSetInsn, IsInstanceInsn,
    LoadFnCachedInsn, LoadFnInsn, MkClosInsn, ObjNewInsn, StaticGetInsn, StaticSetInsn, TypeofInsn,
    VCallInsn,
};
use anyhow::{bail, Result};
use cranelift_codegen::ir::{AbiParam, InstBuilder, MemFlags};
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::condcodes::IntCC;
use crate::metadata::IrType;
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_module::{FuncId, Module as CraneliftModule};
use cranelift_jit::JITModule;

pub use super::helpers::HelperIds;
use super::reg_access::{
    load_payload, load_payload_i64, load_tag, reg_addr, store_const_tag, store_tag_const,
    store_tagged, RegCache, TAG_BOOL, TAG_CHAR, TAG_F64, TAG_I64, TAG_NULL,
};

// ═════════════════════════════════════════════════════════════════════════════
// max_reg — largest register index used in a function
// ═════════════════════════════════════════════════════════════════════════════

/// The register an instruction writes (its `dst`), or `None` for stores that
/// write no register (`ArraySet` / `FieldSet` / `StaticSet` / `UnpinPtr`).
/// Single source of truth for "what does this op define" — used by `max_reg`
/// and the jit-inline-fastpaths never-reassigned scan.
pub fn written_reg(instr: &Instruction) -> Option<u32> {
    match instr {
        Instruction::ConstStr  { dst, .. } => Some(*dst),
        Instruction::ConstI32  { dst, .. } => Some(*dst),
        Instruction::ConstI64  { dst, .. } => Some(*dst),
        Instruction::ConstF64  { dst, .. } => Some(*dst),
        Instruction::ConstBool { dst, .. } => Some(*dst),
        Instruction::ConstChar { dst, .. } => Some(*dst),
        Instruction::ConstNull { dst }      => Some(*dst),
        Instruction::Copy      { dst, .. }  => Some(*dst),
        Instruction::Add       { dst, .. }  => Some(*dst),
        Instruction::Sub       { dst, .. }  => Some(*dst),
        Instruction::Mul       { dst, .. }  => Some(*dst),
        Instruction::Div       { dst, .. }  => Some(*dst),
        Instruction::Rem       { dst, .. }  => Some(*dst),
        Instruction::Eq        { dst, .. }  => Some(*dst),
        Instruction::Ne        { dst, .. }  => Some(*dst),
        Instruction::Lt        { dst, .. }  => Some(*dst),
        Instruction::Le        { dst, .. }  => Some(*dst),
        Instruction::Gt        { dst, .. }  => Some(*dst),
        Instruction::Ge        { dst, .. }  => Some(*dst),
        Instruction::And       { dst, .. }  => Some(*dst),
        Instruction::Or        { dst, .. }  => Some(*dst),
        Instruction::Not       { dst, .. }  => Some(*dst),
        Instruction::Neg       { dst, .. }  => Some(*dst),
        Instruction::BitAnd    { dst, .. }  => Some(*dst),
        Instruction::BitOr     { dst, .. }  => Some(*dst),
        Instruction::BitXor    { dst, .. }  => Some(*dst),
        Instruction::BitNot    { dst, .. }  => Some(*dst),
        Instruction::Shl       { dst, .. }  => Some(*dst),
        Instruction::Shr       { dst, .. }  => Some(*dst),
        Instruction::StrConcat { dst, .. }  => Some(*dst),
        Instruction::ToStr     { dst, .. }  => Some(*dst),
        Instruction::Call(insn)              => Some(insn.dst),
        Instruction::LoadLocalAddr { dst, .. } => Some(*dst),
        Instruction::LoadElemAddr  { dst, .. } => Some(*dst),
        Instruction::LoadFieldAddr(insn)       => Some(insn.dst),
        Instruction::DefaultOf     { dst, .. } => Some(*dst),
        Instruction::Builtin(insn)          => Some(insn.dst),
        Instruction::ArrayNew(insn)          => Some(insn.dst),
        Instruction::ArrayNewLit(insn)       => Some(insn.dst),
        Instruction::ArrayGet    { dst, .. } => Some(*dst),
        Instruction::ArraySet    { .. }      => None,
        Instruction::ArrayLen    { dst, .. } => Some(*dst),
        Instruction::ObjNew(insn)           => Some(insn.dst),
        Instruction::Typeof(insn)           => Some(insn.dst),
        Instruction::FieldGet(insn)         => Some(insn.dst),
        Instruction::FieldSet(_)            => None,
        Instruction::VCall(insn)            => Some(insn.dst),
        Instruction::IsInstance(insn)       => Some(insn.dst),
        Instruction::AsCast(insn)           => Some(insn.dst),
        Instruction::StaticGet(insn)        => Some(insn.dst),
        Instruction::StaticSet(_)           => None,
        Instruction::CallNative(insn)             => Some(insn.dst),
        Instruction::CallNativeVtable { dst, .. } => Some(*dst),
        Instruction::PinPtr           { dst, .. } => Some(*dst),
        Instruction::UnpinPtr         { .. }      => None,
        Instruction::LoadFn(insn)             => Some(insn.dst),
        Instruction::LoadFnCached(insn)       => Some(insn.dst),
        Instruction::CallIndirect { dst, .. } => Some(*dst),
        Instruction::MkClos(insn)             => Some(insn.dst),
        Instruction::Convert      { dst, .. } => Some(*dst),
        // add-struct-value-semantics Phase A: blob value type instructions.
        Instruction::StructAlloc(insn)              => Some(insn.dst),
        Instruction::StructCopy { dst, .. }         => Some(*dst),
        Instruction::StructFieldGetPrim { dst, .. } => Some(*dst),
        Instruction::StructFieldSetPrim { .. }      => None,
    }
}

pub fn max_reg(func: &Function) -> usize {
    let mut max = func.param_count.saturating_sub(1);
    // The compiler-authoritative reg count (`func.max_reg`, from zbc REGT) covers
    // EVERY register the function uses — including exception-table catch registers
    // (written by the runtime at `jit_install_catch`, never by an instruction) and
    // any read-only reg. The instruction scan below misses those: a catch reg only
    // shows up because some instruction happens to reference it, and an IR
    // optimization (e.g. compile-time copy-prop / DCE) can remove the last such
    // instruction — shrinking this recompute below the catch reg and OOB-panicking
    // `frame.regs[catch_reg]`. `func.max_reg` is a COUNT → max index = count - 1.
    if func.max_reg > 0 {
        max = max.max(func.max_reg as usize - 1);
    }
    // Exception-table catch registers are written by the runtime at
    // `jit_install_catch` (a direct `frame.regs[catch_reg] = ..` index, unlike
    // the interpreter's auto-resizing `frame.set`). A catch reg only otherwise
    // surfaces if some instruction happens to reference it, so IR optimizations
    // (copy-prop / DCE) that remove that last instruction leave the frame too
    // small → OOB panic. Fold every catch reg in so the frame always covers it.
    for e in func.exception_table() {
        if e.catch_reg as usize > max { max = e.catch_reg as usize; }
    }
    for block in &func.blocks {
        for instr in &block.instructions {
            let dst: Option<u32> = written_reg(instr);
            if let Some(d) = dst {
                if d as usize > max { max = d as usize; }
            }
        }
    }
    max
}

// ═════════════════════════════════════════════════════════════════════════════
// JIT-translatability pre-scan
// ═════════════════════════════════════════════════════════════════════════════

/// Return `Some(reason)` if `func` contains an opcode the JIT cannot yet
/// translate, otherwise `None`. Keep this list in lock-step with the `bail!`
/// arms in `translate_instr` below — every opcode that bails there must be
/// reported here so `compile_module` can skip the function up front rather than
/// abort mid-translation.
///
/// fix-jit-cross-zpkg-transitive-eager (2026-06-20): with `--mode jit` now the
/// default and eager loading pulling the *whole* transitive dep closure into
/// the module, the merged module routinely contains stdlib functions that use
/// `out`/`ref` params (`LoadLocalAddr`) or native interop (`CallNative`). Those
/// must degrade per-function to interp (jit_call misses `fn_entries` → the
/// `cross_zpkg_via_interp` fallback runs the bytecode), not fail the program.
pub fn jit_unsupported_reason(func: &Function) -> Option<&'static str> {
    for block in &func.blocks {
        for instr in &block.instructions {
            let reason = match instr {
                Instruction::CallNative(_)        => "CallNative",
                Instruction::CallNativeVtable { .. } => "CallNativeVtable",
                Instruction::PinPtr { .. }        => "PinPtr",
                Instruction::UnpinPtr { .. }      => "UnpinPtr",
                Instruction::LoadLocalAddr { .. } => "LoadLocalAddr",
                Instruction::LoadElemAddr { .. }  => "LoadElemAddr",
                Instruction::LoadFieldAddr(_)     => "LoadFieldAddr",
                _ => continue,
            };
            return Some(reason);
        }
    }
    None
}

// ═════════════════════════════════════════════════════════════════════════════
// Exception table helper
// ═════════════════════════════════════════════════════════════════════════════

/// Find every exception_table entry whose try region covers `block_idx`,
/// in source order. catch-by-generic-type (2026-05-06) requires the JIT to
/// see all covering entries (not just the first) so it can emit a typed-catch
/// chain that probes each candidate's `catch_type` against the thrown value's
/// class and jumps to the first matching handler.
fn find_handler_entries(func: &Function, block_idx: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for (i, entry) in func.exception_table().iter().enumerate() {
        let Some(start) = func.blocks.iter().position(|b| b.label == entry.try_start) else { continue };
        let Some(end)   = func.blocks.iter().position(|b| b.label == entry.try_end)   else { continue };
        if block_idx >= start && block_idx < end {
            out.push(i);
        }
    }
    out
}

// ═════════════════════════════════════════════════════════════════════════════
// translate_function
// ═════════════════════════════════════════════════════════════════════════════

/// formalize-jit-method-token Phase 2.C helper: look up the resolved
/// `MethodId.0` for a `Call` site. Returns `UNRESOLVED` (= u32::MAX)
/// for cross-zpkg lazy targets — `jit_call` falls back to name lookup.
fn method_id_at(func: &Function, block_idx: usize, instr_idx: usize) -> u32 {
    func.resolved.get()
        .and_then(|r| {
            let site = *r.site_index.get(block_idx)?.get(instr_idx)?;
            r.method_tokens.get(site as usize)
        })
        .map(|atom| atom.load(std::sync::atomic::Ordering::Relaxed))
        .unwrap_or(crate::metadata::tokens::UNRESOLVED)
}

/// make-vm-loading-lazy helper: stable raw pointer to the per-Call-site
/// `call_jit_ic` slot (an `AtomicU32` caching the resolved lazy/merged fn id)
/// for the `Call` site at `(block_idx, instr_idx)`. Returns null when
/// `Function.resolved` is unset (jit_call degrades to the by-name slow path).
/// The IC lives inside `Function.resolved` (inside Module) → valid for the
/// whole JitModule lifetime, like `vcall_ic_ptr_at`.
fn call_jit_ic_ptr_at(func: &Function, block_idx: usize, instr_idx: usize) -> *const std::sync::atomic::AtomicU32 {
    func.resolved.get()
        .and_then(|r| {
            let site = *r.site_index.get(block_idx)?.get(instr_idx)?;
            r.call_jit_ic.get(site as usize)
        })
        .map(|ic| ic as *const _)
        .unwrap_or(std::ptr::null())
}

/// formalize-jit-method-token Phase 2.E helper: stable raw pointer to
/// the `VCallIC` slot for the VCall site at `(block_idx, instr_idx)`.
/// Returns null when `Function.resolved` is unset (helper degrades to
/// non-IC slow path). The IC lives inside `Function.resolved.vcall_ic`
/// (which lives inside Module), so the pointer is valid for the entire
/// JitModule lifetime.
fn vcall_ic_ptr_at(func: &Function, block_idx: usize, instr_idx: usize) -> *const crate::metadata::resolver::VCallIC {
    func.resolved.get()
        .and_then(|r| {
            let site = *r.site_index.get(block_idx)?.get(instr_idx)?;
            r.vcall_ic.get(site as usize)
        })
        .map(|ic| ic as *const _)
        .unwrap_or(std::ptr::null())
}

/// formalize-jit-method-token Phase 2.E helper: stable raw pointer to
/// the `FieldIC` slot for a FieldGet/FieldSet site. Same lifetime
/// guarantees as `vcall_ic_ptr_at`.
fn field_ic_ptr_at(func: &Function, block_idx: usize, instr_idx: usize) -> *const crate::metadata::resolver::FieldIC {
    func.resolved.get()
        .and_then(|r| {
            let site = *r.site_index.get(block_idx)?.get(instr_idx)?;
            r.field_ic.get(site as usize)
        })
        .map(|ic| ic as *const _)
        .unwrap_or(std::ptr::null())
}

/// formalize-jit-method-token Phase 2 helper: look up the resolved
/// `StaticFieldId.0` for a `StaticGet` / `StaticSet` site at
/// `(block_idx, instr_idx)`, or `UNRESOLVED` when `Function.resolved` is unset.
///
/// make-vm-loading-lazy: a lazily-loaded function is JIT-compiled WITHOUT its
/// `resolved` token table (it never went through `resolve_module` — resolving
/// its method/type tokens against the lazy module would bind them to the wrong
/// indices; interp likewise leaves lazy functions unresolved). So this now
/// degrades to `UNRESOLVED` instead of panicking, and `jit_static_get/set`
/// resolve the field by NAME at runtime — mirroring interp's `exec_object`
/// `field_id: None` fallback. Static field ids are ctx-global (allocated by
/// name), so the by-name path is correct regardless of which module owns the fn.
fn static_field_id_at(func: &Function, block_idx: usize, instr_idx: usize) -> u32 {
    func.resolved.get()
        .and_then(|r| {
            let site = *r.site_index.get(block_idx)?.get(instr_idx)?;
            r.static_field_tokens.get(site as usize)
        })
        .map(|atom| atom.load(std::sync::atomic::Ordering::Relaxed))
        .filter(|&id| id != crate::metadata::tokens::UNRESOLVED)
        .unwrap_or(crate::metadata::tokens::UNRESOLVED)
}

pub fn translate_function(
    jit:          &mut JITModule,
    helper_ids:   &HelperIds,
    z42_func:     &Function,
    _func_max_reg: usize,
    func_id:      FuncId,
    // add-osr-loop-tiering: `Some(K)` compiles an **OSR variant** whose entry runs
    // the normal prologue (safepoint + cached `frame.regs` ptr + hoisted array
    // ptrs — all SSA values that must dominate the loop) and then jumps straight to
    // z42 block `K` (the hot loop header), skipping blocks `0..K` (already executed
    // by the interpreter, whose register state we inherit via `frame.regs` memory).
    // `None` = normal entry at block 0 (unchanged). The dedicated OSR entry block is
    // created only in the `Some` case, so the non-OSR path is byte-for-byte identical.
    osr_entry:    Option<usize>,
) -> Result<()> {
    let ptr     = jit.target_config().pointer_type();

    // Build Cranelift function signature: (frame_ptr, ctx_ptr) -> i8
    let mut cl_sig = jit.make_signature();
    cl_sig.params.push(AbiParam::new(ptr));
    cl_sig.params.push(AbiParam::new(ptr));
    cl_sig.returns.push(AbiParam::new(types::I8));

    let mut ctx = Context::new();
    ctx.func.signature = cl_sig;

    let mut fb_ctx = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut fb_ctx);

    // Create one Cranelift block per z42 block.
    let num_blocks = z42_func.blocks.len();
    let cl_blocks: Vec<cranelift_codegen::ir::Block> = (0..num_blocks)
        .map(|_| builder.create_block())
        .collect();

    // Entry: the block that carries the function params + prologue. Normally that
    // is z42 block 0; for an OSR variant it is a dedicated block that jumps to the
    // loop header after the prologue (add-osr-loop-tiering).
    let entry_blk = match osr_entry {
        None    => cl_blocks[0],
        Some(_) => builder.create_block(),
    };
    builder.append_block_params_for_function_params(entry_blk);
    builder.switch_to_block(entry_blk);

    let frame_val = builder.block_params(entry_blk)[0];
    let ctx_val   = builder.block_params(entry_blk)[1];

    // Import all helpers as FuncRef (per-function, valid for this function only).
    macro_rules! imp {
        ($id:expr) => { jit.declare_func_in_func($id, builder.func) }
    }
    let hr_const_i32     = imp!(helper_ids.const_i32);
    let hr_const_i64     = imp!(helper_ids.const_i64);
    let hr_const_f64     = imp!(helper_ids.const_f64);
    let hr_const_bool    = imp!(helper_ids.const_bool);
    let hr_const_char    = imp!(helper_ids.const_char);
    let hr_const_null    = imp!(helper_ids.const_null);
    let hr_const_str     = imp!(helper_ids.const_str);
    let hr_copy          = imp!(helper_ids.copy);
    let hr_add           = imp!(helper_ids.add);
    let hr_sub           = imp!(helper_ids.sub);
    let hr_mul           = imp!(helper_ids.mul);
    let hr_div           = imp!(helper_ids.div);
    let hr_rem           = imp!(helper_ids.rem);
    let hr_eq            = imp!(helper_ids.eq);
    let hr_ne            = imp!(helper_ids.ne);
    let hr_lt            = imp!(helper_ids.lt);
    let hr_le            = imp!(helper_ids.le);
    let hr_gt            = imp!(helper_ids.gt);
    let hr_ge            = imp!(helper_ids.ge);
    let hr_and           = imp!(helper_ids.and);
    let hr_or            = imp!(helper_ids.or);
    let hr_not           = imp!(helper_ids.not);
    let hr_neg           = imp!(helper_ids.neg);
    let hr_bit_and       = imp!(helper_ids.bit_and);
    let hr_bit_or        = imp!(helper_ids.bit_or);
    let hr_bit_xor       = imp!(helper_ids.bit_xor);
    let hr_bit_not       = imp!(helper_ids.bit_not);
    let hr_shl           = imp!(helper_ids.shl);
    let hr_shr           = imp!(helper_ids.shr);
    let hr_str_concat    = imp!(helper_ids.str_concat);
    let hr_to_str        = imp!(helper_ids.to_str);
    let hr_call          = imp!(helper_ids.call);
    let hr_builtin       = imp!(helper_ids.builtin);
    let hr_array_new     = imp!(helper_ids.array_new);
    let hr_array_new_lit = imp!(helper_ids.array_new_lit);
    let hr_array_get     = imp!(helper_ids.array_get);
    let hr_array_data    = imp!(helper_ids.array_data);
    let hr_array_data_opt = imp!(helper_ids.array_data_opt);
    let hr_array_set     = imp!(helper_ids.array_set);
    let hr_array_len     = imp!(helper_ids.array_len);
    let hr_obj_new       = imp!(helper_ids.obj_new);
    let hr_typeof        = imp!(helper_ids.typeof_op);
    let hr_field_get     = imp!(helper_ids.field_get);
    let hr_obj_field_slot = imp!(helper_ids.obj_field_slot);
    let hr_field_set     = imp!(helper_ids.field_set);
    let hr_vcall         = imp!(helper_ids.vcall);
    let hr_is_instance   = imp!(helper_ids.is_instance);
    let hr_as_cast       = imp!(helper_ids.as_cast);
    let hr_static_get    = imp!(helper_ids.static_get);
    let hr_static_set    = imp!(helper_ids.static_set);
    // add-struct-jit-value-path (P5): blob value-type instruction helpers.
    let hr_struct_alloc          = imp!(helper_ids.struct_alloc);
    let hr_struct_copy           = imp!(helper_ids.struct_copy);
    let hr_struct_field_get_prim = imp!(helper_ids.struct_field_get_prim);
    let hr_struct_field_set_prim = imp!(helper_ids.struct_field_set_prim);
    let hr_get_bool      = imp!(helper_ids.get_bool);
    let hr_set_ret       = imp!(helper_ids.set_ret);
    let hr_throw            = imp!(helper_ids.throw);
    let hr_install_catch    = imp!(helper_ids.install_catch);
    let hr_match_catch_type = imp!(helper_ids.match_catch_type);
    let hr_load_fn       = imp!(helper_ids.load_fn);
    let hr_mk_clos       = imp!(helper_ids.mk_clos);
    let hr_call_indirect = imp!(helper_ids.call_indirect);
    let hr_load_fn_cached = imp!(helper_ids.load_fn_cached);
    let hr_default_of     = imp!(helper_ids.default_of);
    let hr_convert        = imp!(helper_ids.convert);
    // add-gc-safepoint-jit (2026-05-21): cooperative GC safepoint trampoline.
    // inline-jit-safepoint-check (2026-08-01): the fast-path decrement is now
    // emitted inline (`emit_safepoint_check`); only the rare slow branch
    // (counter hit 0) calls a helper. `hr_check_safepoint` is retained for
    // reference / tests but no longer emitted on the hot path.
    let hr_check_safepoint_slow = imp!(helper_ids.check_safepoint_slow);
    let hr_regs_ptr        = imp!(helper_ids.regs_ptr);

    // add-gc-safepoint-jit (2026-05-21): function-entry safepoint check.
    // A spawned worker that enters JIT-compiled code immediately after
    // spawn must respect a pending GC pause before touching any roots.
    emit_safepoint_check(&mut builder, ptr, ctx_val, frame_val, hr_check_safepoint_slow);

    // review.md C2 P1 step 1 (2026-05-28): cache `frame.regs.as_mut_ptr()`
    // for typed-arithmetic fast paths. One helper call per function (not per
    // op) yields raw `*mut Value` we use to compute slot addresses inline.
    // Pre-conditions: `JitFrame::new` pre-allocates regs with stable
    // capacity → the data pointer never moves for the function's lifetime.
    let regs_base = {
        let inst = builder.ins().call(hr_regs_ptr, &[frame_val]);
        builder.inst_results(inst)[0]
    };

    // ── jit-unbox-regalloc Phase 2C: loop-carried integer residency ──────────
    // Promote integer regs whose *every* access is routed (const-int / native
    // int arith·cmp·convert / Ret) to Cranelift `Variable`s. Cranelift's
    // use_var/def_var + seal_all_blocks build the SSA — including loop-header
    // phis — so these values stay resident in machine registers across blocks
    // AND loop back-edges (2C), with no manual block-param threading. Promoted
    // regs live entirely in Variables; the whitelist guarantees no memory-backed
    // op ever touches them, so they never interact with the 2B cache or a helper
    // (the sole memory sync points are: seed here, and spill at `Ret`). Disabled
    // for OSR variants (v1) — their mid-function entry would need Variable
    // reload at the OSR block. Seed each Variable from its `frame.regs` slot in
    // the entry block (dominates all uses); a local's dead seed (garbage/Null
    // payload) is overwritten by its first real def before any use.
    // OSR variants are supported: their dedicated entry block runs the same
    // prologue (incl. the Variable seed loads from `frame.regs`, which for OSR
    // holds interp's copied-in state), then `jump cl_blocks[k]`. Cranelift's
    // SSA construction appends the seeded value as the loop-header block-param
    // arg on that jump automatically — so the loop-header phi merges
    // (OSR-entry seed, back-edge value) correctly. Verified byte-identical
    // under `Z42_OSR_THRESHOLD=1` (forces every loop through OSR).
    let promoted = compute_promotable_regs(z42_func, true);
    let any_promoted = promoted.iter().any(|&p| p);
    if std::env::var_os("Z42_JIT_DEBUG_PROMOTE").is_some() {
        let cnt = promoted.iter().filter(|&&p| p).count();
        eprintln!("[2C] {} promoted {}/{} int regs (osr={})",
            z42_func.name, cnt, promoted.len(), osr_entry.is_some());
    }
    if any_promoted {
        for (reg, &p) in promoted.iter().enumerate() {
            if p {
                let var = Variable::from_u32(reg as u32);
                let addr = reg_addr(&mut builder, regs_base, reg as u32);
                // F64 regs get an F64-typed Variable seeded with the f64 payload;
                // integer regs (I8..U64, all physically Value::I64) get an I64
                // Variable seeded with the i64 payload. A local's dead seed
                // (garbage / Null) is overwritten by its first real def before use.
                if z42_func.reg_types.get(reg).copied() == Some(IrType::F64) {
                    builder.declare_var(var, types::F64);
                    let seed = load_payload(&mut builder, addr, types::F64);
                    builder.def_var(var, seed);
                } else {
                    builder.declare_var(var, types::I64);
                    let seed = load_payload_i64(&mut builder, addr);
                    builder.def_var(var, seed);
                }
            }
        }
    }

    // C2 P1 fast-path layout constants live inside `emit_i64_binop` (the sole
    // consumer today); when comparison + logical ops are specialized in the
    // next chunk they'll move to module scope.

    // ── jit-inline-fastpaths 方案 B: hoist loop-invariant array data ptr/len ──
    // For array registers that are (a) indexed by an i64-inline-eligible
    // `ArrayGet` and (b) NEVER written (never a `dst`) anywhere in the function,
    // the array Value in `regs[arr]` is stable for the whole activation, so its
    // element-buffer pointer + length are loop-invariant. Fetch them ONCE here in
    // the entry block (which dominates every block ⇒ the SSA values are usable
    // everywhere) via the NON-throwing `jit_array_data_opt`. The per-`ArrayGet`
    // inline then does a pure native bounds-check + load with ZERO per-iteration
    // helper call — approaching the fully-native ceiling. Null/invalid arrays
    // yield `ptr=null,len=0`, so the unsigned bounds check routes every access to
    // the `jit_array_get` cold path (correct exception at the real access site;
    // no spurious throw when the loop runs 0 times).
    // Registers written anywhere (never-reassigned check for both hoists below).
    let written: std::collections::HashSet<u32> = {
        let mut w = std::collections::HashSet::new();
        for b in &z42_func.blocks {
            for ins in &b.instructions {
                if let Some(d) = written_reg(ins) { w.insert(d); }
            }
        }
        w
    };
    // hoisted (ptr, len, width): width is the runtime packed slot width the
    // ArraySet inline consults (jit-inline-i32-arrays). ArrayGet ignores it.
    let hoisted_arrays: std::collections::HashMap<u32, (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value, cranelift_codegen::ir::Value)> = {
        let mut candidates: Vec<u32> = Vec::new();
        let mut consider = |arr: &u32, ok: bool, candidates: &mut Vec<u32>| {
            if ok && !written.contains(arr) && !candidates.contains(arr) {
                candidates.push(*arr);
            }
        };
        for b in &z42_func.blocks {
            for ins in &b.instructions {
                match ins {
                    Instruction::ArrayGet { dst, arr, idx } => consider(arr,
                        arr_prim_elem(z42_func, *dst).is_some() && idx_int_ok(z42_func, *idx),
                        &mut candidates),
                    // i32/i64/f64 ArraySet also reads the loop-invariant data ptr/len/width.
                    Instruction::ArraySet { arr, idx, val } => consider(arr,
                        arr_prim_elem(z42_func, *val).is_some() && idx_int_ok(z42_func, *idx),
                        &mut candidates),
                    _ => {}
                }
            }
        }
        candidates.sort_unstable(); // deterministic codegen order
        let mut map = std::collections::HashMap::new();
        for arr in candidates {
            use cranelift_codegen::ir::{StackSlotData, StackSlotKind};
            let ss_ptr = builder.create_sized_stack_slot(
                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            let ss_len = builder.create_sized_stack_slot(
                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            let ss_width = builder.create_sized_stack_slot(
                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            let ptr_addr = builder.ins().stack_addr(ptr, ss_ptr, 0);
            let len_addr = builder.ins().stack_addr(ptr, ss_len, 0);
            let width_addr = builder.ins().stack_addr(ptr, ss_width, 0);
            let a_c = builder.ins().iconst(types::I32, arr as i64);
            builder.ins().call(hr_array_data_opt, &[frame_val, ctx_val, a_c, ptr_addr, len_addr, width_addr]);
            let dptr = builder.ins().stack_load(ptr, ss_ptr, 0);
            let dlen = builder.ins().stack_load(types::I64, ss_len, 0);
            let dwidth = builder.ins().stack_load(types::I64, ss_width, 0);
            map.insert(arr, (dptr, dlen, dwidth));
        }
        map
    };

    // ── FieldGet/Set P5-B: hoist (bytes_ptr, byte_offset) for never-reassigned objects ──
    // For an object register never written (e.g. `this`) accessed via `FieldGet`/
    // `FieldSet` on an inline-primitive field, resolve (bytes_ptr, offset) ONCE in the
    // entry block via the non-throwing byte-aware `jit_obj_field_slot`. Keyed by
    // (obj_reg, field_name); the expected (width, tag) come from the field's static
    // type. The per-access inline then does a native width-aware byte load/store at
    // `bytes_ptr + offset`; `offset < 0` (null / non-object / field-not-found /
    // reference / struct root / string / layout mismatch) routes to the helper.
    let hoisted_fields: std::collections::HashMap<(u32, String), (cranelift_codegen::ir::Value, cranelift_codegen::ir::Value)> = {
        let mut cands: Vec<(u32, &str, u32, u8)> = Vec::new();
        for b in &z42_func.blocks {
            for ins in &b.instructions {
                // FieldGet (dst) / FieldSet (val) on a never-reassigned object read the
                // loop-invariant bytes ptr + offset. width/tag come from the field type.
                let hit = match ins {
                    Instruction::FieldGet(insn) => field_prim_kind(z42_func, insn.dst)
                        .map(|k| (insn.obj, insn.field_name.as_str(), k.width, k.field_tag)),
                    Instruction::FieldSet(insn) => field_prim_kind(z42_func, insn.val)
                        .map(|k| (insn.obj, insn.field_name.as_str(), k.width, k.field_tag)),
                    _ => None,
                };
                if let Some((obj, fname, w, tag)) = hit {
                    if !written.contains(&obj) && !cands.iter().any(|(o, f, _, _)| *o == obj && *f == fname) {
                        cands.push((obj, fname, w, tag));
                    }
                }
            }
        }
        cands.sort_unstable();
        let mut map = std::collections::HashMap::new();
        for (obj, fname, exp_w, exp_tag) in cands {
            use cranelift_codegen::ir::{StackSlotData, StackSlotKind};
            let ss_ptr = builder.create_sized_stack_slot(
                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            let ss_off = builder.create_sized_stack_slot(
                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
            let ptr_addr = builder.ins().stack_addr(ptr, ss_ptr, 0);
            let off_addr = builder.ins().stack_addr(ptr, ss_off, 0);
            let o_c = builder.ins().iconst(types::I32, obj as i64);
            let fp = builder.ins().iconst(ptr, fname.as_ptr() as i64);
            let fl = builder.ins().iconst(types::I64, fname.len() as i64);
            let w_c = builder.ins().iconst(types::I32, exp_w as i64);
            let tag_c = builder.ins().iconst(types::I32, exp_tag as i64);
            builder.ins().call(hr_obj_field_slot,
                &[frame_val, ctx_val, o_c, fp, fl, w_c, tag_c, ptr_addr, off_addr]);
            let bptr = builder.ins().stack_load(ptr, ss_ptr, 0);
            let off = builder.ins().stack_load(types::I64, ss_off, 0);
            map.insert((obj, fname.to_string()), (bptr, off));
        }
        map
    };

    // add-osr-loop-tiering: OSR variant — after the prologue (which cached
    // `regs_base` etc. into SSA values that now dominate every block), jump into the
    // hot loop header. Blocks `0..K` become unreachable and cranelift DCEs them at
    // `seal_all_blocks`. The interpreter has already run them and left their results
    // in `frame.regs`, which the native code reads back through `regs_base`.
    if let Some(k) = osr_entry {
        builder.ins().jump(cl_blocks[k], &[]);
    }

    // ── Translate each z42 block ──────────────────────────────────────────────
    for (block_idx, z42_block) in z42_func.blocks.iter().enumerate() {
        // Non-OSR: block 0 is the entry block (already switched to, prologue emitted
        // there) so we don't re-switch. OSR: the entry is a dedicated block, so we
        // must switch to cl_blocks[0] like any other body block.
        if block_idx != 0 || osr_entry.is_some() {
            builder.switch_to_block(cl_blocks[block_idx]);
        }

        // catch-by-generic-type (2026-05-06): collect every enclosing exception-
        // handler entry, in source order. Each tuple is
        //   (catch_cl, catch_reg, catch_type)
        // where `catch_type` is None for wildcard / synthetic-finally fallthrough
        // (matches any thrown value) and Some(t) for a typed catch (only matches
        // when the thrown value's class is `t` or a subclass).
        //
        // The legacy single-entry shortcut is preserved as `catch_info` for the
        // wildcard-only case so the unconditional jump path stays identical to
        // pre-fix behaviour. Typed / multi-catch goes through `catch_chain`.
        let catch_chain: Vec<(cranelift_codegen::ir::Block, u32, Option<&str>)> =
            find_handler_entries(z42_func, block_idx).into_iter().map(|ei| {
                let entry      = &z42_func.exception_table()[ei];
                let catch_idx  = z42_func.blocks.iter().position(|b| b.label == entry.catch_label)
                    .expect("catch_label block must exist");
                let ty: Option<&str> = match entry.catch_type.as_deref() {
                    None | Some("*") => None,
                    Some(t)          => Some(t),
                };
                (cl_blocks[catch_idx], entry.catch_reg, ty)
            }).collect();
        // Wildcard shortcut: if there is exactly one covering entry and it is
        // untyped, the JIT can skip the type-probe chain entirely (cheap path
        // for the existing 7 untyped-catch goldens).
        let catch_info: Option<(cranelift_codegen::ir::Block, u32)> =
            if catch_chain.len() == 1 && catch_chain[0].2.is_none() {
                Some((catch_chain[0].0, catch_chain[0].1))
            } else {
                None
            };

        // ── Inline helpers used in match arms ────────────────────────────────

        // Emit an i32 constant for a register index.
        macro_rules! ri {
            ($r:expr) => { builder.ins().iconst(types::I32, $r as i64) }
        }

        // Embed a &str as (ptr: pointer_type, len: i64) Cranelift constants.
        // We use a global_value backed by a static string slice.
        // Since we can't easily add global data from here, we pass the string
        // pointer as an i64 address constant (valid for the duration of execution).
        macro_rules! str_val {
            ($s:expr) => {{
                // SAFETY: the string literal is 'static (from the bytecode module
                // which lives for the whole JitModule lifetime).
                let bytes: &'static [u8] = unsafe {
                    std::slice::from_raw_parts(
                        $s.as_ptr(),
                        $s.len(),
                    )
                };
                let sptr = builder.ins().iconst(ptr, bytes.as_ptr() as i64);
                let slen = builder.ins().iconst(types::I64, bytes.len() as i64);
                (sptr, slen)
            }};
        }

        // Pack a &[u32] of register indices into a static-lifetime pointer+len.
        macro_rules! regs_val {
            ($regs:expr) => {{
                let slice: &'static [u32] = unsafe {
                    std::slice::from_raw_parts(
                        $regs.as_ptr(),
                        $regs.len(),
                    )
                };
                let rptr = builder.ins().iconst(ptr, slice.as_ptr() as i64);
                let rlen = builder.ins().iconst(types::I64, slice.len() as i64);
                (rptr, rlen)
            }};
        }

        // After a helper call that returns u8: branch to catch dispatch or
        // return 1 on error. Blocks are NOT sealed here; seal_all_blocks() is
        // called once after all control-flow edges are established (handles
        // back-edges in loops correctly).
        //
        // catch-by-generic-type (2026-05-06): when the enclosing scope has any
        // typed catches (or multiple covering entries), the exception path
        // probes each entry's catch_type via `jit_match_catch_type` and jumps
        // to the first match; falls through to return-1 if none match. The
        // wildcard fast-path (single covering untyped entry → unconditional
        // jump) is preserved via the `catch_info` shortcut on the cold side.
        macro_rules! emit_dispatch_to_catch_or_return {
            () => {{
                if let Some((catch_cl, catch_reg)) = catch_info {
                    let creg = ri!(catch_reg);
                    builder.ins().call(hr_install_catch, &[frame_val, ctx_val, creg]);
                    builder.ins().jump(catch_cl, &[]);
                } else if !catch_chain.is_empty() {
                    // Typed / multi-catch chain: probe each entry's catch_type;
                    // first instance-of match wins. The `closed_by_wildcard`
                    // flag tracks whether a wildcard entry already terminated
                    // the current Cranelift block via `jump` — once that happens
                    // the block is "filled" and the trailing return-1 fallthrough
                    // would be illegal (panic in Cranelift's frontend).
                    let mut closed_by_wildcard = false;
                    for (catch_cl, catch_reg, ty) in catch_chain.iter() {
                        match ty {
                            None => {
                                // Wildcard / synthetic-finally fallthrough — always match.
                                let creg = ri!(*catch_reg);
                                builder.ins().call(hr_install_catch, &[frame_val, ctx_val, creg]);
                                builder.ins().jump(*catch_cl, &[]);
                                closed_by_wildcard = true;
                                break;
                            }
                            Some(t) => {
                                let (tptr, tlen) = str_val!(t);
                                let inst = builder.ins().call(hr_match_catch_type, &[frame_val, ctx_val, tptr, tlen]);
                                let m = builder.inst_results(inst)[0];
                                let take_blk = builder.create_block();
                                let next_blk = builder.create_block();
                                builder.ins().brif(m, take_blk, &[], next_blk, &[]);
                                builder.switch_to_block(take_blk);
                                let creg = ri!(*catch_reg);
                                builder.ins().call(hr_install_catch, &[frame_val, ctx_val, creg]);
                                builder.ins().jump(*catch_cl, &[]);
                                builder.switch_to_block(next_blk);
                            }
                        }
                    }
                    if !closed_by_wildcard {
                        // All entries were typed and none matched — propagate.
                        let one = builder.ins().iconst(types::I8, 1);
                        builder.ins().return_(&[one]);
                    }
                } else {
                    let one = builder.ins().iconst(types::I8, 1);
                    builder.ins().return_(&[one]);
                }
            }};
        }
        macro_rules! check {
            ($ret:expr) => {{
                let ok_blk  = builder.create_block();
                let exc_blk = builder.create_block();
                builder.ins().brif($ret, exc_blk, &[], ok_blk, &[]);
                builder.switch_to_block(exc_blk);
                emit_dispatch_to_catch_or_return!();
                builder.switch_to_block(ok_blk);
            }};
        }

        // ── Instruction translation ───────────────────────────────────────────
        //
        // jit-unbox-regalloc Phase 2B: a fresh block-local integer-scalar cache.
        // Created empty per z42 block (memory is authoritative at block entry —
        // every predecessor flushed at its terminator). Cache-participating
        // integer ops read/write it; every other instruction, every Category-B
        // helper, and the terminator flush it first (see `instr_uses_int_cache`
        // + the flush points below). Cached SSA values therefore never cross a
        // Cranelift block boundary and never go stale.
        let mut cache = RegCache::new();
        for (instr_idx, instr) in z42_block.instructions.iter().enumerate() {
            // Coherence gate: any instruction that is NOT a cache-participating
            // integer op may read/write `frame.regs` directly (or call a helper
            // that does), so make memory authoritative first.
            if !instr_uses_int_cache(z42_func, instr) {
                cache.flush(&mut builder, regs_base);
            }
            match instr {
                // C2 P1 step 5 (2026-05-28): ConstI32/I64/F64/Bool/Char/Null
                // inline directly when dst is typed-compatible — no helper
                // call. ConstStr still goes through the helper because it
                // needs ctx.string_pool lookup + bounds check + Arc::clone.
                //
                // Safety: previous slot value at `dst` is statically known
                // by reg_types to be the matching primitive type (or Null
                // for first-write), so raw bit-copy is sound. If reg_types
                // is `Unknown` (legacy zbc / pre-REGT path), we fall back
                // to the helper which handles arbitrary old values via Drop.
                Instruction::ConstI32 { dst, val } => {
                    if promoted.get(*dst as usize).copied().unwrap_or(false) {
                        // 2C: define the resident Variable, no memory store.
                        let v = builder.ins().iconst(types::I64, *val as i64);
                        builder.def_var(Variable::from_u32(*dst), v);
                    } else if is_typed(z42_func, *dst, IrType::I64) {
                        emit_const_i64(&mut builder, regs_base, *dst, *val as i64);
                    } else {
                        let d = ri!(*dst); let v = builder.ins().iconst(types::I32, *val as i64);
                        builder.ins().call(hr_const_i32, &[frame_val, ctx_val, d, v]);
                    }
                }
                Instruction::ConstI64 { dst, val } => {
                    if promoted.get(*dst as usize).copied().unwrap_or(false) {
                        let v = builder.ins().iconst(types::I64, *val);
                        builder.def_var(Variable::from_u32(*dst), v);
                    } else if is_typed(z42_func, *dst, IrType::I64) {
                        emit_const_i64(&mut builder, regs_base, *dst, *val);
                    } else {
                        let d = ri!(*dst); let v = builder.ins().iconst(types::I64, *val);
                        builder.ins().call(hr_const_i64, &[frame_val, ctx_val, d, v]);
                    }
                }
                Instruction::ConstF64 { dst, val } => {
                    if promoted.get(*dst as usize).copied().unwrap_or(false) {
                        // 2C (F64 residency): define the resident Variable, no memory store.
                        let v = builder.ins().f64const(*val);
                        builder.def_var(Variable::from_u32(*dst), v);
                    } else if is_typed(z42_func, *dst, IrType::F64) {
                        emit_const_f64(&mut builder, regs_base, *dst, *val);
                    } else {
                        let d = ri!(*dst); let v = builder.ins().f64const(*val);
                        builder.ins().call(hr_const_f64, &[frame_val, ctx_val, d, v]);
                    }
                }
                Instruction::ConstBool { dst, val } => {
                    if is_typed(z42_func, *dst, IrType::Bool) {
                        emit_const_bool(&mut builder, regs_base, *dst, *val);
                    } else {
                        let d = ri!(*dst); let v = builder.ins().iconst(types::I8, if *val { 1 } else { 0 });
                        builder.ins().call(hr_const_bool, &[frame_val, ctx_val, d, v]);
                    }
                }
                Instruction::ConstChar { dst, val } => {
                    if is_typed(z42_func, *dst, IrType::Char) {
                        emit_const_char(&mut builder, regs_base, *dst, *val);
                    } else {
                        let d = ri!(*dst); let v = builder.ins().iconst(types::I32, *val as i32 as i64);
                        builder.ins().call(hr_const_char, &[frame_val, ctx_val, d, v]);
                    }
                }
                Instruction::ConstNull { dst } => {
                    if is_drop_free_primitive(z42_func, *dst) {
                        emit_const_null(&mut builder, regs_base, *dst);
                    } else {
                        let d = ri!(*dst);
                        builder.ins().call(hr_const_null, &[frame_val, ctx_val, d]);
                    }
                }
                Instruction::ConstStr { dst, idx } => {
                    let d = ri!(*dst); let i = ri!(*idx);
                    let inst = builder.ins().call(hr_const_str, &[frame_val, ctx_val, d, i]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                }
                Instruction::Copy { dst, src } => {
                    // review.md C2 P1 follow-up (2026-05-30): inline when src
                    // and dst are both drop-free primitives (I64 / F64 / Bool
                    // / Char). 16 B Value = 1 B tag at offset 0 + 8 B payload
                    // at offset 8. Heap-ref payload requires Arc::clone so
                    // those keep the helper.
                    if is_drop_free_primitive(z42_func, *dst)
                        && is_drop_free_primitive(z42_func, *src)
                    {
                        emit_primitive_copy(&mut builder, regs_base, *dst, *src);
                    } else {
                        let d = ri!(*dst); let s = ri!(*src);
                        builder.ins().call(hr_copy, &[frame_val, ctx_val, d, s]);
                    }
                }

                // Arithmetic — review.md C2 P1 (2026-05-28); widened by
                // jit-unbox-regalloc Phase 2A (2026-08-15): when reg_types
                // confirm all three operands are integer types (I8..U64, all
                // stored as Value::I64), emit native Cranelift
                // iadd/isub/imul via raw load/store on frame.regs; skip the
                // extern "C" helper call entirely. Otherwise fall back to the
                // type-dispatching helper (handles Str concat, F64, mixed
                // types, etc.).
                //
                // Safety of raw store: when reg_types[dst] is an integer type,
                // every write to that register slot is Value::I64 (initial
                // Null also has no Drop), so raw bit-copy without Drop is
                // sound. Div/Rem on i64 panic on /0 — keep helper for those
                // (zero-check + exception propagation lives there). Add/Sub/Mul
                // are wrapping (`vm-wrapping-int-arith`, 2026-04-28) matching
                // Cranelift defaults, at i64 width for all integer types.
                Instruction::Add { dst, a, b } => {
                    if is_int_typed(z42_func, *dst, *a, *b) {
                        emit_i64_binop(&mut builder, regs_base, &mut cache, &promoted, *dst, *a, *b, BinopKind::Add);
                    } else if is_f64_typed(z42_func, *dst, *a, *b) {
                        emit_f64_binop(&mut builder, regs_base, &promoted, *dst, *a, *b, F64BinopKind::Add);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_add, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::Sub { dst, a, b } => {
                    if is_int_typed(z42_func, *dst, *a, *b) {
                        emit_i64_binop(&mut builder, regs_base, &mut cache, &promoted, *dst, *a, *b, BinopKind::Sub);
                    } else if is_f64_typed(z42_func, *dst, *a, *b) {
                        emit_f64_binop(&mut builder, regs_base, &promoted, *dst, *a, *b, F64BinopKind::Sub);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_sub, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::Mul { dst, a, b } => {
                    if is_int_typed(z42_func, *dst, *a, *b) {
                        emit_i64_binop(&mut builder, regs_base, &mut cache, &promoted, *dst, *a, *b, BinopKind::Mul);
                    } else if is_f64_typed(z42_func, *dst, *a, *b) {
                        emit_f64_binop(&mut builder, regs_base, &promoted, *dst, *a, *b, F64BinopKind::Mul);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_mul, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::Div { dst, a, b } => {
                    // jit-native-float: F64 divide is native `fdiv` — IEEE /0 →
                    // ±inf/NaN (no trap, no exception), matching interp. i64
                    // div-by-zero must surface a catchable z42 exception (native
                    // sdiv on x86_64 traps SIGFPE) → keep the helper for ints.
                    if is_f64_typed(z42_func, *dst, *a, *b) {
                        emit_f64_binop(&mut builder, regs_base, &promoted, *dst, *a, *b, F64BinopKind::Div);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_div, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::Rem { dst, a, b } => {
                    // Same as Div — keep helper for /0 exception handling.
                    let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                    let inst = builder.ins().call(hr_rem, &[frame_val, ctx_val, d, av, bv]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                }

                // Comparison — C2 P1; Phase 2A widened to all integer types:
                // integer-typed operands (I8..U64) emit Cranelift `icmp <pred>`
                // directly (signed, matching the VM's uniform signed compare);
                // Bool result stored back inline.
                Instruction::Eq { dst, a, b } => {
                    if is_int_cmp(z42_func, *a, *b) {
                        emit_i64_cmp(&mut builder, regs_base, &mut cache, &promoted, *dst, *a, *b, CmpKind::Eq);
                    } else if is_f64_cmp(z42_func, *a, *b) {
                        emit_f64_cmp(&mut builder, regs_base, &promoted, *dst, *a, *b, CmpKind::Eq);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        builder.ins().call(hr_eq, &[frame_val, ctx_val, d, av, bv]);
                    }
                }
                Instruction::Ne { dst, a, b } => {
                    if is_int_cmp(z42_func, *a, *b) {
                        emit_i64_cmp(&mut builder, regs_base, &mut cache, &promoted, *dst, *a, *b, CmpKind::Ne);
                    } else if is_f64_cmp(z42_func, *a, *b) {
                        emit_f64_cmp(&mut builder, regs_base, &promoted, *dst, *a, *b, CmpKind::Ne);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        builder.ins().call(hr_ne, &[frame_val, ctx_val, d, av, bv]);
                    }
                }
                Instruction::Lt { dst, a, b } => {
                    if is_int_cmp(z42_func, *a, *b) {
                        emit_i64_cmp(&mut builder, regs_base, &mut cache, &promoted, *dst, *a, *b, CmpKind::Lt);
                    } else if is_f64_cmp(z42_func, *a, *b) {
                        emit_f64_cmp(&mut builder, regs_base, &promoted, *dst, *a, *b, CmpKind::Lt);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_lt, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::Le { dst, a, b } => {
                    if is_int_cmp(z42_func, *a, *b) {
                        emit_i64_cmp(&mut builder, regs_base, &mut cache, &promoted, *dst, *a, *b, CmpKind::Le);
                    } else if is_f64_cmp(z42_func, *a, *b) {
                        emit_f64_cmp(&mut builder, regs_base, &promoted, *dst, *a, *b, CmpKind::Le);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_le, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::Gt { dst, a, b } => {
                    if is_int_cmp(z42_func, *a, *b) {
                        emit_i64_cmp(&mut builder, regs_base, &mut cache, &promoted, *dst, *a, *b, CmpKind::Gt);
                    } else if is_f64_cmp(z42_func, *a, *b) {
                        emit_f64_cmp(&mut builder, regs_base, &promoted, *dst, *a, *b, CmpKind::Gt);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_gt, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::Ge { dst, a, b } => {
                    if is_int_cmp(z42_func, *a, *b) {
                        emit_i64_cmp(&mut builder, regs_base, &mut cache, &promoted, *dst, *a, *b, CmpKind::Ge);
                    } else if is_f64_cmp(z42_func, *a, *b) {
                        emit_f64_cmp(&mut builder, regs_base, &promoted, *dst, *a, *b, CmpKind::Ge);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_ge, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }

                // Logical — C2 P1: Bool-typed operands emit Cranelift
                // `band`/`bor`/`bnot` directly on the i8 payload.
                Instruction::And { dst, a, b } => {
                    if is_bool_typed(z42_func, *dst, *a, *b) {
                        emit_bool_binop(&mut builder, regs_base, *dst, *a, *b, BoolBinopKind::And);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_and, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::Or { dst, a, b } => {
                    if is_bool_typed(z42_func, *dst, *a, *b) {
                        emit_bool_binop(&mut builder, regs_base, *dst, *a, *b, BoolBinopKind::Or);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_or, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::Not { dst, src } => {
                    if is_bool_typed_unary(z42_func, *dst, *src) {
                        emit_bool_not(&mut builder, regs_base, *dst, *src);
                    } else {
                        let d = ri!(*dst); let s = ri!(*src);
                        let inst = builder.ins().call(hr_not, &[frame_val, ctx_val, d, s]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }

                // Unary arithmetic — review.md C2 P1 follow-up (2026-05-30):
                // I64-typed Neg emits native Cranelift `ineg` (wrapping,
                // matches helper's `Value::I64(-n)`).
                Instruction::Neg { dst, src } => {
                    if is_int_typed_unary(z42_func, *dst, *src) {
                        emit_i64_neg(&mut builder, regs_base, &mut cache, &promoted, *dst, *src);
                    } else if is_f64_typed_unary(z42_func, *dst, *src) {
                        emit_f64_neg(&mut builder, regs_base, &promoted, *dst, *src);
                    } else {
                        let d = ri!(*dst); let s = ri!(*src);
                        let inst = builder.ins().call(hr_neg, &[frame_val, ctx_val, d, s]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }

                // Bitwise — review.md C2 P1 follow-up (2026-05-30); Phase 2A
                // widened to all integer types: inline native Cranelift
                // band/bor/bxor/bnot/ishl/sshr when reg_types confirm integer
                // operands (I8..U64). Same payload load/store layout as arith;
                // shift amount masked to low 6 bits; `sshr` (arithmetic) matches
                // the VM's uniform signed `>>` on all integer types.
                Instruction::BitAnd { dst, a, b } => {
                    if is_int_typed(z42_func, *dst, *a, *b) {
                        emit_i64_binop(&mut builder, regs_base, &mut cache, &promoted, *dst, *a, *b, BinopKind::BitAnd);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_bit_and, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::BitOr { dst, a, b } => {
                    if is_int_typed(z42_func, *dst, *a, *b) {
                        emit_i64_binop(&mut builder, regs_base, &mut cache, &promoted, *dst, *a, *b, BinopKind::BitOr);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_bit_or, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::BitXor { dst, a, b } => {
                    if is_int_typed(z42_func, *dst, *a, *b) {
                        emit_i64_binop(&mut builder, regs_base, &mut cache, &promoted, *dst, *a, *b, BinopKind::BitXor);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_bit_xor, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::BitNot { dst, src } => {
                    if is_int_typed_unary(z42_func, *dst, *src) {
                        emit_i64_bit_not(&mut builder, regs_base, &mut cache, &promoted, *dst, *src);
                    } else {
                        let d = ri!(*dst); let s = ri!(*src);
                        let inst = builder.ins().call(hr_bit_not, &[frame_val, ctx_val, d, s]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::Shl { dst, a, b } => {
                    if is_int_typed(z42_func, *dst, *a, *b) {
                        emit_i64_binop(&mut builder, regs_base, &mut cache, &promoted, *dst, *a, *b, BinopKind::Shl);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_shl, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::Shr { dst, a, b } => {
                    if is_int_typed(z42_func, *dst, *a, *b) {
                        emit_i64_binop(&mut builder, regs_base, &mut cache, &promoted, *dst, *a, *b, BinopKind::Shr);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_shr, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }

                // String
                Instruction::StrConcat { dst, a, b } => {
                    let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                    let inst = builder.ins().call(hr_str_concat, &[frame_val, ctx_val, d, av, bv]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                }
                Instruction::ToStr { dst, src } => {
                    let d = ri!(*dst); let s = ri!(*src);
                    let inst = builder.ins().call(hr_to_str, &[frame_val, ctx_val, d, s]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                }

                // Calls
                // formalize-jit-method-token Phase 2.C (2026-05-08): emit
                // pre-resolved MethodId + name (fallback for cross-zpkg).
                // Helper checks id first; UNRESOLVED → uses name HashMap.
                Instruction::Call(insn) => {
                    let CallInsn { dst, func: fname, args } = &**insn;
                    let d = ri!(*dst);
                    let (np, nl) = str_val!(fname);
                    let (ap, al) = regs_val!(args);
                    let mid = method_id_at(z42_func, block_idx, instr_idx);
                    let mid_val = builder.ins().iconst(types::I32, mid as i64);
                    // make-vm-loading-lazy: per-site IC caching the resolved
                    // lazy/merged fn id, so a cross-zpkg call resolves the name
                    // once then hits the lock-free by-id fast path thereafter.
                    let ic_ptr = call_jit_ic_ptr_at(z42_func, block_idx, instr_idx);
                    let ic_val = builder.ins().iconst(ptr, ic_ptr as i64);
                    // 2026-05-10 jit-stack-trace + span-column-propagate: pass
                    // current source (line, col) so jit_call can stamp the
                    // caller's frame info before descending into the callee.
                    let (line, col) = crate::interp::resolve_line(z42_func.line_table(), block_idx as u32, instr_idx as u32);
                    let line_val = builder.ins().iconst(types::I32, line as i64);
                    let col_val  = builder.ins().iconst(types::I32, col as i64);
                    // add-offline-symbolication: bake linearized code offset (caller frame).
                    let off_val = builder.ins().iconst(types::I32, z42_func.linear_offset(block_idx as u32, instr_idx as u32) as i64);
                    let inst = builder.ins().call(hr_call, &[frame_val, ctx_val, d, mid_val, np, nl, ap, al, ic_val, line_val, col_val, off_val]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                    // add-gc-safepoint-jit (2026-05-21): post-Call safepoint
                    // — long callees may yield to a GC request that arrived
                    // partway through; the caller catches it on return.
                    emit_safepoint_check(&mut builder, ptr, ctx_val, frame_val, hr_check_safepoint_slow);
                }
                Instruction::Builtin(insn) => {
                    let BuiltinInsn { dst, name, args } = &**insn;
                    // formalize-jit-method-token Phase 2 (2026-05-08): emit
                    // pre-resolved BuiltinId as i32 const, drop name pointers.
                    // Resolver populates Function.resolved.builtin_tokens at
                    // load (closed set, never UNRESOLVED at this point).
                    let d = ri!(*dst);
                    let (ap, al) = regs_val!(args);
                    let builtin_id = z42_func.resolved.get()
                        .and_then(|r| {
                            let site = *r.site_index.get(block_idx)?.get(instr_idx)?;
                            r.builtin_tokens.get(site as usize).copied()
                        })
                        .unwrap_or_else(|| {
                            // Fallback: resolver hadn't run (shouldn't happen
                            // in production via Vm::run, but guards against
                            // direct compile_module callers in tests).
                            crate::corelib::builtin_id_of(name)
                                .unwrap_or_else(|| panic!("unknown builtin `{}`", name))
                                .0
                        });
                    let bid = builder.ins().iconst(types::I32, builtin_id as i64);
                    let inst = builder.ins().call(hr_builtin, &[frame_val, ctx_val, d, bid, ap, al]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                }

                // Arrays
                Instruction::ArrayNew(insn) => {
                    let d = ri!(insn.dst); let s = ri!(insn.size);
                    let t = builder.ins().iconst(types::I8, insn.elem_tag as i64);
                    let (etp, etl) = str_val!(insn.element_type);   // add-reflection-array-element-type
                    let inst = builder.ins().call(hr_array_new, &[frame_val, ctx_val, d, s, t, etp, etl]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                }
                Instruction::ArrayNewLit(insn) => {
                    let d = ri!(insn.dst);
                    let (ep, el) = regs_val!(&insn.elems);
                    let (etp, etl) = str_val!(insn.element_type);
                    let inst = builder.ins().call(hr_array_new_lit, &[frame_val, ctx_val, d, ep, el, etp, etl]);
                    // add-struct-jit-value-path (P5): now u8 (struct-literal pack / OOM can throw).
                    let ret = builder.inst_results(inst)[0]; check!(ret);
                }
                Instruction::ArrayGet { dst, arr, idx } => {
                    // jit-inline-fastpaths: when the element (`dst`) and index are
                    // statically i64, do a NATIVE bounds-check + element load +
                    // unboxed store — no per-element `Value` round-trip through the
                    // `jit_array_get` helper. The array data ptr+len come either
                    // from the loop-invariant HOIST (方案 B: never-reassigned array
                    // ⇒ zero per-iteration call, approaching the native ceiling) or
                    // a per-get `jit_array_data` (方案 A). Cold OOB path reuses
                    // `jit_array_get` so the exception is identical; for a hoisted
                    // null/invalid array `len==0` routes every access there too.
                    if let (Some((val_tag, arr_width)), true) =
                        (arr_prim_elem(z42_func, *dst), idx_int_ok(z42_func, *idx))
                    {
                        // jit-inline-i32-arrays: `dst`'s IR type reliably equals the
                        // array element type, so `arr_width` (4=int / 8=long·double)
                        // is a compile-time constant here — no runtime-width branch.
                        use cranelift_codegen::ir::condcodes::IntCC;
                        use cranelift_codegen::ir::{StackSlotData, StackSlotKind};
                        let (data_ptr, len, width) = if let Some(&(hptr, hlen, hw)) = hoisted_arrays.get(arr) {
                            (hptr, hlen, hw) // 方案 B: loop-invariant, hoisted in entry block
                        } else {
                            let ss_ptr = builder.create_sized_stack_slot(
                                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
                            let ss_len = builder.create_sized_stack_slot(
                                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
                            let ss_width = builder.create_sized_stack_slot(
                                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
                            let ptr_addr = builder.ins().stack_addr(ptr, ss_ptr, 0);
                            let len_addr = builder.ins().stack_addr(ptr, ss_len, 0);
                            let width_addr = builder.ins().stack_addr(ptr, ss_width, 0);
                            let a_c = builder.ins().iconst(types::I32, *arr as i64);
                            let inst = builder.ins().call(hr_array_data,
                                &[frame_val, ctx_val, a_c, ptr_addr, len_addr, width_addr]);
                            let ret = builder.inst_results(inst)[0];
                            check!(ret); // not-an-array → exception exit (方案 A)
                            let dp = builder.ins().stack_load(ptr, ss_ptr, 0);
                            let dl = builder.ins().stack_load(types::I64, ss_len, 0);
                            let dw = builder.ins().stack_load(types::I64, ss_width, 0);
                            (dp, dl, dw)
                        };
                        // idx payload (i64) from regs[idx]
                        let idx_addr = reg_addr(&mut builder, regs_base, *idx);
                        let idx_v = load_payload_i64(&mut builder, idx_addr);
                        // width==0 → non-packed backing (`Boxed`/`Bytes`/…), e.g. a
                        // closure env array read with a primitive-typed `dst`, or an
                        // `object[]` — the fast-path ptr is null there, so route to
                        // the `jit_array_get` helper (`get_boxed` returns the value).
                        let width_zero = builder.ins().icmp_imm(IntCC::Equal, width, 0);
                        let helper_blk = builder.create_block();
                        let fast_blk   = builder.create_block();
                        let done_blk   = builder.create_block();
                        builder.ins().brif(width_zero, helper_blk, &[], fast_blk, &[]);
                        // helper fallback: identical to the non-inline path.
                        builder.switch_to_block(helper_blk);
                        let d = ri!(*dst); let a = ri!(*arr); let i = ri!(*idx);
                        let hinst = builder.ins().call(hr_array_get, &[frame_val, ctx_val, d, a, i]);
                        let hret  = builder.inst_results(hinst)[0];
                        check!(hret);
                        builder.ins().jump(done_blk, &[]);
                        // packed fast path: bounds-check, then native element load.
                        builder.switch_to_block(fast_blk);
                        let oob = builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, idx_v, len);
                        let oob_blk = builder.create_block();
                        let in_blk  = builder.create_block();
                        builder.ins().brif(oob, oob_blk, &[], in_blk, &[]);
                        // cold OOB: reuse jit_array_get to set the identical exception.
                        builder.switch_to_block(oob_blk);
                        let d_c = builder.ins().iconst(types::I32, *dst as i64);
                        let a_c2 = builder.ins().iconst(types::I32, *arr as i64);
                        let i_c = builder.ins().iconst(types::I32, *idx as i64);
                        builder.ins().call(hr_array_get, &[frame_val, ctx_val, d_c, a_c2, i_c]);
                        emit_dispatch_to_catch_or_return!();
                        // in-bounds: native element load + unboxed store. The packed
                        // array buffer is contiguous `arr_width`-byte slots with NO
                        // per-element tag; when width!=0 the runtime backing matches
                        // the compile-time `arr_width` (dst = element type). width-4
                        // (`int[]`) sign-extends into the i64 payload; width-8
                        // (`long[]`/`double[]`) is a raw load. Tag = static `val_tag`.
                        builder.switch_to_block(in_blk);
                        let stride_c = builder.ins().iconst(types::I64, arr_width);
                        let elem_off = builder.ins().imul(idx_v, stride_c);
                        let elem_addr = builder.ins().iadd(data_ptr, elem_off);
                        let elem = if arr_width == 4 {
                            let e32 = builder.ins().load(types::I32, MemFlags::trusted(), elem_addr, 0);
                            builder.ins().sextend(types::I64, e32)
                        } else {
                            builder.ins().load(types::I64, MemFlags::trusted(), elem_addr, 0)
                        };
                        // store into the 16-byte register `Value` (tag + payload).
                        let dst_addr = reg_addr(&mut builder, regs_base, *dst);
                        let tag_c = builder.ins().iconst(types::I8, val_tag); // I64=0 / F64=1
                        store_tagged(&mut builder, dst_addr, tag_c, elem);
                        builder.ins().jump(done_blk, &[]);
                        builder.switch_to_block(done_blk);
                    } else {
                        let d = ri!(*dst); let a = ri!(*arr); let i = ri!(*idx);
                        let inst = builder.ins().call(hr_array_get, &[frame_val, ctx_val, d, a, i]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::ArraySet { arr, idx, val } => {
                    // jit-inline-fastpaths: i64 element store → native bounds-check
                    // + native store (no write barrier needed — i64 is drop-free
                    // and a type-correct `long[]` slot's old value is also i64).
                    // Data ptr+len from the hoist (方案 B) or per-set `jit_array_data`.
                    // Cold OOB / null reuses `jit_array_set` (identical exception,
                    // + write barrier for the heap-ref-value case that stays here).
                    if arr_prim_elem(z42_func, *val).is_some() && idx_int_ok(z42_func, *idx) {
                        // jit-inline-i32-arrays: the value register's width does NOT
                        // reliably match the array element width (a narrowing store
                        // `int[i] = <i64 value>` has an i64 value into a 4-byte slot),
                        // and the IR carries no element type on the array reg. So the
                        // store width comes from the RUNTIME backing (`out_width`):
                        // 4 (`int[]`), 8 (`long[]`/`double[]`), or 0 (non-packed →
                        // fall back to the helper, which narrows/boxes + write-barriers).
                        use cranelift_codegen::ir::condcodes::IntCC;
                        use cranelift_codegen::ir::{StackSlotData, StackSlotKind};
                        let (data_ptr, len, width) = if let Some(&(hptr, hlen, hw)) = hoisted_arrays.get(arr) {
                            (hptr, hlen, hw)
                        } else {
                            let ss_ptr = builder.create_sized_stack_slot(
                                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
                            let ss_len = builder.create_sized_stack_slot(
                                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
                            let ss_width = builder.create_sized_stack_slot(
                                StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
                            let ptr_addr = builder.ins().stack_addr(ptr, ss_ptr, 0);
                            let len_addr = builder.ins().stack_addr(ptr, ss_len, 0);
                            let width_addr = builder.ins().stack_addr(ptr, ss_width, 0);
                            let a_c = builder.ins().iconst(types::I32, *arr as i64);
                            let inst = builder.ins().call(hr_array_data,
                                &[frame_val, ctx_val, a_c, ptr_addr, len_addr, width_addr]);
                            let ret = builder.inst_results(inst)[0];
                            check!(ret);
                            let dp = builder.ins().stack_load(ptr, ss_ptr, 0);
                            let dl = builder.ins().stack_load(types::I64, ss_len, 0);
                            let dw = builder.ins().stack_load(types::I64, ss_width, 0);
                            (dp, dl, dw)
                        };
                        let idx_addr = reg_addr(&mut builder, regs_base, *idx);
                        let idx_v = load_payload_i64(&mut builder, idx_addr);
                        let val_addr = reg_addr(&mut builder, regs_base, *val);
                        let val_v = load_payload_i64(&mut builder, val_addr);
                        // width==0 → non-packed backing (byte[]/Boxed/bool[]/char[]) →
                        // route to the helper (narrowing/boxing + write barrier).
                        let width_zero = builder.ins().icmp_imm(IntCC::Equal, width, 0);
                        let helper_blk = builder.create_block();
                        let fast_blk   = builder.create_block();
                        let done_blk   = builder.create_block();
                        builder.ins().brif(width_zero, helper_blk, &[], fast_blk, &[]);
                        // helper fallback: identical semantics to the non-inline path.
                        builder.switch_to_block(helper_blk);
                        let a = ri!(*arr); let i = ri!(*idx); let v = ri!(*val);
                        let hinst = builder.ins().call(hr_array_set, &[frame_val, ctx_val, a, i, v]);
                        let hret  = builder.inst_results(hinst)[0];
                        check!(hret);
                        builder.ins().jump(done_blk, &[]);
                        // packed fast path: bounds-check, then native store by runtime width.
                        builder.switch_to_block(fast_blk);
                        let oob = builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, idx_v, len);
                        let oob_blk   = builder.create_block();
                        let store_blk = builder.create_block();
                        builder.ins().brif(oob, oob_blk, &[], store_blk, &[]);
                        // cold OOB: reuse jit_array_set (identical exception).
                        builder.switch_to_block(oob_blk);
                        let a_c2 = builder.ins().iconst(types::I32, *arr as i64);
                        let i_c = builder.ins().iconst(types::I32, *idx as i64);
                        let v_c = builder.ins().iconst(types::I32, *val as i64);
                        builder.ins().call(hr_array_set, &[frame_val, ctx_val, a_c2, i_c, v_c]);
                        emit_dispatch_to_catch_or_return!();
                        // in-bounds: elem_addr = data_ptr + idx*width; store `width`
                        // bytes — width-4 truncates the i64 payload, width-8 raw.
                        builder.switch_to_block(store_blk);
                        let elem_off = builder.ins().imul(idx_v, width);
                        let elem_addr = builder.ins().iadd(data_ptr, elem_off);
                        let is_w4 = builder.ins().icmp_imm(IntCC::Equal, width, 4);
                        let store4_blk = builder.create_block();
                        let store8_blk = builder.create_block();
                        builder.ins().brif(is_w4, store4_blk, &[], store8_blk, &[]);
                        builder.switch_to_block(store4_blk);
                        let v32 = builder.ins().ireduce(types::I32, val_v);
                        builder.ins().store(MemFlags::trusted(), v32, elem_addr, 0);
                        builder.ins().jump(done_blk, &[]);
                        builder.switch_to_block(store8_blk);
                        builder.ins().store(MemFlags::trusted(), val_v, elem_addr, 0);
                        builder.ins().jump(done_blk, &[]);
                        builder.switch_to_block(done_blk);
                    } else {
                        let a = ri!(*arr); let i = ri!(*idx); let v = ri!(*val);
                        let inst = builder.ins().call(hr_array_set, &[frame_val, ctx_val, a, i, v]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::ArrayLen { dst, arr } => {
                    let d = ri!(*dst); let a = ri!(*arr);
                    let inst = builder.ins().call(hr_array_len, &[frame_val, ctx_val, d, a]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                }

                // Objects
                Instruction::ObjNew(insn) => {
                    // add-escape-analysis-stack-alloc: JIT ignores stack_alloc in v1
                    // (heap-allocates); the optimization targets interp (interp-first).
                    let ObjNewInsn { dst, class_name, ctor_name, args, type_args, stack_alloc: _ } = &**insn;
                    // 2026-05-07 expand-jit-type-args: marshal `Vec<String>` as a
                    // `*const String` + count to `jit_obj_new`. The IR storage
                    // lives for the module lifetime, so the raw pointer is valid
                    // for the duration of all JIT-compiled calls.
                    let d = ri!(*dst);
                    let (cp, cl) = str_val!(class_name);
                    let (kp, kl) = str_val!(ctor_name);
                    let (ap, al) = regs_val!(args);
                    let tap = builder.ins().iconst(ptr, type_args.as_ptr() as i64);
                    let tac = builder.ins().iconst(types::I64, type_args.len() as i64);
                    let inst = builder.ins().call(hr_obj_new,
                        &[frame_val, ctx_val, d, cp, cl, kp, kl, ap, al, tap, tac]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                }
                Instruction::Typeof(insn) => {
                    // add-reflection-generic-type-definition: marshal type_name +
                    // the IR `type_args: Box<[String]>` storage as `*const String`
                    // + count (mirrors ObjNew type_args). Helper can't throw.
                    let TypeofInsn { dst, type_name, type_args } = &**insn;
                    let d = ri!(*dst);
                    let (np, nl) = str_val!(type_name);
                    let tap = builder.ins().iconst(ptr, type_args.as_ptr() as i64);
                    let tac = builder.ins().iconst(types::I64, type_args.len() as i64);
                    builder.ins().call(hr_typeof, &[frame_val, ctx_val, d, np, nl, tap, tac]);
                }
                // formalize-jit-method-token Phase 2.E (2026-05-08): emit
                // FieldIC pointer as i64 const so helper can take IC fast
                // path on monomorphic sites. Pointer is stable through
                // Function.resolved (OnceLock-set, never overwritten).
                Instruction::FieldGet(insn) => {
                    let FieldGetInsn { dst, obj, field_name } = &**insn;
                    // P5-B: inline-primitive field of a hoisted (never-reassigned)
                    // object → native width-aware byte load + widen into the 16B
                    // register (mirrors `decode_prim`). `off < 0` (null / non-object /
                    // field-not-found / reference / struct root / string / layout
                    // mismatch) falls back to jit_field_get (Str.Length / Array.Length /
                    // null-throw / field-not-found→Null all preserved). Paths converge.
                    let hoisted = hoisted_fields.get(&(*obj, field_name.clone())).copied();
                    if let (Some(fk), Some((bytes_ptr, off))) =
                        (field_prim_kind(z42_func, *dst), hoisted)
                    {
                        use cranelift_codegen::ir::condcodes::IntCC;
                        let bad = builder.ins().icmp_imm(IntCC::SignedLessThan, off, 0);
                        let fb_blk = builder.create_block();
                        let native_blk = builder.create_block();
                        let cont_blk = builder.create_block();
                        builder.ins().brif(bad, fb_blk, &[], native_blk, &[]);
                        // fallback: full helper (may continue OR throw via check!).
                        builder.switch_to_block(fb_blk);
                        let d = ri!(*dst); let o = ri!(*obj);
                        let (fp, fl) = str_val!(field_name);
                        let ic_ptr = field_ic_ptr_at(z42_func, block_idx, instr_idx);
                        let ic_val = builder.ins().iconst(ptr, ic_ptr as i64);
                        let inst = builder.ins().call(hr_field_get, &[frame_val, ctx_val, d, o, fp, fl, ic_val]);
                        let ret = builder.inst_results(inst)[0];
                        check!(ret);
                        builder.ins().jump(cont_blk, &[]);
                        // native byte load at bytes_ptr+off, widen per field type, store reg.
                        builder.switch_to_block(native_blk);
                        let elem_addr = builder.ins().iadd(bytes_ptr, off);
                        let raw = builder.ins().load(fk.load_ty, MemFlags::trusted(), elem_addr, 0);
                        let payload = match fk.ext {
                            FieldExt::Sext  => builder.ins().sextend(types::I64, raw),
                            FieldExt::Uext  => builder.ins().uextend(types::I64, raw),
                            FieldExt::Keep | FieldExt::Float => raw, // I64 / F64 stored as-is
                        };
                        let dst_addr = reg_addr(&mut builder, regs_base, *dst);
                        let tag_c = builder.ins().iconst(types::I8, fk.reg_tag);
                        store_tagged(&mut builder, dst_addr, tag_c, payload);
                        builder.ins().jump(cont_blk, &[]);
                        builder.switch_to_block(cont_blk);
                    } else {
                        let d = ri!(*dst); let o = ri!(*obj);
                        let (fp, fl) = str_val!(field_name);
                        let ic_ptr = field_ic_ptr_at(z42_func, block_idx, instr_idx);
                        let ic_val = builder.ins().iconst(ptr, ic_ptr as i64);
                        let inst = builder.ins().call(hr_field_get, &[frame_val, ctx_val, d, o, fp, fl, ic_val]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::FieldSet(insn) => {
                    let FieldSetInsn { obj, field_name, val } = &**insn;
                    // P5-B: inline-primitive field on a hoisted object → native
                    // width-aware byte store at `bytes_ptr + off` (mirrors
                    // `encode_prim`; low `width` bytes of the register payload). No
                    // write barrier (primitive is not a heap ref). `off < 0` /
                    // reference / struct root / string / layout mismatch → jit_field_set
                    // (write barrier + full semantics). z42 has no implicit narrowing,
                    // so `val`'s static width equals the packed field width.
                    let hoisted = hoisted_fields.get(&(*obj, field_name.clone())).copied();
                    if let (Some(fk), Some((bytes_ptr, off))) =
                        (field_prim_kind(z42_func, *val), hoisted)
                    {
                        use cranelift_codegen::ir::condcodes::IntCC;
                        let bad = builder.ins().icmp_imm(IntCC::SignedLessThan, off, 0);
                        let fb_blk = builder.create_block();
                        let native_blk = builder.create_block();
                        let cont_blk = builder.create_block();
                        builder.ins().brif(bad, fb_blk, &[], native_blk, &[]);
                        builder.switch_to_block(fb_blk);
                        let o = ri!(*obj);
                        let (fp, fl) = str_val!(field_name);
                        let v = ri!(*val);
                        let ic_ptr = field_ic_ptr_at(z42_func, block_idx, instr_idx);
                        let ic_val = builder.ins().iconst(ptr, ic_ptr as i64);
                        let inst = builder.ins().call(hr_field_set, &[frame_val, ctx_val, o, fp, fl, v, ic_val]);
                        let ret = builder.inst_results(inst)[0];
                        check!(ret);
                        builder.ins().jump(cont_blk, &[]);
                        builder.switch_to_block(native_blk);
                        let val_addr = reg_addr(&mut builder, regs_base, *val);
                        let elem_addr = builder.ins().iadd(bytes_ptr, off);
                        if fk.ext == FieldExt::Float {
                            // f64 field: store the 8-byte payload verbatim.
                            let v = load_payload(&mut builder, val_addr, types::F64);
                            builder.ins().store(MemFlags::trusted(), v, elem_addr, 0);
                        } else {
                            // integer field: take the low `width` bytes of the i64 payload
                            // (ireduce = the same truncation `encode_prim`'s `as uN` does).
                            let v64 = load_payload_i64(&mut builder, val_addr);
                            let to_store = if fk.load_ty == types::I64 {
                                v64
                            } else {
                                builder.ins().ireduce(fk.load_ty, v64)
                            };
                            builder.ins().store(MemFlags::trusted(), to_store, elem_addr, 0);
                        }
                        builder.ins().jump(cont_blk, &[]);
                        builder.switch_to_block(cont_blk);
                    } else {
                        let o = ri!(*obj);
                        let (fp, fl) = str_val!(field_name);
                        let v = ri!(*val);
                        let ic_ptr = field_ic_ptr_at(z42_func, block_idx, instr_idx);
                        let ic_val = builder.ins().iconst(ptr, ic_ptr as i64);
                        let inst = builder.ins().call(hr_field_set, &[frame_val, ctx_val, o, fp, fl, v, ic_val]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                // Phase 2.E: emit VCallIC pointer as trailing helper arg.
                Instruction::VCall(insn) => {
                    let VCallInsn { dst, obj, method, args } = &**insn;
                    let d = ri!(*dst); let o = ri!(*obj);
                    let (mp, ml) = str_val!(method);
                    let (ap, al) = regs_val!(args);
                    let ic_ptr = vcall_ic_ptr_at(z42_func, block_idx, instr_idx);
                    let ic_val = builder.ins().iconst(ptr, ic_ptr as i64);
                    // 2026-05-10 jit-stack-trace + span-column-propagate.
                    let (line, col) = crate::interp::resolve_line(z42_func.line_table(), block_idx as u32, instr_idx as u32);
                    let line_val = builder.ins().iconst(types::I32, line as i64);
                    let col_val  = builder.ins().iconst(types::I32, col as i64);
                    let off_val = builder.ins().iconst(types::I32, z42_func.linear_offset(block_idx as u32, instr_idx as u32) as i64);
                    let inst = builder.ins().call(hr_vcall, &[frame_val, ctx_val, d, o, mp, ml, ap, al, ic_val, line_val, col_val, off_val]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                }
                Instruction::IsInstance(insn) => {
                    let IsInstanceInsn { dst, obj, class_name } = &**insn;
                    let d = ri!(*dst); let o = ri!(*obj);
                    let (cp, cl) = str_val!(class_name);
                    builder.ins().call(hr_is_instance, &[frame_val, ctx_val, d, o, cp, cl]);
                }
                Instruction::AsCast(insn) => {
                    let AsCastInsn { dst, obj, class_name } = &**insn;
                    let d = ri!(*dst); let o = ri!(*obj);
                    let (cp, cl) = str_val!(class_name);
                    builder.ins().call(hr_as_cast, &[frame_val, ctx_val, d, o, cp, cl]);
                }

                // Static fields
                // formalize-jit-method-token Phase 2 (2026-05-08): emit
                // pre-resolved StaticFieldId directly. make-vm-loading-lazy: a
                // lazily-loaded fn has no resolved table → id is UNRESOLVED and
                // the helper resolves the field by NAME (passed as ptr+len).
                Instruction::StaticGet(insn) => {
                    let StaticGetInsn { dst, field } = &**insn;
                    let d = ri!(*dst);
                    let (fp, fl) = str_val!(field);
                    let field_id = static_field_id_at(z42_func, block_idx, instr_idx);
                    let id_val = builder.ins().iconst(types::I32, field_id as i64);
                    builder.ins().call(hr_static_get, &[frame_val, ctx_val, d, id_val, fp, fl]);
                }
                Instruction::StaticSet(insn) => {
                    let StaticSetInsn { field, val } = &**insn;
                    let v = ri!(*val);
                    let (fp, fl) = str_val!(field);
                    let field_id = static_field_id_at(z42_func, block_idx, instr_idx);
                    let id_val = builder.ins().iconst(types::I32, field_id as i64);
                    builder.ins().call(hr_static_set, &[frame_val, ctx_val, id_val, v, fp, fl]);
                }

                // C1 native interop scaffold: JIT translation lands in
                // L3.M16. Refuse to compile a function that contains these
                // opcodes; caller should keep the function in Interp mode.
                Instruction::CallNative(insn) => {
                    let CallNativeInsn { module, type_name, symbol, .. } = &**insn;
                    bail!(
                        "JIT cannot translate CallNative yet (L3.M16): {module}::{type_name}::{symbol}"
                    );
                }
                Instruction::CallNativeVtable { vtable_slot, .. } => {
                    bail!(
                        "JIT cannot translate CallNativeVtable yet (L3.M16): slot={vtable_slot}"
                    );
                }
                Instruction::PinPtr { .. } => {
                    bail!("JIT cannot translate PinPtr yet (L3.M16)");
                }
                Instruction::UnpinPtr { .. } => {
                    bail!("JIT cannot translate UnpinPtr yet (L3.M16)");
                }

                // Spec impl-ref-out-in-runtime: address-load opcodes are
                // interp-only; JIT path needs Value::Ref handling + cross-
                // frame deref support which is not yet implemented (CLAUDE.md
                // "interp 全绿前不碰 JIT/AOT"). Function falls back to interp.
                Instruction::LoadLocalAddr { .. } => {
                    bail!("JIT cannot translate LoadLocalAddr yet (impl-ref-out-in-runtime; interp only)");
                }
                Instruction::LoadElemAddr { .. } => {
                    bail!("JIT cannot translate LoadElemAddr yet (impl-ref-out-in-runtime; interp only)");
                }
                Instruction::LoadFieldAddr(_) => {
                    bail!("JIT cannot translate LoadFieldAddr yet (impl-ref-out-in-runtime; interp only)");
                }
                // add-struct-jit-value-path (P5-A): blob value-type instructions are
                // emitted as calls to the struct helpers, which run on the shared
                // per-context struct arena (helper-bridge — see struct_ops.rs). The
                // struct op itself runs at interp speed; the surrounding code is
                // native. Native inline byte access is Deferred (P5-B).
                Instruction::StructAlloc(insn) => {
                    let d = ri!(insn.dst);
                    let (tp, tl) = str_val!(insn.type_name);
                    let sz = builder.ins().iconst(types::I32, insn.size as i64);
                    builder.ins().call(hr_struct_alloc, &[frame_val, ctx_val, d, tp, tl, sz]);
                }
                Instruction::StructCopy { dst, src, size } => {
                    let d = ri!(*dst); let s = ri!(*src);
                    let sz = builder.ins().iconst(types::I32, *size as i64);
                    let inst = builder.ins().call(hr_struct_copy, &[frame_val, ctx_val, d, s, sz]);
                    let ret = builder.inst_results(inst)[0]; check!(ret);
                }
                Instruction::StructFieldGetPrim { dst, base, byte_off, kind } => {
                    let d = ri!(*dst); let b = ri!(*base);
                    let off = builder.ins().iconst(types::I32, *byte_off as i64);
                    let k   = builder.ins().iconst(types::I8,  *kind as i64);
                    let inst = builder.ins().call(hr_struct_field_get_prim, &[frame_val, ctx_val, d, b, off, k]);
                    let ret = builder.inst_results(inst)[0]; check!(ret);
                }
                Instruction::StructFieldSetPrim { base, byte_off, kind, val } => {
                    let b = ri!(*base); let v = ri!(*val);
                    let off = builder.ins().iconst(types::I32, *byte_off as i64);
                    let k   = builder.ins().iconst(types::I8,  *kind as i64);
                    let inst = builder.ins().call(hr_struct_field_set_prim, &[frame_val, ctx_val, b, off, k, v]);
                    let ret = builder.inst_results(inst)[0]; check!(ret);
                }
                // 2026-05-07 D-8b-3 Phase 2 + switch-multicast-funcpredicate-to-generic-exception:
                // emit `jit_default_of(frame, ctx, dst, param_index)` helper call.
                // JIT-allocated instances still have empty type_args (jit_obj_new
                // doesn't propagate them yet), so the helper falls through to Null
                // when called on a JIT-allocated generic instance — same path as
                // method-level / free generic graceful-degradation.
                Instruction::DefaultOf { dst, param_index } => {
                    let d  = ri!(*dst);
                    let pi = builder.ins().iconst(types::I32, *param_index as i64);
                    let inst = builder.ins().call(hr_default_of, &[frame_val, ctx_val, d, pi]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                }

                // spec fix-numeric-cast-lowering (2026-05-13): explicit numeric cast
                // review.md C2 P1 follow-up (2026-05-30); Phase 2A widened src
                // to any integer type (2026-08-15): when src is an integer
                // (I8..U64, all stored as Value::I64) and to_tag is one of the
                // integer widths (I8 / I16 / I32 / I64 / U8 / U16 / U32 / U64),
                // emit the bit-mask / sign-extend directly. The result layout is
                // unchanged (still Value::I64) — just the payload bits change;
                // `emit_i64_convert` reads the full i64 payload, identical to
                // the `hr_convert` helper for any narrow-int source.
                Instruction::Convert { dst, src, to_tag } => {
                    // exec_value tag constants — keep in sync.
                    const T_I8:  u8 = 0x02;
                    const T_I16: u8 = 0x03;
                    const T_I32: u8 = 0x04;
                    const T_I64: u8 = 0x05;
                    const T_U8:  u8 = 0x06;
                    const T_U16: u8 = 0x07;
                    const T_U32: u8 = 0x08;
                    const T_U64: u8 = 0x09;
                    // jit-native-convert-float: float target tags.
                    const T_F32: u8 = 0x0A;
                    const T_F64: u8 = 0x0B;
                    let src_ty = z42_func.reg_types
                        .get(*src as usize).copied().unwrap_or(IrType::Unknown);
                    let src_is_int = src_ty.is_integer();
                    let inline_int = src_is_int
                        && matches!(*to_tag,
                            T_I8 | T_I16 | T_I32 | T_I64 | T_U8 | T_U16 | T_U32 | T_U64);
                    // jit-native-convert-float (float→int): F64 source narrowed
                    // to an integer width via saturating fcvt.
                    let inline_f64_to_int = src_ty == IrType::F64
                        && matches!(*to_tag,
                            T_I8 | T_I16 | T_I32 | T_I64 | T_U8 | T_U16 | T_U32 | T_U64);
                    if inline_int {
                        emit_i64_convert(&mut builder, regs_base, &mut cache, &promoted, *dst, *src, *to_tag);
                    } else if inline_f64_to_int {
                        emit_f64_to_int(&mut builder, regs_base, &mut cache, &promoted, *dst, *src, *to_tag);
                    } else if src_is_int && matches!(*to_tag, T_F32 | T_F64) {
                        // int → f64 native (fcvt). src signedness picks
                        // fcvt_from_sint vs fcvt_from_uint.
                        let src_signed = matches!(src_ty,
                            IrType::I8 | IrType::I16 | IrType::I32 | IrType::I64);
                        emit_int_to_f64(&mut builder, regs_base, *dst, *src, src_signed);
                    } else {
                        let d = ri!(*dst);
                        let s = ri!(*src);
                        let t = builder.ins().iconst(types::I32, *to_tag as i64);
                        let inst = builder.ins().call(hr_convert, &[frame_val, ctx_val, d, s, t]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }

                // impl-lambda-l2: lambdas / function references — JIT support
                // lands in a later iteration (L3+). Refuse to compile so the
                // caller keeps the function in Interp mode.
                // L3 closure helpers (impl-closure-l3-jit-complete).
                // Behaviour mirrors interp::exec_instr; see closure.md §6.
                Instruction::LoadFn(insn) => {
                    let LoadFnInsn { dst, func } = &**insn;
                    let d = ri!(*dst);
                    let (np, nl) = str_val!(func);
                    let inst = builder.ins().call(hr_load_fn, &[frame_val, ctx_val, d, np, nl]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                }
                // 2026-05-02 D1b: cached method group conversion
                Instruction::LoadFnCached(insn) => {
                    let LoadFnCachedInsn { dst, func, slot_id } = &**insn;
                    let d = ri!(*dst);
                    let (np, nl) = str_val!(func);
                    let sid = builder.ins().iconst(types::I32, *slot_id as i64);
                    let inst = builder.ins().call(hr_load_fn_cached,
                        &[frame_val, ctx_val, d, np, nl, sid]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                }
                Instruction::MkClos(insn) => {
                    let MkClosInsn { dst, fn_name, captures, stack_alloc } = &**insn;
                    let d = ri!(*dst);
                    let (np, nl) = str_val!(fn_name);
                    let (cp, cl) = regs_val!(captures);
                    let sa = builder.ins().iconst(types::I8, if *stack_alloc { 1 } else { 0 });
                    let inst = builder.ins().call(hr_mk_clos,
                        &[frame_val, ctx_val, d, np, nl, cp, cl, sa]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                }
                Instruction::CallIndirect { dst, callee, args } => {
                    let d = ri!(*dst);
                    let c = ri!(*callee);
                    let (ap, al) = regs_val!(args);
                    // 2026-05-10 jit-stack-trace + span-column-propagate.
                    let (line, col) = crate::interp::resolve_line(z42_func.line_table(), block_idx as u32, instr_idx as u32);
                    let line_val = builder.ins().iconst(types::I32, line as i64);
                    let col_val  = builder.ins().iconst(types::I32, col as i64);
                    let off_val = builder.ins().iconst(types::I32, z42_func.linear_offset(block_idx as u32, instr_idx as u32) as i64);
                    let inst = builder.ins().call(hr_call_indirect,
                        &[frame_val, ctx_val, d, c, ap, al, line_val, col_val, off_val]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                    // add-gc-safepoint-jit (2026-05-21): post-CallIndirect
                    // safepoint, see Instruction::Call for rationale.
                    emit_safepoint_check(&mut builder, ptr, ctx_val, frame_val, hr_check_safepoint_slow);
                }
            }
        }

        // ── Terminator ───────────────────────────────────────────────────────
        // Block-end flush (Phase 2B): spill any dirty cached scalar back to
        // `frame.regs` before the terminator — cross-block values travel through
        // memory (no block params yet; that's Phase 2C), and terminators read
        // their operands (`Ret`/`Throw` reg, `BrCond` cond) from memory.
        cache.flush(&mut builder, regs_base);
        match &z42_block.terminator {
            Terminator::Ret { reg: None } => {
                let zero = builder.ins().iconst(types::I8, 0);
                builder.ins().return_(&[zero]);
            }
            Terminator::Ret { reg: Some(r) } => {
                // 2C: a resident Variable's value lives in SSA, not memory —
                // spill it to `frame.regs[r]` so `hr_set_ret` (reads by index)
                // sees the current value. F64 residents carry the TAG_F64
                // discriminant; integer residents TAG_I64.
                if promoted.get(*r as usize).copied().unwrap_or(false) {
                    let v = builder.use_var(Variable::from_u32(*r));
                    let addr = reg_addr(&mut builder, regs_base, *r);
                    let tag = if z42_func.reg_types.get(*r as usize).copied() == Some(IrType::F64) {
                        TAG_F64
                    } else {
                        TAG_I64
                    };
                    store_const_tag(&mut builder, addr, tag, v);
                }
                let rv   = ri!(*r);
                builder.ins().call(hr_set_ret, &[frame_val, ctx_val, rv]);
                let zero = builder.ins().iconst(types::I8, 0);
                builder.ins().return_(&[zero]);
            }
            Terminator::Br { label } => {
                let target = z42_func.blocks.iter().position(|b| &b.label == label)
                    .expect("Br label not found");
                // add-gc-safepoint-jit (2026-05-21): backward branch =
                // loop back-edge; check safepoint so long-running JIT
                // loops park promptly when GC requests a pause.
                if target <= block_idx {
                    emit_safepoint_check(&mut builder, ptr, ctx_val, frame_val, hr_check_safepoint_slow);
                }
                builder.ins().jump(cl_blocks[target], &[]);
            }
            Terminator::BrCond { cond, true_label, false_label } => {
                // add-gc-safepoint-jit (2026-05-21): BrCond's runtime target
                // isn't known until cond is evaluated; check unconditionally.
                // Idle fast path is cheap; this catches loops where the
                // back-edge is a BrCond rather than a Br.
                emit_safepoint_check(&mut builder, ptr, ctx_val, frame_val, hr_check_safepoint_slow);

                let true_idx  = z42_func.blocks.iter().position(|blk| &blk.label == true_label)
                    .expect("true_label not found");
                let false_idx = z42_func.blocks.iter().position(|blk| &blk.label == false_label)
                    .expect("false_label not found");

                // C2 P1 step 4 (2026-05-28): when reg_types[cond] confirms
                // Bool, skip the `jit_get_bool` helper call entirely — load
                // the i8 payload byte directly from `frame.regs[cond]` and
                // feed it to `brif`. Closes the dominant remaining helper-
                // call cost in the canonical numeric loop (every backward
                // branch was paying a function call to read a Bool we'd
                // *just* written via the cmp fast path).
                let cond_is_bool = z42_func.reg_types
                    .get(*cond as usize)
                    .copied() == Some(IrType::Bool);
                if cond_is_bool {
                    let addr = reg_addr(&mut builder, regs_base, *cond);
                    let b    = load_payload(&mut builder, addr, types::I8);
                    builder.ins().brif(b, cl_blocks[true_idx], &[], cl_blocks[false_idx], &[]);
                } else {
                    let cv   = ri!(*cond);
                    let inst = builder.ins().call(hr_get_bool, &[frame_val, ctx_val, cv]);
                    let b    = builder.inst_results(inst)[0];
                    builder.ins().brif(b, cl_blocks[true_idx], &[], cl_blocks[false_idx], &[]);
                }
            }
            Terminator::Throw { reg } => {
                let rv = ri!(*reg);
                // 2026-05-10 jit-stack-trace + span-column-propagate: pass
                // the throw site's (line, col) so jit_throw can stamp the
                // throwing frame's FrameInfo before populating
                // Std.Exception.StackTrace. Throw is a block terminator;
                // mirror interp's "instr_idx = block.len()" so the position
                // resolves to the *last* LineEntry covering the block.
                let (line, col) = crate::interp::resolve_line(
                    z42_func.line_table(),
                    block_idx as u32,
                    z42_block.instructions.len() as u32,
                );
                let line_val = builder.ins().iconst(types::I32, line as i64);
                let col_val  = builder.ins().iconst(types::I32, col as i64);
                // add-offline-symbolication: bake throw-site offset (terminator slot).
                let off_val = builder.ins().iconst(types::I32, z42_func.linear_offset(block_idx as u32, z42_block.instructions.len() as u32) as i64);
                builder.ins().call(hr_throw, &[frame_val, ctx_val, rv, line_val, col_val, off_val]);
                emit_dispatch_to_catch_or_return!();
            }
        }

    }

    builder.seal_all_blocks();
    builder.finalize();

    jit.define_function(func_id, &mut ctx)?;
    jit.clear_context(&mut ctx);
    Ok(())
}

// ── review.md C2 P1 specialization helpers (2026-05-28) ────────────────────────
//
// Predicate + emitter for the I64-typed arithmetic fast path. Pure module-
// scope functions so translate_function's hot path can call them without the
// borrow-checker grief of closures-over-mut-builder.

/// inline-jit-safepoint-check (2026-08-01): emit the cooperative-GC safepoint
/// **fast path** inline as native load/store + branch, replacing a
/// `jit_check_safepoint` helper call (~10ns) on the hot path. See
/// `docs/spec/changes/inline-jit-safepoint-check/design.md`.
///
/// Mirrors `gc::safepoint::check_safepoint`:
/// ```text
///   vm_ctx = *(ctx + JIT_MODULE_CTX_VM_CTX_OFFSET)
///   prev   = *(vm_ctx + SAFEPOINT_SKIP_OFFSET)      // plain i32 load
///            *(vm_ctx + SAFEPOINT_SKIP_OFFSET) = prev - 1
///   if prev u> 1 { fast: continue }                 // ~99.9%
///   else         { slow: jit_check_safepoint_slow(frame, ctx); continue }
/// ```
/// The decrement is a plain (non-atomic) load/store: `safepoint_skip` is
/// single-writer per mutator (only `force_safepoint`, test-only, writes it
/// cross-thread), so RMW atomicity is unnecessary — and dropping it is what
/// makes the fast path inlinable as two bare `mov`s (the `atomic_rmw` form
/// panicked on x86_64 Cranelift lowering; the load/store form does not).
///
/// The current block ends with a `brif`; emission continues in the created
/// `fast` block, which the caller keeps building into. Blocks are sealed later
/// by `seal_all_blocks()` (per this file's convention).
fn emit_safepoint_check(
    builder:   &mut FunctionBuilder,
    ptr:       cranelift_codegen::ir::Type,
    ctx_val:   cranelift_codegen::ir::Value,
    frame_val: cranelift_codegen::ir::Value,
    hr_slow:   cranelift_codegen::ir::FuncRef,
) {
    let flags = MemFlags::trusted();
    // vm_ctx pointer lives inside JitModuleCtx.
    let vm_ctx = builder.ins().load(
        ptr, flags, ctx_val,
        crate::jit::frame::JIT_MODULE_CTX_VM_CTX_OFFSET as i32,
    );
    let skip_off = crate::vm_context::VM_CONTEXT_SAFEPOINT_SKIP_OFFSET as i32;
    let prev = builder.ins().load(types::I32, flags, vm_ctx, skip_off);
    let newv = builder.ins().iadd_imm(prev, -1);
    builder.ins().store(flags, newv, vm_ctx, skip_off);
    // prev u> 1  ⇒  still throttled, take the fast (skip) path.
    let cond = builder.ins().icmp_imm(IntCC::UnsignedGreaterThan, prev, 1);
    let fast_blk = builder.create_block();
    let slow_blk = builder.create_block();
    builder.ins().brif(cond, fast_blk, &[], slow_blk, &[]);
    builder.switch_to_block(slow_blk);
    builder.ins().call(hr_slow, &[frame_val, ctx_val]);
    builder.ins().jump(fast_blk, &[]);
    builder.switch_to_block(fast_blk);
}

/// True iff `reg_types[dst]`, `reg_types[a]`, `reg_types[b]` are all integer
/// types (`I8..U64`). Out-of-range or `Unknown` regs fall back to the slow
/// (helper-call) path.
///
/// jit-unbox-regalloc Phase 2A (2026-08-15): widened from `== I64` to
/// `is_integer()`. Every narrow integer (`I8..U64`) is physically stored as
/// `Value::I64` (payload i64 @off8), and the VM computes **all** integer
/// arithmetic/bitwise ops as signed i64 wrapping regardless of the declared
/// type (`jit_add` fast path + `int_bitop_helper` + interp `exec_value` all
/// operate on the i64 payload). So the native `iadd`/`band`/… path is
/// byte-identical to the helper for any integer type — the old `== I64`
/// predicate was leaving `int`/`uint`/`short`/… arithmetic on the helper path
/// for no reason. Narrowing is handled separately by the explicit `Convert`
/// op (`emit_i64_convert`), not here (z42 has no implicit narrowing →
/// intermediates stay i64).
///
/// **Unsigned note**: the VM (both interp and the JIT helper fallback) treats
/// `U64` uniformly as *signed* i64 for compare/shift (`numeric_lt`: `x < y`;
/// `shr`: `x >> (y & 63)` arithmetic). The native path deliberately matches
/// that (signed `icmp`, `sshr`) so `vm-jit-consistency` stays byte-identical —
/// making `U64` truly unsigned is a separate VM-wide change, not this one.
#[inline]
fn is_int_typed(func: &Function, dst: u32, a: u32, b: u32) -> bool {
    let rt = &func.reg_types;
    let get = |i: u32| rt.get(i as usize).copied().unwrap_or(IrType::Unknown);
    get(dst).is_integer() && get(a).is_integer() && get(b).is_integer()
}

/// Binary op kind passed to `emit_i64_binop`. Mirrors the subset of
/// `Instruction` variants we specialize so far.
///
/// review.md C2 P1 follow-up (2026-05-30): bitwise + shift opcodes added.
/// `Shl` / `Shr` mask the shift amount by 63 to match the helper
/// `jit_shl` / `jit_shr` behavior (`x << (y & 63)`).
#[derive(Clone, Copy)]
enum BinopKind { Add, Sub, Mul, BitAnd, BitOr, BitXor, Shl, Shr }

/// F64 binary op kind for `emit_f64_binop` (jit-native-float). `Div` is safe
/// natively: IEEE float divide-by-zero yields ±inf/NaN (no trap), unlike i64
/// `sdiv` which must stay on the helper for the catchable exception.
#[derive(Clone, Copy)]
enum F64BinopKind { Add, Sub, Mul, Div }

/// Comparison op kind for `emit_i64_cmp`.
#[derive(Clone, Copy)]
enum CmpKind { Eq, Ne, Lt, Le, Gt, Ge }

/// Bool binary op kind for `emit_bool_binop`.
#[derive(Clone, Copy)]
enum BoolBinopKind { And, Or }

/// Integer comparison fast-path predicate. Output is always `Bool` regardless
/// of input — we only need both operands to be integer types (`I8..U64`, all
/// stored as `Value::I64`). Phase 2A widened this from `== I64`; the native
/// compare is signed (`icmp`), matching the VM's uniform signed treatment of
/// all integer types incl. `U64` (see `is_int_typed`).
#[inline]
fn is_int_cmp(func: &Function, a: u32, b: u32) -> bool {
    let rt = &func.reg_types;
    let get = |i: u32| rt.get(i as usize).copied().unwrap_or(IrType::Unknown);
    get(a).is_integer() && get(b).is_integer()
}

/// Bool binary-op predicate (And/Or): all three regs are Bool.
#[inline]
fn is_bool_typed(func: &Function, dst: u32, a: u32, b: u32) -> bool {
    let rt = &func.reg_types;
    let is_bool = |i: u32| rt.get(i as usize).copied() == Some(IrType::Bool);
    is_bool(dst) && is_bool(a) && is_bool(b)
}

/// Bool unary-op predicate (Not): both regs are Bool.
#[inline]
fn is_bool_typed_unary(func: &Function, dst: u32, src: u32) -> bool {
    let rt = &func.reg_types;
    let is_bool = |i: u32| rt.get(i as usize).copied() == Some(IrType::Bool);
    is_bool(dst) && is_bool(src)
}

/// Integer unary-op predicate (BitNot / Neg fast-path): both regs are integer
/// types (`I8..U64`). Phase 2A widened this from `== I64`; native `ineg`/`bnot`
/// on the i64 payload is byte-identical to the helper (`Value::I64(-n)` /
/// `Value::I64(!n)`) for any narrow integer, all stored as `Value::I64`.
#[inline]
fn is_int_typed_unary(func: &Function, dst: u32, src: u32) -> bool {
    let rt = &func.reg_types;
    let is_int = |i: u32| rt.get(i as usize).copied().unwrap_or(IrType::Unknown).is_integer();
    is_int(dst) && is_int(src)
}

/// jit-native-float: `true` iff `dst`, `a`, `b` are all `IrType::F64` (double).
/// Only `F64` — `F32` is stored widened as `Value::F64` and must round to f32
/// precision on write, which the native `fadd`/… path does not do, so `F32`
/// keeps the helper path. Mixed int/float also stays on the helper (which
/// promotes int→f64).
#[inline]
fn is_f64_typed(func: &Function, dst: u32, a: u32, b: u32) -> bool {
    let rt = &func.reg_types;
    let is_f64 = |i: u32| rt.get(i as usize).copied() == Some(IrType::F64);
    is_f64(dst) && is_f64(a) && is_f64(b)
}

/// jit-native-float: both compare operands are `F64` (dst is Bool).
#[inline]
fn is_f64_cmp(func: &Function, a: u32, b: u32) -> bool {
    let rt = &func.reg_types;
    let is_f64 = |i: u32| rt.get(i as usize).copied() == Some(IrType::F64);
    is_f64(a) && is_f64(b)
}

/// jit-native-float: both unary operands are `F64` (Neg).
#[inline]
fn is_f64_typed_unary(func: &Function, dst: u32, src: u32) -> bool {
    let rt = &func.reg_types;
    let is_f64 = |i: u32| rt.get(i as usize).copied() == Some(IrType::F64);
    is_f64(dst) && is_f64(src)
}

/// jit-unbox-regalloc Phase 2B: does this instruction participate in the
/// block-local integer-scalar cache — i.e. does it take the native
/// `emit_i64_*` path that reads/writes the cache rather than `frame.regs`
/// directly? Every instruction for which this is `false` must be preceded by a
/// `cache.flush` so memory is authoritative (it either touches `frame.regs`
/// directly — const/copy/field/array/bool/… — or calls a Category-B helper
/// that reads/writes regs by index).
///
/// The condition MUST match the fast-path predicate at each op's match arm
/// exactly: an op only reaches `emit_i64_*` (and thus the cache) when its
/// predicate holds; otherwise it falls to the helper (non-participating). A
/// mismatch here would either desync the cache (participating op preceded by a
/// spurious flush is harmless; a *non*-participating op NOT flushed is a
/// stale-memory bug) so keep them in lock-step.
fn instr_uses_int_cache(func: &Function, instr: &Instruction) -> bool {
    match instr {
        Instruction::Add { dst, a, b }
        | Instruction::Sub { dst, a, b }
        | Instruction::Mul { dst, a, b }
        | Instruction::BitAnd { dst, a, b }
        | Instruction::BitOr { dst, a, b }
        | Instruction::BitXor { dst, a, b }
        | Instruction::Shl { dst, a, b }
        | Instruction::Shr { dst, a, b } => is_int_typed(func, *dst, *a, *b),

        Instruction::Eq { a, b, .. }
        | Instruction::Ne { a, b, .. }
        | Instruction::Lt { a, b, .. }
        | Instruction::Le { a, b, .. }
        | Instruction::Gt { a, b, .. }
        | Instruction::Ge { a, b, .. } => is_int_cmp(func, *a, *b),

        Instruction::Neg { dst, src }
        | Instruction::BitNot { dst, src } => is_int_typed_unary(func, *dst, *src),

        // Convert participates in the int cache iff its dst (integer) is written
        // via `store_int`: that is int→int (`emit_i64_convert`, reads src via
        // cache too) and float→int (`emit_f64_to_int`, reads its F64 src from
        // memory but writes the integer dst via `store_int`). int→f64 does NOT
        // participate (its dst is F64, its int src is read from memory → needs a
        // flush before so memory is authoritative). Mirror the match arm.
        Instruction::Convert { src, to_tag, .. } => {
            let src_ty = func.reg_types
                .get(*src as usize).copied().unwrap_or(IrType::Unknown);
            (src_ty.is_integer() || src_ty == IrType::F64)
                && (0x02..=0x09).contains(to_tag)
        }

        _ => false,
    }
}

/// jit-unbox-regalloc Phase 2C: compute which integer / F64 registers can be
/// promoted to Cranelift `Variable`s — kept resident in SSA / machine registers
/// across the whole function, INCLUDING loop-carried across back-edges
/// (Cranelift's `use_var`/`def_var` + `seal_all_blocks` insert the loop phis for
/// us, so no manual loop detection / block-param threading is needed).
///
/// A reg is promotable iff it is integer- or F64-typed AND **every** one of its
/// appearances is in a position the codegen routes through `use_var`/`def_var`
/// (or spills, for the `Ret` operand). The routed ("whitelisted") positions are
/// exactly the native fast-path ops:
///   * `ConstI32`/`ConstI64`/`ConstF64` dst,
///   * `Add`/`Sub`/`Mul` dst+a+b (integer OR F64), `Div` dst+a+b (F64 only),
///   * `BitAnd`/`BitOr`/`BitXor`/`Shl`/`Shr` dst+a+b (integer only),
///   * `Neg` dst+src (integer OR F64), `BitNot` dst+src (integer only),
///   * compare (`Eq`..`Ge`) a+b (integer OR F64; dst is Bool → never a candidate),
///   * integer→integer / float→integer `Convert` dst (to_tag 0x02..=0x09; the
///     int→int form also routes src, float→int reads its F64 src from memory),
///   * `Ret` operand (spilled to memory before `hr_set_ret`).
/// ANY appearance in a memory-backed op (copy, field, array, call/helper,
/// struct, static, throw, int→f64 / helper convert, …) DISQUALIFIES the reg:
/// promoting it would desync its `frame.regs` slot from the resident SSA value —
/// a silent miscompile. Conservative by construction — when unsure, don't
/// promote, and the reg falls back to the 2A/2B memory+cache model (integers) or
/// direct memory (F64), byte-identical to pre-2C. Safepoints do NOT disqualify:
/// the GC is non-moving and treats an integer/F64 slot as a scalar (never a
/// root), so a resident scalar needs no spill across a safepoint (the source of
/// 2C's per-iteration win) — a stale slot always still carries a scalar/Null tag.
///
/// Returns a per-reg bool vector. The caller passes `enable=false` (→ all
/// false) for OSR variants, whose entry jumps mid-function; v1 keeps those on
/// the memory model to avoid OSR-entry reload complexity.
fn compute_promotable_regs(func: &Function, enable: bool) -> Vec<bool> {
    let n = func.reg_types.len().max(max_reg(func) + 1);
    let mut ok = vec![false; n];
    if !enable {
        return ok;
    }
    for r in 0..n {
        // Integer regs (I8..U64, all physically Value::I64) and F64 regs are
        // promotion candidates. F64 residency (2C-for-floats) routes through the
        // F64-native fast paths (fadd/fsub/fmul/fdiv/fcmp/fneg/ConstF64) exactly
        // as integers route through their native ops.
        let t = func.reg_types.get(r).copied().unwrap_or(IrType::Unknown);
        ok[r] = t.is_integer() || t == IrType::F64;
    }
    // Regs to disqualify (appear in a memory-backed / non-routed position).
    let mut disq: Vec<u32> = Vec::new();
    use Instruction as I;
    for block in &func.blocks {
        for instr in &block.instructions {
            match instr {
                // ── Routed to use_var/def_var ONLY when the op takes its native
                // fast path — the SAME predicate the codegen match arm uses. If
                // the predicate is false the op falls to a helper that reads/
                // writes `frame.regs` by index, so a promoted operand's stale
                // memory slot would be read → disqualify all its regs. (Const to
                // a promoted dst is routed to def_var unconditionally, so it is
                // always safe and stays whitelisted.)
                I::ConstI32 { .. } | I::ConstI64 { .. } => {}
                // ConstF64 to a promoted F64 dst is routed to def_var (like the
                // integer consts) — always safe, no disqualification.
                I::ConstF64 { .. } => {}
                // Add/Sub/Mul take a native fast path for BOTH integer
                // (`emit_i64_binop`) and F64 (`emit_f64_binop`) operands → routed
                // when either predicate holds.
                I::Add { dst, a, b } | I::Sub { dst, a, b } | I::Mul { dst, a, b } => {
                    if !(is_int_typed(func, *dst, *a, *b) || is_f64_typed(func, *dst, *a, *b)) {
                        disq.push(*dst); disq.push(*a); disq.push(*b);
                    }
                }
                // Bit ops / shifts are integer-only (no F64 form).
                I::BitAnd { dst, a, b } | I::BitOr { dst, a, b } | I::BitXor { dst, a, b }
                | I::Shl { dst, a, b } | I::Shr { dst, a, b } => {
                    if !is_int_typed(func, *dst, *a, *b) {
                        disq.push(*dst); disq.push(*a); disq.push(*b);
                    }
                }
                // Div is native for F64 (`emit_f64_binop` fdiv); integer Div goes
                // to the helper (for the /0 exception) → routed only for F64.
                I::Div { dst, a, b } => {
                    if !is_f64_typed(func, *dst, *a, *b) {
                        disq.push(*dst); disq.push(*a); disq.push(*b);
                    }
                }
                // Neg is native for integer (`emit_i64_neg`) and F64 (`emit_f64_neg`).
                I::Neg { dst, src } => {
                    if !(is_int_typed_unary(func, *dst, *src) || is_f64_typed_unary(func, *dst, *src)) {
                        disq.push(*dst); disq.push(*src);
                    }
                }
                // BitNot is integer-only.
                I::BitNot { dst, src } => {
                    if !is_int_typed_unary(func, *dst, *src) {
                        disq.push(*dst); disq.push(*src);
                    }
                }
                I::Eq { a, b, .. } | I::Ne { a, b, .. } | I::Lt { a, b, .. }
                | I::Le { a, b, .. } | I::Gt { a, b, .. } | I::Ge { a, b, .. } => {
                    // dst is Bool (never a candidate); only a,b matter, and only
                    // when the native `emit_i64_cmp` / `emit_f64_cmp` reads them
                    // via load_int / load_f64.
                    if !(is_int_cmp(func, *a, *b) || is_f64_cmp(func, *a, *b)) {
                        disq.push(*a); disq.push(*b);
                    }
                }
                I::Convert { dst, src, to_tag } => {
                    // Mirror the codegen match arm exactly:
                    //   * int→int (src int, to_tag 0x02..=0x09): `emit_i64_convert`
                    //     routes BOTH dst and src via load_int/store_int → neither
                    //     disqualifies.
                    //   * float→int (src F64, to_tag 0x02..=0x09): `emit_f64_to_int`
                    //     writes dst (int) via store_int (routed → keep), but reads
                    //     src (F64) from MEMORY → src must not be resident → disq src.
                    //   * int→f64 / helper convert: dst+src read/written via memory
                    //     (or F64 dst, non-candidate) → disqualify both.
                    let src_ty = func.reg_types
                        .get(*src as usize).copied().unwrap_or(IrType::Unknown);
                    let to_int = (0x02..=0x09).contains(to_tag);
                    if src_ty.is_integer() && to_int {
                        // int→int — both routed, no disqualification.
                    } else if src_ty == IrType::F64 && to_int {
                        // float→int — dst routed; F64 src read from memory.
                        disq.push(*src);
                    } else {
                        disq.push(*dst);
                        disq.push(*src);
                    }
                }

                // ── Non-routed (memory-backed) — disqualify every reg they touch ──
                I::ConstStr { dst, .. }
                | I::ConstBool { dst, .. } | I::ConstChar { dst, .. }
                | I::ConstNull { dst } | I::DefaultOf { dst, .. } => disq.push(*dst),
                I::Typeof(bx) => disq.push(bx.dst),
                I::Copy { dst, src } | I::Not { dst, src } | I::ToStr { dst, src }
                | I::PinPtr { dst, src } | I::StructCopy { dst, src, .. }
                | I::LoadLocalAddr { dst, slot: src } => { disq.push(*dst); disq.push(*src); }
                // Rem is helper-only (int /0 exception); integer And/Or (logical
                // short-circuit forms) and StrConcat are memory-backed helpers.
                // (Div is handled in the routed section above — native for F64.)
                I::Rem { dst, a, b }
                | I::And { dst, a, b } | I::Or { dst, a, b }
                | I::StrConcat { dst, a, b } => { disq.push(*dst); disq.push(*a); disq.push(*b); }
                I::ArrayGet { dst, arr, idx } | I::LoadElemAddr { dst, arr, idx } => {
                    disq.push(*dst); disq.push(*arr); disq.push(*idx);
                }
                I::ArraySet { arr, idx, val } => { disq.push(*arr); disq.push(*idx); disq.push(*val); }
                I::ArrayLen { dst, arr } => { disq.push(*dst); disq.push(*arr); }
                I::UnpinPtr { pinned } => disq.push(*pinned),
                I::StructFieldGetPrim { dst, base, .. } => { disq.push(*dst); disq.push(*base); }
                I::StructFieldSetPrim { base, val, .. } => { disq.push(*base); disq.push(*val); }
                I::CallIndirect { dst, callee, args } => {
                    disq.push(*dst); disq.push(*callee); disq.extend(args.iter().copied());
                }
                I::CallNativeVtable { dst, recv, args, .. } => {
                    disq.push(*dst); disq.push(*recv); disq.extend(args.iter().copied());
                }
                I::Call(bx) => { disq.push(bx.dst); disq.extend(bx.args.iter().copied()); }
                I::Builtin(bx) => { disq.push(bx.dst); disq.extend(bx.args.iter().copied()); }
                I::CallNative(bx) => { disq.push(bx.dst); disq.extend(bx.args.iter().copied()); }
                I::ObjNew(bx) => { disq.push(bx.dst); disq.extend(bx.args.iter().copied()); }
                I::VCall(bx) => { disq.push(bx.dst); disq.push(bx.obj); disq.extend(bx.args.iter().copied()); }
                I::MkClos(bx) => { disq.push(bx.dst); disq.extend(bx.captures.iter().copied()); }
                I::ArrayNew(bx) => { disq.push(bx.dst); disq.push(bx.size); }
                I::ArrayNewLit(bx) => { disq.push(bx.dst); disq.extend(bx.elems.iter().copied()); }
                I::LoadFn(bx) => disq.push(bx.dst),
                I::LoadFnCached(bx) => disq.push(bx.dst),
                I::FieldGet(bx) => { disq.push(bx.dst); disq.push(bx.obj); }
                I::FieldSet(bx) => { disq.push(bx.obj); disq.push(bx.val); }
                I::IsInstance(bx) => { disq.push(bx.dst); disq.push(bx.obj); }
                I::AsCast(bx) => { disq.push(bx.dst); disq.push(bx.obj); }
                I::StaticGet(bx) => disq.push(bx.dst),
                I::StaticSet(bx) => disq.push(bx.val),
                I::StructAlloc(bx) => disq.push(bx.dst),
                I::LoadFieldAddr(bx) => { disq.push(bx.dst); disq.push(bx.obj); }
            }
        }
        // Terminators: `Ret` operand is routed (spilled before hr_set_ret);
        // `Throw` reg is a heap exception object (never integer) — disqualify.
        if let Terminator::Throw { reg } = &block.terminator {
            disq.push(*reg);
        }
    }
    for r in disq {
        if (r as usize) < n {
            ok[r as usize] = false;
        }
    }
    ok
}

/// jit-unbox-regalloc Phase 2C: read an integer reg's i64 payload — from its
/// resident Cranelift `Variable` (via `use_var`, Cranelift inserts the SSA
/// phis) if promoted, else via the 2B block-local cache (which loads from
/// `frame.regs` on a miss).
#[inline]
fn load_int(
    builder: &mut FunctionBuilder, cache: &mut RegCache, promoted: &[bool],
    regs_base: cranelift_codegen::ir::Value, reg: u32,
) -> cranelift_codegen::ir::Value {
    if promoted.get(reg as usize).copied().unwrap_or(false) {
        builder.use_var(Variable::from_u32(reg))
    } else {
        cache.load_i64(builder, regs_base, reg)
    }
}

/// Write an integer reg's i64 payload — to its resident `Variable` (`def_var`)
/// if promoted, else to the 2B cache (deferred spill). No `frame.regs` store
/// either way until a flush (cache) or the `Ret` spill (Variable).
#[inline]
fn store_int(
    builder: &mut FunctionBuilder, cache: &mut RegCache, promoted: &[bool],
    reg: u32, val: cranelift_codegen::ir::Value,
) {
    if promoted.get(reg as usize).copied().unwrap_or(false) {
        builder.def_var(Variable::from_u32(reg), val);
    } else {
        cache.store_i64(reg, val);
    }
}

/// jit-unbox-regalloc Phase 2C (F64 residency): read an F64 reg's payload — from
/// its resident Cranelift `Variable` (declared F64-typed) if promoted, else via a
/// direct `frame.regs` memory load. F64 regs have NO block-local cache (unlike
/// the 2B integer cache — floats never enter `RegCache`); they are either
/// resident Variables or memory-backed, so no cache/flush interaction exists.
#[inline]
fn load_f64(
    builder: &mut FunctionBuilder, promoted: &[bool],
    regs_base: cranelift_codegen::ir::Value, reg: u32,
) -> cranelift_codegen::ir::Value {
    if promoted.get(reg as usize).copied().unwrap_or(false) {
        builder.use_var(Variable::from_u32(reg))
    } else {
        let addr = reg_addr(builder, regs_base, reg);
        load_payload(builder, addr, types::F64)
    }
}

/// Write an F64 reg's payload — to its resident `Variable` (`def_var`) if
/// promoted, else straight to `frame.regs[reg]` with the `TAG_F64` discriminant.
#[inline]
fn store_f64(
    builder: &mut FunctionBuilder, promoted: &[bool],
    regs_base: cranelift_codegen::ir::Value, reg: u32, val: cranelift_codegen::ir::Value,
) {
    if promoted.get(reg as usize).copied().unwrap_or(false) {
        builder.def_var(Variable::from_u32(reg), val);
    } else {
        let addr = reg_addr(builder, regs_base, reg);
        store_const_tag(builder, addr, TAG_F64, val);
    }
}

/// Emit Cranelift native code for `frame.regs[dst] = Value::I64(op(a, b))`,
/// loading both operands' i64 payloads via raw pointer arithmetic against
/// the cached `regs_base` and storing back with the I64 discriminant byte.
///
/// Layout assumption (pinned by `value_size_observed` +
/// `value_*_payload_at_offset_8` tests):
///   * Value stride 16 B, alignment 8
///   * u8 discriminant at offset 0 (TAG_I64 = 0)
///   * i64 payload at offset 8
///
/// Safety: caller must have verified `reg_types[dst] == I64` so the
/// pre-existing slot value is either `Null` (initial) or `I64`, both of
/// which have no Drop work — raw bit-copy is sound.
fn emit_i64_binop(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    cache: &mut RegCache,
    promoted: &[bool],
    dst: u32, a: u32, b: u32,
    op: BinopKind,
) {
    // Load payload i64s — resident Variable (2C) / cached SSA value (2B) /
    // memory load, resolved by `load_int`.
    let ai = load_int(builder, cache, promoted, regs_base, a);
    let bi = load_int(builder, cache, promoted, regs_base, b);

    // Compute (Cranelift `iadd`/`isub`/`imul` are wrapping by default —
    // matches z42's `vm-wrapping-int-arith` semantics).
    let result = match op {
        BinopKind::Add    => builder.ins().iadd(ai, bi),
        BinopKind::Sub    => builder.ins().isub(ai, bi),
        BinopKind::Mul    => builder.ins().imul(ai, bi),
        BinopKind::BitAnd => builder.ins().band(ai, bi),
        BinopKind::BitOr  => builder.ins().bor(ai, bi),
        BinopKind::BitXor => builder.ins().bxor(ai, bi),
        BinopKind::Shl    => {
            // Match `jit_shl` / `jit_shr`: shift amount masked to low 6 bits.
            let mask = builder.ins().iconst(types::I64, 63);
            let masked_bi = builder.ins().band(bi, mask);
            builder.ins().ishl(ai, masked_bi)
        }
        BinopKind::Shr    => {
            // Arithmetic shift (sign-extending) matches Rust's `i64 >>`.
            let mask = builder.ins().iconst(types::I64, 63);
            let masked_bi = builder.ins().band(bi, mask);
            builder.ins().sshr(ai, masked_bi)
        }
    };

    // Store to the resident Variable (2C) or the cache (2B), via `store_int`.
    store_int(builder, cache, promoted, dst, result);
}

/// Emit native I64-source integer convert (Convert opcode fast path).
/// All narrow ints (I8/I16/I32/U8/U16/U32) are stored as Value::I64
/// payload internally, so the conversion is just a sign-trunc or
/// zero-trunc of the i64 bits — output type tag stays TAG_I64.
///
/// Caller must have verified `reg_types[src].is_integer()` (I8..U64, all
/// stored as `Value::I64`) and `to_tag` ∈
/// {T_I8, T_I16, T_I32, T_I64, T_U8, T_U16, T_U32, T_U64}.
fn emit_i64_convert(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    cache: &mut RegCache,
    promoted: &[bool],
    dst: u32, src: u32, to_tag: u8,
) {
    let si = load_int(builder, cache, promoted, regs_base, src);

    // Tag constants — mirror exec_value module-private T_* (the primitive-type
    // wire tags, a DIFFERENT namespace from the `Value` discriminants).
    const T_I8:  u8 = 0x02;
    const T_I16: u8 = 0x03;
    const T_I32: u8 = 0x04;
    const T_I64: u8 = 0x05;
    const T_U8:  u8 = 0x06;
    const T_U16: u8 = 0x07;
    const T_U32: u8 = 0x08;
    const T_U64: u8 = 0x09;
    let result = match to_tag {
        // I64 / U64: no truncation — pass through.
        T_I64 | T_U64 => si,
        // Signed narrowing: ireduce → sextend back to i64 (sign-extend bits).
        T_I8  => {
            let low = builder.ins().ireduce(types::I8,  si);
            builder.ins().sextend(types::I64, low)
        }
        T_I16 => {
            let low = builder.ins().ireduce(types::I16, si);
            builder.ins().sextend(types::I64, low)
        }
        T_I32 => {
            let low = builder.ins().ireduce(types::I32, si);
            builder.ins().sextend(types::I64, low)
        }
        // Unsigned narrowing: zero-extend low N bits — equivalent to
        // bit-and with the mask.
        T_U8  => {
            let mask = builder.ins().iconst(types::I64, 0xFF);
            builder.ins().band(si, mask)
        }
        T_U16 => {
            let mask = builder.ins().iconst(types::I64, 0xFFFF);
            builder.ins().band(si, mask)
        }
        T_U32 => {
            let mask = builder.ins().iconst(types::I64, 0xFFFFFFFF);
            builder.ins().band(si, mask)
        }
        // Caller's matches!() restricts to_tag — this is unreachable.
        _ => si,
    };

    store_int(builder, cache, promoted, dst, result);
}

/// jit-native-convert-float: emit `frame.regs[dst] = Value::F64(src as f64)` for
/// an integer→float `Convert`. All narrow ints are stored as `Value::I64` with
/// the payload already sign/zero-extended to i64, so a single `fcvt_from_sint`
/// (signed src) / `fcvt_from_uint` (unsigned src) on the i64 payload reproduces
/// interp's `x as f64` / `u as f64` exactly (interp uses full f64 precision even
/// for an `F32` target — no f32 rounding — so this covers both `to_tag`
/// F32/F64). Result discriminant `TAG_F64`.
///
/// Reads `src` straight from memory: the Phase 2C promotion whitelist
/// disqualifies any reg used as a non-int-`Convert` src, so `src` is never a
/// resident Variable here.
fn emit_int_to_f64(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, src: u32, src_signed: bool,
) {
    let addr_src = reg_addr(builder, regs_base, src);
    let addr_dst = reg_addr(builder, regs_base, dst);
    let si = load_payload_i64(builder, addr_src);
    let f = if src_signed {
        builder.ins().fcvt_from_sint(types::F64, si)
    } else {
        builder.ins().fcvt_from_uint(types::F64, si)
    };
    store_const_tag(builder, addr_dst, TAG_F64, f);
}

/// jit-native-convert-float (float→int): emit `frame.regs[dst] = Value::I64(f as T)`
/// for an F64→integer `Convert`. Rust's `as` (used by interp `convert_from_f64`)
/// is a *saturating* float→int cast with NaN→0; Cranelift `fcvt_to_sint_sat` /
/// `fcvt_to_uint_sat` reproduce that byte-for-byte (same clamp-to-range, same
/// NaN→0). Every narrow int lives as a `Value::I64` payload, so the saturated
/// low-width result is sign/zero-extended back to i64 — matching interp's
/// `(f as i8) as i64` / `(f as u8) as i64` etc. `T_U64` deliberately mirrors
/// interp's `f as i64` (signed saturation to i64 range, NOT an unsigned cast —
/// see `convert_from_f64`). Result discriminant `TAG_I64`.
///
/// Reads `src` (F64) straight from memory: the Phase 2C promotion whitelist
/// disqualifies any F64 reg used as a float→int `Convert` src, so `src` is never
/// a resident Variable. Writes `dst` (integer) via `store_int` (resident
/// Variable / 2B cache / memory), so a float→int result feeding a resident
/// accumulator stays unboxed.
fn emit_f64_to_int(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    cache: &mut RegCache,
    promoted: &[bool],
    dst: u32, src: u32, to_tag: u8,
) {
    const T_I8:  u8 = 0x02;
    const T_I16: u8 = 0x03;
    const T_I32: u8 = 0x04;
    const T_I64: u8 = 0x05;
    const T_U8:  u8 = 0x06;
    const T_U16: u8 = 0x07;
    const T_U32: u8 = 0x08;
    const T_U64: u8 = 0x09;
    let addr_src = reg_addr(builder, regs_base, src);
    let f = load_payload(builder, addr_src, types::F64);
    let result = match to_tag {
        // Signed narrow widths: saturating fcvt to width, sign-extend to i64.
        T_I8  => { let v = builder.ins().fcvt_to_sint_sat(types::I8,  f); builder.ins().sextend(types::I64, v) }
        T_I16 => { let v = builder.ins().fcvt_to_sint_sat(types::I16, f); builder.ins().sextend(types::I64, v) }
        T_I32 => { let v = builder.ins().fcvt_to_sint_sat(types::I32, f); builder.ins().sextend(types::I64, v) }
        // I64 and U64 both use signed saturation to the i64 range (interp uses
        // `f as i64` for T_U64 too).
        T_I64 | T_U64 => builder.ins().fcvt_to_sint_sat(types::I64, f),
        // Unsigned narrow widths: saturating fcvt to width, zero-extend to i64.
        T_U8  => { let v = builder.ins().fcvt_to_uint_sat(types::I8,  f); builder.ins().uextend(types::I64, v) }
        T_U16 => { let v = builder.ins().fcvt_to_uint_sat(types::I16, f); builder.ins().uextend(types::I64, v) }
        T_U32 => { let v = builder.ins().fcvt_to_uint_sat(types::I32, f); builder.ins().uextend(types::I64, v) }
        // Caller's matches!() restricts to_tag to the eight integer widths.
        _ => builder.ins().fcvt_to_sint_sat(types::I64, f),
    };
    store_int(builder, cache, promoted, dst, result);
}

/// Emit native `frame.regs[dst] = frame.regs[src]` for drop-free primitive
/// slots (I64 / F64 / Bool / Char). Copies the 1 B tag at offset 0 plus
/// the 8 B payload at offset 8 — heap-ref payloads keep the helper path
/// because they need Arc::clone. Caller verified `is_drop_free_primitive`
/// on both dst and src so neither side has Drop work (review.md C2 P1
/// follow-up, 2026-05-30).
fn emit_primitive_copy(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, src: u32,
) {
    let addr_src = reg_addr(builder, regs_base, src);
    let addr_dst = reg_addr(builder, regs_base, dst);
    let tag      = load_tag(builder, addr_src);
    let payload  = load_payload_i64(builder, addr_src);
    store_tagged(builder, addr_dst, tag, payload);
}

/// Emit native `frame.regs[dst] = Value::I64(-src)` — integer negate
/// via Cranelift `ineg` (wrapping; `ineg(i64::MIN) == i64::MIN` matching
/// the helper's release-mode `-n` semantics). Caller must have verified
/// `reg_types[dst] == reg_types[src] == I64`.
fn emit_i64_neg(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    cache: &mut RegCache,
    promoted: &[bool],
    dst: u32, src: u32,
) {
    let si     = load_int(builder, cache, promoted, regs_base, src);
    let result = builder.ins().ineg(si);
    store_int(builder, cache, promoted, dst, result);
}

/// Emit native `frame.regs[dst] = Value::I64(!src)` — bitwise NOT on i64
/// via Cranelift `bnot`. Caller must have verified `reg_types[dst] ==
/// reg_types[src] == I64` (review.md C2 P1 follow-up, 2026-05-30).
fn emit_i64_bit_not(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    cache: &mut RegCache,
    promoted: &[bool],
    dst: u32, src: u32,
) {
    let si     = load_int(builder, cache, promoted, regs_base, src);
    let result = builder.ins().bnot(si);
    store_int(builder, cache, promoted, dst, result);
}

/// Emit Cranelift native `icmp <pred>` for `frame.regs[dst] = Value::Bool(a OP b)`
/// when both `a` and `b` are statically I64. Result discriminant is `TAG_BOOL`,
/// payload is the i8 comparison result.
fn emit_i64_cmp(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    cache: &mut RegCache,
    promoted: &[bool],
    dst: u32, a: u32, b: u32,
    kind: CmpKind,
) {
    use cranelift_codegen::ir::condcodes::IntCC;

    // Operands read via resident Variable (2C) / cache (2B) / memory.
    let ai = load_int(builder, cache, promoted, regs_base, a);
    let bi = load_int(builder, cache, promoted, regs_base, b);

    // Cranelift `icmp` returns an i8 (boolean: 0 or 1) — directly the
    // payload byte we need to write back. Signed compares since z42's
    // `<` / `<=` etc. are signed on all narrow integer types (i8..i64).
    let cc = match kind {
        CmpKind::Eq => IntCC::Equal,
        CmpKind::Ne => IntCC::NotEqual,
        CmpKind::Lt => IntCC::SignedLessThan,
        CmpKind::Le => IntCC::SignedLessThanOrEqual,
        CmpKind::Gt => IntCC::SignedGreaterThan,
        CmpKind::Ge => IntCC::SignedGreaterThanOrEqual,
    };
    let result_i8 = builder.ins().icmp(cc, ai, bi);

    // The dst is a Bool, not an integer — write it straight to memory (with
    // TAG_BOOL) and drop any stale integer cache entry for it. The consumer
    // (a `BrCond`) reads it from memory after the block-end flush.
    let addr_dst = reg_addr(builder, regs_base, dst);
    store_const_tag(builder, addr_dst, TAG_BOOL, result_i8);
    cache.invalidate(dst);
}

/// Emit Cranelift native `band`/`bor` on Bool operands.
/// `frame.regs[dst] = Value::Bool(a OP b)` for And/Or, statically Bool inputs.
fn emit_bool_binop(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, a: u32, b: u32,
    kind: BoolBinopKind,
) {
    let addr_a   = reg_addr(builder, regs_base, a);
    let addr_b   = reg_addr(builder, regs_base, b);
    let addr_dst = reg_addr(builder, regs_base, dst);

    // Bool payload is a single u8 at offset 8.
    let ai = load_payload(builder, addr_a, types::I8);
    let bi = load_payload(builder, addr_b, types::I8);

    let result = match kind {
        BoolBinopKind::And => builder.ins().band(ai, bi),
        BoolBinopKind::Or  => builder.ins().bor(ai, bi),
    };

    store_const_tag(builder, addr_dst, TAG_BOOL, result);
}

/// Emit Cranelift native `bnot` (xor 1) for `Value::Bool(!a)`. The src
/// payload is a single u8 (0 or 1); `xor 1` flips it. Avoids the
/// `band/bor` constant-fold subtlety of Cranelift's `bnot` on i8 (which
/// would flip ALL bits, producing 0xfe from 0x01 — wrong for a Bool slot).
fn emit_bool_not(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, src: u32,
) {
    let addr_src = reg_addr(builder, regs_base, src);
    let addr_dst = reg_addr(builder, regs_base, dst);

    let si = load_payload(builder, addr_src, types::I8);
    let one = builder.ins().iconst(types::I8, 1);
    let result = builder.ins().bxor(si, one);

    store_const_tag(builder, addr_dst, TAG_BOOL, result);
}

/// jit-native-float: emit `frame.regs[dst] = Value::F64(a OP b)` with native
/// Cranelift `fadd`/`fsub`/`fmul`/`fdiv` on the f64 payloads. Caller verified
/// all three regs are `F64`. Result discriminant `TAG_F64`. Matches interp's
/// `int_binop_helper` float arm (plain IEEE f64 arithmetic).
fn emit_f64_binop(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    promoted: &[bool],
    dst: u32, a: u32, b: u32,
    op: F64BinopKind,
) {
    // Operands read via resident F64 Variable (2C) or memory, resolved by load_f64.
    let ai = load_f64(builder, promoted, regs_base, a);
    let bi = load_f64(builder, promoted, regs_base, b);
    let result = match op {
        F64BinopKind::Add => builder.ins().fadd(ai, bi),
        F64BinopKind::Sub => builder.ins().fsub(ai, bi),
        F64BinopKind::Mul => builder.ins().fmul(ai, bi),
        F64BinopKind::Div => builder.ins().fdiv(ai, bi),
    };
    store_f64(builder, promoted, regs_base, dst, result);
}

/// jit-native-float: emit `frame.regs[dst] = Value::Bool(a OP b)` for F64 operands
/// via native `fcmp`. Uses ORDERED comparisons (NaN → false) for
/// Eq/Lt/Le/Gt/Ge and UNORDERED-or-not-equal for Ne (NaN != NaN → true),
/// matching Rust's f64 `==`/`<`/… used by interp `numeric_lt`/`ops::compare`.
fn emit_f64_cmp(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    promoted: &[bool],
    dst: u32, a: u32, b: u32,
    kind: CmpKind,
) {
    use cranelift_codegen::ir::condcodes::FloatCC;
    // F64 operands read via resident Variable (2C) / memory; dst is Bool (never
    // an F64 candidate) → written straight to memory with TAG_BOOL.
    let addr_dst = reg_addr(builder, regs_base, dst);
    let ai = load_f64(builder, promoted, regs_base, a);
    let bi = load_f64(builder, promoted, regs_base, b);
    let cc = match kind {
        CmpKind::Eq => FloatCC::Equal,             // ordered: NaN==NaN → false
        CmpKind::Ne => FloatCC::NotEqual,          // unordered: NaN!=NaN → true
        CmpKind::Lt => FloatCC::LessThan,
        CmpKind::Le => FloatCC::LessThanOrEqual,
        CmpKind::Gt => FloatCC::GreaterThan,
        CmpKind::Ge => FloatCC::GreaterThanOrEqual,
    };
    let result_i8 = builder.ins().fcmp(cc, ai, bi);
    store_const_tag(builder, addr_dst, TAG_BOOL, result_i8);
}

/// jit-native-float: emit `frame.regs[dst] = Value::F64(-src)` via native `fneg`
/// (flips the IEEE sign bit; `-NaN` stays NaN). Caller verified both F64.
fn emit_f64_neg(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    promoted: &[bool],
    dst: u32, src: u32,
) {
    let si     = load_f64(builder, promoted, regs_base, src);
    let result = builder.ins().fneg(si);
    store_f64(builder, promoted, regs_base, dst, result);
}

/// Predicate: `reg_types[reg]` is `expected`. Used by const-emit fast paths.
#[inline]
fn is_typed(func: &Function, reg: u32, expected: IrType) -> bool {
    func.reg_types.get(reg as usize).copied() == Some(expected)
}

/// post-layout JIT perf (P5-B): how to natively load/store a register's static
/// primitive type against `ScriptObject::bytes` — mirroring `decode_prim`/
/// `encode_prim`. `width` = packed byte width, `ext` = how to widen a loaded scalar
/// into the 16B register payload, `reg_tag` = the `Value` discriminant stamped into
/// the register tag byte (int types → `Value::I64` = 0; floats → `Value::F64` = 1),
/// `field_tag` = the `TAG_*` handed to `jit_obj_field_slot` for runtime validation.
/// `None` for types stage 1 does NOT inline (F32 / Bool / Char / Str / Ref / Void /
/// Unknown) → those keep the `jit_field_get`/`jit_field_set` helper. Relies on the
/// invariant that a `FieldGet` dst / `FieldSet` val is typed as the field's declared
/// type (z42 has no implicit narrowing), so this width equals the packed field width.
#[derive(Clone, Copy)]
struct FieldPrim {
    load_ty: cranelift_codegen::ir::Type,
    ext: FieldExt,
    reg_tag: i64,
    width: u32,
    field_tag: u8,
}

#[derive(Clone, Copy, PartialEq)]
enum FieldExt { Sext, Uext, Keep, Float }

fn field_prim_kind(func: &Function, reg: u32) -> Option<FieldPrim> {
    use crate::metadata::types::{
        TAG_I8, TAG_I16, TAG_I32, TAG_I64, TAG_U8, TAG_U16, TAG_U32, TAG_U64, TAG_F64,
    };
    let (load_ty, ext, reg_tag, width, field_tag) = match func.reg_types.get(reg as usize).copied()? {
        IrType::I8  => (types::I8,  FieldExt::Sext,  0, 1, TAG_I8),
        IrType::I16 => (types::I16, FieldExt::Sext,  0, 2, TAG_I16),
        IrType::I32 => (types::I32, FieldExt::Sext,  0, 4, TAG_I32),
        IrType::I64 => (types::I64, FieldExt::Keep,  0, 8, TAG_I64),
        IrType::U8  => (types::I8,  FieldExt::Uext,  0, 1, TAG_U8),
        IrType::U16 => (types::I16, FieldExt::Uext,  0, 2, TAG_U16),
        IrType::U32 => (types::I32, FieldExt::Uext,  0, 4, TAG_U32),
        IrType::U64 => (types::I64, FieldExt::Keep,  0, 8, TAG_U64),
        IrType::F64 => (types::F64, FieldExt::Float, 1, 8, TAG_F64),
        _ => return None, // F32 / Bool / Char / Str / Ref / Void / Unknown → helper
    };
    Some(FieldPrim { load_ty, ext, reg_tag, width, field_tag })
}

/// Array-element classifier for the JIT inline get/set fast path
/// (jit-inline-i32-arrays). Returns `(val_tag, arr_width)`:
/// - `val_tag`: the `Value` tag written into the 16-byte register (0=I64, 1=F64).
///   `int` is stored as `Value::I64`, so I32 uses tag 0.
/// - `arr_width`: the packed slot width in bytes (4 for I32, 8 for I64/F64).
///
/// Reliable **only** for a register whose IR type equals the array element type
/// — i.e. an ArrayGet `dst` (the compiler types the result as the element type).
/// It is NOT reliable for an ArraySet `val`, which can be wider than the element
/// on a narrowing store; the set path consults the runtime width instead and
/// uses this only as a "worth attempting to inline" gate.
fn arr_prim_elem(func: &Function, reg: u32) -> Option<(i64, i64)> {
    match func.reg_types.get(reg as usize).copied() {
        Some(IrType::I64) => Some((0, 8)),
        Some(IrType::F64) => Some((1, 8)),
        Some(IrType::I32) => Some((0, 4)),
        // jit-inline-char-arrays: `char` → `Value::Char` tag (3), width-4 slot.
        // The width-4 load sign-extends, but a valid `char` (≤ 0x10FFFF) has bit
        // 31 clear so sext == zext; the register store writes the codepoint into
        // the low 4 payload bytes + tag 3, mirroring `emit_const_char`.
        Some(IrType::Char) => Some((3, 4)),
        _ => None,
    }
}

/// Index-register gate for the array inline fast path: accept `I32` (`int i`)
/// as well as `I64` (`long i`). Both are stored as a `Value::I64` payload in the
/// register, so reading the index as an i64 is correct regardless.
fn idx_int_ok(func: &Function, reg: u32) -> bool {
    is_typed(func, reg, IrType::I64) || is_typed(func, reg, IrType::I32)
}

/// Emit native `frame.regs[dst] = Value::I64(val)` — store TAG_I64 + i64
/// payload at known offsets, no helper call. Caller must have verified
/// `reg_types[dst] == I64` (so the old slot value is Null or I64 = Drop-free).
fn emit_const_i64(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, val: i64,
) {
    let addr_dst = reg_addr(builder, regs_base, dst);
    let v        = builder.ins().iconst(types::I64, val);
    store_const_tag(builder, addr_dst, TAG_I64, v);
}

/// Emit native `frame.regs[dst] = Value::F64(val)`.
fn emit_const_f64(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, val: f64,
) {
    let addr_dst = reg_addr(builder, regs_base, dst);
    let v        = builder.ins().f64const(val);
    store_const_tag(builder, addr_dst, TAG_F64, v);
}

/// Emit native `frame.regs[dst] = Value::Bool(val)`.
fn emit_const_bool(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, val: bool,
) {
    let addr_dst = reg_addr(builder, regs_base, dst);
    let v        = builder.ins().iconst(types::I8, if val { 1 } else { 0 });
    store_const_tag(builder, addr_dst, TAG_BOOL, v);
}

/// Emit native `frame.regs[dst] = Value::Char(val)` — store TAG_CHAR + 4 B
/// codepoint payload. Caller must have verified `reg_types[dst] == Char`
/// (review.md C11 #4, 2026-05-30).
fn emit_const_char(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, val: char,
) {
    let addr_dst = reg_addr(builder, regs_base, dst);
    let v        = builder.ins().iconst(types::I32, val as u32 as i64);
    store_const_tag(builder, addr_dst, TAG_CHAR, v);
}

/// Emit native `frame.regs[dst] = Value::Null` — just stores TAG_NULL.
/// Caller must have verified the previous slot value is Drop-free (any
/// primitive `IrType` — I64/F64/Bool/Char). For Ref/Str/Unknown dst we
/// keep the helper path so the Drop runs (review.md C11 #4, 2026-05-30).
fn emit_const_null(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32,
) {
    let addr_dst = reg_addr(builder, regs_base, dst);
    // Payload slot is left as-is; the discriminant alone defines `Null`.
    store_tag_const(builder, addr_dst, TAG_NULL);
}

/// True when `reg_types[reg]` is a primitive (drop-free) type — I64 / F64
/// / Bool / Char. Used by inline `ConstNull` to verify the existing slot
/// value is safe to overwrite without running Drop.
fn is_drop_free_primitive(func: &Function, reg: u32) -> bool {
    matches!(
        func.reg_types.get(reg as usize).copied(),
        Some(IrType::I64) | Some(IrType::F64) | Some(IrType::Bool) | Some(IrType::Char)
    )
}
