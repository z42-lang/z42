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

// refactor-jit-translate-split (H2): TxCtx + per-category instruction handlers
mod ctx;
mod hoist;
use ctx::TxCtx;
mod value;
mod arith;
mod compare;
mod convert;
mod call;
mod array;
mod object;
mod structs;
mod term;

// refactor-jit-translate-split (H2): leaf emit/predicate/analysis helpers
mod unsupported;
mod ic;
mod control;
mod analysis;
mod reg_var;
mod predicates;
mod emit_int;
mod emit_fc;

use unsupported::*;
pub(crate) use unsupported::jit_unsupported_reason;
pub(crate) use analysis::max_reg;
use ic::*;
use control::*;
use analysis::*;
use reg_var::*;
use predicates::*;
use emit_int::*;
use emit_fc::*;


// ═════════════════════════════════════════════════════════════════════════════
// JIT-translatability pre-scan
// ═════════════════════════════════════════════════════════════════════════════



// ═════════════════════════════════════════════════════════════════════════════
// Exception table helper
// ═════════════════════════════════════════════════════════════════════════════


// ═════════════════════════════════════════════════════════════════════════════
// translate_function
// ═════════════════════════════════════════════════════════════════════════════






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
    let hr_obj_ref_field_slot = imp!(helper_ids.obj_ref_field_slot);
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
    let (hoisted_arrays, hoisted_fields, hoisted_ref_fields) =
        hoist::compute_hoists(&mut builder, z42_func, ptr, frame_val, ctx_val,
            hr_array_data_opt, hr_obj_field_slot, hr_obj_ref_field_slot);

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

        let mut cache = RegCache::new();
        let mut cx = TxCtx {
            builder: &mut builder, cache: &mut cache, func: z42_func,
            promoted: &promoted, cl_blocks: &cl_blocks,
            regs_base, frame_val, ctx_val, ptr, block_idx, instr_idx: 0,
            catch_info, catch_chain: &catch_chain,
            hoisted_arrays: &hoisted_arrays, hoisted_fields: &hoisted_fields,
            hoisted_ref_fields: &hoisted_ref_fields,
            hr_const_i32, hr_const_i64, hr_const_f64, hr_const_bool, hr_const_char, hr_const_null, hr_const_str, hr_copy, hr_add, hr_sub, hr_mul, hr_div, hr_rem, hr_eq, hr_ne, hr_lt, hr_le, hr_gt, hr_ge, hr_and, hr_or, hr_not, hr_neg, hr_bit_and, hr_bit_or, hr_bit_xor, hr_bit_not, hr_shl, hr_shr, hr_str_concat, hr_to_str, hr_call, hr_builtin, hr_array_new, hr_array_new_lit, hr_array_get, hr_array_data, hr_array_set, hr_array_len, hr_obj_new, hr_typeof, hr_field_get, hr_field_set, hr_vcall, hr_is_instance, hr_as_cast, hr_static_get, hr_static_set, hr_struct_alloc, hr_struct_copy, hr_struct_field_get_prim, hr_struct_field_set_prim, hr_get_bool, hr_set_ret, hr_throw, hr_install_catch, hr_match_catch_type, hr_load_fn, hr_mk_clos, hr_call_indirect, hr_load_fn_cached, hr_default_of, hr_convert, hr_check_safepoint_slow,
        };
        for (instr_idx, instr) in z42_block.instructions.iter().enumerate() {
            cx.instr_idx = instr_idx;
            // Coherence gate: non-cache-participating ops make memory authoritative.
            if !instr_uses_int_cache(z42_func, instr) { cx.cache.flush(cx.builder, cx.regs_base); }
            match instr {
                    Instruction::ConstI32 { .. } | Instruction::ConstI64 { .. } | Instruction::ConstF64 { .. } | Instruction::ConstBool { .. } | Instruction::ConstChar { .. } | Instruction::ConstNull { .. } | Instruction::ConstStr { .. } | Instruction::Copy { .. } | Instruction::StrConcat { .. } | Instruction::ToStr { .. }
                        => cx.tr_value(instr)?,
                    Instruction::Add { .. } | Instruction::Sub { .. } | Instruction::Mul { .. } | Instruction::Div { .. } | Instruction::Rem { .. } | Instruction::And { .. } | Instruction::Or { .. } | Instruction::Not { .. } | Instruction::Neg { .. } | Instruction::BitAnd { .. } | Instruction::BitOr { .. } | Instruction::BitXor { .. } | Instruction::BitNot { .. } | Instruction::Shl { .. } | Instruction::Shr { .. }
                        => cx.tr_arith(instr)?,
                    Instruction::Eq { .. } | Instruction::Ne { .. } | Instruction::Lt { .. } | Instruction::Le { .. } | Instruction::Gt { .. } | Instruction::Ge { .. }
                        => cx.tr_compare(instr)?,
                    Instruction::Convert { .. }
                        => cx.tr_convert(instr)?,
                    Instruction::Call(..) | Instruction::Builtin(..) | Instruction::LoadFn(..) | Instruction::LoadFnCached(..) | Instruction::MkClos(..) | Instruction::CallIndirect { .. }
                        => cx.tr_call(instr)?,
                    Instruction::ArrayNew(..) | Instruction::ArrayNewLit(..) | Instruction::ArrayGet { .. } | Instruction::ArraySet { .. } | Instruction::ArrayLen { .. }
                        => cx.tr_array(instr)?,
                    Instruction::ObjNew(..) | Instruction::Typeof(..) | Instruction::FieldGet(..) | Instruction::FieldSet(..) | Instruction::VCall(..) | Instruction::IsInstance(..) | Instruction::AsCast(..) | Instruction::StaticGet(..) | Instruction::StaticSet(..)
                        => cx.tr_object(instr)?,
                    Instruction::StructAlloc { .. } | Instruction::StructCopy { .. } | Instruction::StructFieldGetPrim { .. } | Instruction::StructFieldSetPrim { .. } | Instruction::DefaultOf { .. }
                        => cx.tr_structs(instr)?,
                                    Instruction::CallNative(insn) => {
                    let CallNativeInsn { module, type_name, symbol, .. } = &**insn;
                    bail!(
                        "JIT cannot translate {} yet (L3.M16): {module}::{type_name}::{symbol}",
                        unsupported_reason(instr).unwrap_or("CallNative")
                    );
                }
                                    Instruction::CallNativeVtable { vtable_slot, .. } => {
                    bail!(
                        "JIT cannot translate {} yet (L3.M16): slot={vtable_slot}",
                        unsupported_reason(instr).unwrap_or("CallNativeVtable")
                    );
                }
                                    Instruction::PinPtr { .. } => {
                    bail!("JIT cannot translate {} yet (L3.M16)", unsupported_reason(instr).unwrap_or("PinPtr"));
                }
                                    Instruction::UnpinPtr { .. } => {
                    bail!("JIT cannot translate {} yet (L3.M16)", unsupported_reason(instr).unwrap_or("UnpinPtr"));
                }

                // Spec impl-ref-out-in-runtime: address-load opcodes are
                // interp-only; JIT path needs Value::Ref handling + cross-
                // frame deref support which is not yet implemented (CLAUDE.md
                // "interp 全绿前不碰 JIT/AOT"). Function falls back to interp.
                                    Instruction::LoadLocalAddr { .. } => {
                    bail!("JIT cannot translate {} yet (impl-ref-out-in-runtime; interp only)", unsupported_reason(instr).unwrap_or("LoadLocalAddr"));
                }
                                    Instruction::LoadElemAddr { .. } => {
                    bail!("JIT cannot translate {} yet (impl-ref-out-in-runtime; interp only)", unsupported_reason(instr).unwrap_or("LoadElemAddr"));
                }
                                    Instruction::LoadFieldAddr(_) => {
                    bail!("JIT cannot translate {} yet (impl-ref-out-in-runtime; interp only)", unsupported_reason(instr).unwrap_or("LoadFieldAddr"));
                }
                // add-struct-jit-value-path (P5-A): blob value-type instructions are
                // emitted as calls to the struct helpers, which run on the shared
                // per-context struct arena (helper-bridge — see struct_ops.rs). The
                // struct op itself runs at interp speed; the surrounding code is
                // native. Native inline byte access is Deferred (P5-B).
                                    Instruction::MethodTypeArg { .. } | Instruction::MethodDefault { .. } => {
                    bail!("JIT cannot translate {} yet (add-generic-methods; interp only)",
                          unsupported_reason(instr).unwrap_or("method-level generics"));
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
            }
        }
        cx.tr_terminator(&z42_block.terminator, z42_block.instructions.len())?;


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












































