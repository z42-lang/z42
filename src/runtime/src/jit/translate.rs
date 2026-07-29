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
use crate::metadata::IrType;
use cranelift_codegen::Context;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{FuncId, Module as CraneliftModule};
use cranelift_jit::JITModule;

pub use super::helpers::HelperIds;

// ═════════════════════════════════════════════════════════════════════════════
// max_reg — largest register index used in a function
// ═════════════════════════════════════════════════════════════════════════════

pub fn max_reg(func: &Function) -> usize {
    let mut max = func.param_count.saturating_sub(1);
    for block in &func.blocks {
        for instr in &block.instructions {
            let dst: Option<u32> = match instr {
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
                // Spec impl-ref-out-in-runtime: address-load opcodes (interp
                // only; JIT body match further down emits unimplemented).
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

                // C1 native interop scaffold — JIT path lands in L3.M16; for
                // now compute dst register correctly so reg-allocator stays
                // sound when these opcodes appear in interp-mode bytecode.
                Instruction::CallNative(insn)             => Some(insn.dst),
                Instruction::CallNativeVtable { dst, .. } => Some(*dst),
                Instruction::PinPtr           { dst, .. } => Some(*dst),
                Instruction::UnpinPtr         { .. }      => None,

                // impl-lambda-l2: JIT path lands in L3+. For now compute dst
                // correctly so reg-allocation stays sound; translation falls
                // back to interp mode (see translate.rs match below).
                Instruction::LoadFn(insn)             => Some(insn.dst),
                Instruction::LoadFnCached(insn)       => Some(insn.dst),
                Instruction::CallIndirect { dst, .. } => Some(*dst),
                Instruction::MkClos(insn)             => Some(insn.dst),

                // spec fix-numeric-cast-lowering (2026-05-13)
                Instruction::Convert      { dst, .. } => Some(*dst),
            };
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

    // Entry: append function parameters to cl_blocks[0].
    builder.append_block_params_for_function_params(cl_blocks[0]);
    builder.switch_to_block(cl_blocks[0]);

    let frame_val = builder.block_params(cl_blocks[0])[0];
    let ctx_val   = builder.block_params(cl_blocks[0])[1];

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
    let hr_array_set     = imp!(helper_ids.array_set);
    let hr_array_len     = imp!(helper_ids.array_len);
    let hr_obj_new       = imp!(helper_ids.obj_new);
    let hr_typeof        = imp!(helper_ids.typeof_op);
    let hr_field_get     = imp!(helper_ids.field_get);
    let hr_field_set     = imp!(helper_ids.field_set);
    let hr_vcall         = imp!(helper_ids.vcall);
    let hr_is_instance   = imp!(helper_ids.is_instance);
    let hr_as_cast       = imp!(helper_ids.as_cast);
    let hr_static_get    = imp!(helper_ids.static_get);
    let hr_static_set    = imp!(helper_ids.static_set);
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
    let hr_check_safepoint = imp!(helper_ids.check_safepoint);
    let hr_regs_ptr        = imp!(helper_ids.regs_ptr);

    // add-gc-safepoint-jit (2026-05-21): function-entry safepoint check.
    // A spawned worker that enters JIT-compiled code immediately after
    // spawn must respect a pending GC pause before touching any roots.
    // Idle fast path is one Mutex lock + one enum compare (~10ns).
    builder.ins().call(hr_check_safepoint, &[frame_val, ctx_val]);

    // review.md C2 P1 step 1 (2026-05-28): cache `frame.regs.as_mut_ptr()`
    // for typed-arithmetic fast paths. One helper call per function (not per
    // op) yields raw `*mut Value` we use to compute slot addresses inline.
    // Pre-conditions: `JitFrame::new` pre-allocates regs with stable
    // capacity → the data pointer never moves for the function's lifetime.
    let regs_base = {
        let inst = builder.ins().call(hr_regs_ptr, &[frame_val]);
        builder.inst_results(inst)[0]
    };
    // C2 P1 fast-path layout constants live inside `emit_i64_binop` (the sole
    // consumer today); when comparison + logical ops are specialized in the
    // next chunk they'll move to module scope.

    // ── Translate each z42 block ──────────────────────────────────────────────
    for (block_idx, z42_block) in z42_func.blocks.iter().enumerate() {
        if block_idx != 0 {
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
        for (instr_idx, instr) in z42_block.instructions.iter().enumerate() {
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
                    if is_typed(z42_func, *dst, IrType::I64) {
                        emit_const_i64(&mut builder, regs_base, *dst, *val as i64);
                    } else {
                        let d = ri!(*dst); let v = builder.ins().iconst(types::I32, *val as i64);
                        builder.ins().call(hr_const_i32, &[frame_val, ctx_val, d, v]);
                    }
                }
                Instruction::ConstI64 { dst, val } => {
                    if is_typed(z42_func, *dst, IrType::I64) {
                        emit_const_i64(&mut builder, regs_base, *dst, *val);
                    } else {
                        let d = ri!(*dst); let v = builder.ins().iconst(types::I64, *val);
                        builder.ins().call(hr_const_i64, &[frame_val, ctx_val, d, v]);
                    }
                }
                Instruction::ConstF64 { dst, val } => {
                    if is_typed(z42_func, *dst, IrType::F64) {
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
                    // / Char). 24 B Value = 1 B tag at offset 0 + 8 B payload
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

                // Arithmetic — review.md C2 P1 (2026-05-28): when reg_types
                // confirm all three operands are I64, emit native Cranelift
                // iadd/isub/imul/sdiv/srem via raw load/store on frame.regs;
                // skip the extern "C" helper call entirely. Otherwise fall
                // back to the type-dispatching helper (handles Str concat,
                // F64, mixed types, etc.).
                //
                // Safety of raw store: when reg_types[dst] == I64, every
                // write to that register slot is I64 (initial Null also has
                // no Drop), so raw bit-copy without Drop is sound. Div/Rem
                // on i64 panic on /0 — keep helper for those (zero-check +
                // exception propagation lives there). Add/Sub/Mul are
                // wrapping (`vm-wrapping-int-arith`, 2026-04-28) matching
                // Cranelift defaults.
                Instruction::Add { dst, a, b } => {
                    if is_i64_typed(z42_func, *dst, *a, *b) {
                        emit_i64_binop(&mut builder, regs_base, *dst, *a, *b, BinopKind::Add);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_add, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::Sub { dst, a, b } => {
                    if is_i64_typed(z42_func, *dst, *a, *b) {
                        emit_i64_binop(&mut builder, regs_base, *dst, *a, *b, BinopKind::Sub);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_sub, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::Mul { dst, a, b } => {
                    if is_i64_typed(z42_func, *dst, *a, *b) {
                        emit_i64_binop(&mut builder, regs_base, *dst, *a, *b, BinopKind::Mul);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_mul, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::Div { dst, a, b } => {
                    // Keep helper: i64 div-by-zero must surface a catchable
                    // z42 exception. Native sdiv on x86_64 traps with SIGFPE.
                    let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                    let inst = builder.ins().call(hr_div, &[frame_val, ctx_val, d, av, bv]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                }
                Instruction::Rem { dst, a, b } => {
                    // Same as Div — keep helper for /0 exception handling.
                    let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                    let inst = builder.ins().call(hr_rem, &[frame_val, ctx_val, d, av, bv]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                }

                // Comparison — C2 P1: I64-typed operands emit Cranelift
                // `icmp <pred>` directly; Bool result stored back inline.
                Instruction::Eq { dst, a, b } => {
                    if is_i64_cmp(z42_func, *a, *b) {
                        emit_i64_cmp(&mut builder, regs_base, *dst, *a, *b, CmpKind::Eq);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        builder.ins().call(hr_eq, &[frame_val, ctx_val, d, av, bv]);
                    }
                }
                Instruction::Ne { dst, a, b } => {
                    if is_i64_cmp(z42_func, *a, *b) {
                        emit_i64_cmp(&mut builder, regs_base, *dst, *a, *b, CmpKind::Ne);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        builder.ins().call(hr_ne, &[frame_val, ctx_val, d, av, bv]);
                    }
                }
                Instruction::Lt { dst, a, b } => {
                    if is_i64_cmp(z42_func, *a, *b) {
                        emit_i64_cmp(&mut builder, regs_base, *dst, *a, *b, CmpKind::Lt);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_lt, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::Le { dst, a, b } => {
                    if is_i64_cmp(z42_func, *a, *b) {
                        emit_i64_cmp(&mut builder, regs_base, *dst, *a, *b, CmpKind::Le);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_le, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::Gt { dst, a, b } => {
                    if is_i64_cmp(z42_func, *a, *b) {
                        emit_i64_cmp(&mut builder, regs_base, *dst, *a, *b, CmpKind::Gt);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_gt, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::Ge { dst, a, b } => {
                    if is_i64_cmp(z42_func, *a, *b) {
                        emit_i64_cmp(&mut builder, regs_base, *dst, *a, *b, CmpKind::Ge);
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
                    if is_i64_typed_unary(z42_func, *dst, *src) {
                        emit_i64_neg(&mut builder, regs_base, *dst, *src);
                    } else {
                        let d = ri!(*dst); let s = ri!(*src);
                        let inst = builder.ins().call(hr_neg, &[frame_val, ctx_val, d, s]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }

                // Bitwise — review.md C2 P1 follow-up (2026-05-30): inline
                // native Cranelift band/bor/bxor/bnot/ishl/sshr when reg_types
                // confirm I64 operands. Same payload load/store layout as
                // arith; shift amount masked to low 6 bits.
                Instruction::BitAnd { dst, a, b } => {
                    if is_i64_typed(z42_func, *dst, *a, *b) {
                        emit_i64_binop(&mut builder, regs_base, *dst, *a, *b, BinopKind::BitAnd);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_bit_and, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::BitOr { dst, a, b } => {
                    if is_i64_typed(z42_func, *dst, *a, *b) {
                        emit_i64_binop(&mut builder, regs_base, *dst, *a, *b, BinopKind::BitOr);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_bit_or, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::BitXor { dst, a, b } => {
                    if is_i64_typed(z42_func, *dst, *a, *b) {
                        emit_i64_binop(&mut builder, regs_base, *dst, *a, *b, BinopKind::BitXor);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_bit_xor, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::BitNot { dst, src } => {
                    if is_i64_typed_unary(z42_func, *dst, *src) {
                        emit_i64_bit_not(&mut builder, regs_base, *dst, *src);
                    } else {
                        let d = ri!(*dst); let s = ri!(*src);
                        let inst = builder.ins().call(hr_bit_not, &[frame_val, ctx_val, d, s]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::Shl { dst, a, b } => {
                    if is_i64_typed(z42_func, *dst, *a, *b) {
                        emit_i64_binop(&mut builder, regs_base, *dst, *a, *b, BinopKind::Shl);
                    } else {
                        let (d, av, bv) = (ri!(*dst), ri!(*a), ri!(*b));
                        let inst = builder.ins().call(hr_shl, &[frame_val, ctx_val, d, av, bv]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::Shr { dst, a, b } => {
                    if is_i64_typed(z42_func, *dst, *a, *b) {
                        emit_i64_binop(&mut builder, regs_base, *dst, *a, *b, BinopKind::Shr);
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
                    let inst = builder.ins().call(hr_call, &[frame_val, ctx_val, d, mid_val, np, nl, ap, al, ic_val, line_val, col_val]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                    // add-gc-safepoint-jit (2026-05-21): post-Call safepoint
                    // — long callees may yield to a GC request that arrived
                    // partway through; the caller catches it on return.
                    builder.ins().call(hr_check_safepoint, &[frame_val, ctx_val]);
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
                    builder.ins().call(hr_array_new_lit, &[frame_val, ctx_val, d, ep, el, etp, etl]);
                }
                Instruction::ArrayGet { dst, arr, idx } => {
                    // jit-inline-fastpaths Phase 4a: when the element (`dst`) and
                    // index are statically i64, fetch the array's data ptr+len via
                    // `jit_array_data` then do a NATIVE bounds-check + element load
                    // + unboxed store — no per-element `Value` round-trip through
                    // the `jit_array_get` helper. Cold OOB path reuses
                    // `jit_array_get` so the exception message/type is identical.
                    if is_typed(z42_func, *dst, IrType::I64) && is_typed(z42_func, *idx, IrType::I64) {
                        use cranelift_codegen::ir::condcodes::IntCC;
                        use cranelift_codegen::ir::{StackSlotData, StackSlotKind};
                        const STRIDE: i64 = 24;
                        const PAYLOAD: i32 = 8;
                        let ss_ptr = builder.create_sized_stack_slot(
                            StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
                        let ss_len = builder.create_sized_stack_slot(
                            StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
                        let ptr_addr = builder.ins().stack_addr(ptr, ss_ptr, 0);
                        let len_addr = builder.ins().stack_addr(ptr, ss_len, 0);
                        let a_c = builder.ins().iconst(types::I32, *arr as i64);
                        let inst = builder.ins().call(hr_array_data,
                            &[frame_val, ctx_val, a_c, ptr_addr, len_addr]);
                        let ret = builder.inst_results(inst)[0];
                        check!(ret); // not-an-array → exception exit
                        let data_ptr = builder.ins().stack_load(ptr, ss_ptr, 0);
                        let len = builder.ins().stack_load(types::I64, ss_len, 0);
                        // idx payload (i64) from regs[idx]
                        let idx_off = builder.ins().iconst(types::I64, (*idx as i64) * STRIDE);
                        let idx_addr = builder.ins().iadd(regs_base, idx_off);
                        let idx_v = builder.ins().load(types::I64, MemFlags::trusted(), idx_addr, PAYLOAD);
                        // bounds: (u64)idx >= (u64)len → OOB (also catches negative)
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
                        // in-bounds: native element load + unboxed store.
                        builder.switch_to_block(in_blk);
                        let stride_c = builder.ins().iconst(types::I64, STRIDE);
                        let elem_off = builder.ins().imul(idx_v, stride_c);
                        let elem_addr = builder.ins().iadd(data_ptr, elem_off);
                        let elem = builder.ins().load(types::I64, MemFlags::trusted(), elem_addr, PAYLOAD);
                        let dst_off = builder.ins().iconst(types::I64, (*dst as i64) * STRIDE);
                        let dst_addr = builder.ins().iadd(regs_base, dst_off);
                        let tag0 = builder.ins().iconst(types::I8, 0); // TAG_I64
                        builder.ins().store(MemFlags::trusted(), tag0, dst_addr, 0);
                        builder.ins().store(MemFlags::trusted(), elem, dst_addr, PAYLOAD);
                    } else {
                        let d = ri!(*dst); let a = ri!(*arr); let i = ri!(*idx);
                        let inst = builder.ins().call(hr_array_get, &[frame_val, ctx_val, d, a, i]);
                        let ret  = builder.inst_results(inst)[0]; check!(ret);
                    }
                }
                Instruction::ArraySet { arr, idx, val } => {
                    let a = ri!(*arr); let i = ri!(*idx); let v = ri!(*val);
                    let inst = builder.ins().call(hr_array_set, &[frame_val, ctx_val, a, i, v]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                }
                Instruction::ArrayLen { dst, arr } => {
                    let d = ri!(*dst); let a = ri!(*arr);
                    let inst = builder.ins().call(hr_array_len, &[frame_val, ctx_val, d, a]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                }

                // Objects
                Instruction::ObjNew(insn) => {
                    let ObjNewInsn { dst, class_name, ctor_name, args, type_args } = &**insn;
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
                    let d = ri!(*dst); let o = ri!(*obj);
                    let (fp, fl) = str_val!(field_name);
                    let ic_ptr = field_ic_ptr_at(z42_func, block_idx, instr_idx);
                    let ic_val = builder.ins().iconst(ptr, ic_ptr as i64);
                    let inst = builder.ins().call(hr_field_get, &[frame_val, ctx_val, d, o, fp, fl, ic_val]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                }
                Instruction::FieldSet(insn) => {
                    let FieldSetInsn { obj, field_name, val } = &**insn;
                    let o = ri!(*obj);
                    let (fp, fl) = str_val!(field_name);
                    let v = ri!(*val);
                    let ic_ptr = field_ic_ptr_at(z42_func, block_idx, instr_idx);
                    let ic_val = builder.ins().iconst(ptr, ic_ptr as i64);
                    let inst = builder.ins().call(hr_field_set, &[frame_val, ctx_val, o, fp, fl, v, ic_val]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
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
                    let inst = builder.ins().call(hr_vcall, &[frame_val, ctx_val, d, o, mp, ml, ap, al, ic_val, line_val, col_val]);
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
                // review.md C2 P1 follow-up (2026-05-30): when src is I64 and
                // to_tag is one of the integer widths (I8 / I16 / I32 / I64 /
                // U8 / U16 / U32 / U64), emit the bit-mask / sign-extend
                // directly. z42 stores all narrow ints as Value::I64 so the
                // result layout is unchanged — just the payload bits change.
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
                    let inline_int = is_typed(z42_func, *src, IrType::I64)
                        && matches!(*to_tag,
                            T_I8 | T_I16 | T_I32 | T_I64 | T_U8 | T_U16 | T_U32 | T_U64);
                    if inline_int {
                        emit_i64_convert(&mut builder, regs_base, *dst, *src, *to_tag);
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
                    let inst = builder.ins().call(hr_call_indirect,
                        &[frame_val, ctx_val, d, c, ap, al, line_val, col_val]);
                    let ret  = builder.inst_results(inst)[0]; check!(ret);
                    // add-gc-safepoint-jit (2026-05-21): post-CallIndirect
                    // safepoint, see Instruction::Call for rationale.
                    builder.ins().call(hr_check_safepoint, &[frame_val, ctx_val]);
                }
            }
        }

        // ── Terminator ───────────────────────────────────────────────────────
        match &z42_block.terminator {
            Terminator::Ret { reg: None } => {
                let zero = builder.ins().iconst(types::I8, 0);
                builder.ins().return_(&[zero]);
            }
            Terminator::Ret { reg: Some(r) } => {
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
                    builder.ins().call(hr_check_safepoint, &[frame_val, ctx_val]);
                }
                builder.ins().jump(cl_blocks[target], &[]);
            }
            Terminator::BrCond { cond, true_label, false_label } => {
                // add-gc-safepoint-jit (2026-05-21): BrCond's runtime target
                // isn't known until cond is evaluated; check unconditionally.
                // Idle fast path is cheap; this catches loops where the
                // back-edge is a BrCond rather than a Br.
                builder.ins().call(hr_check_safepoint, &[frame_val, ctx_val]);

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
                    const VALUE_STRIDE:   i64 = 24;
                    const PAYLOAD_OFFSET: i32 = 8;
                    let off  = builder.ins().iconst(types::I64, (*cond as i64) * VALUE_STRIDE);
                    let addr = builder.ins().iadd(regs_base, off);
                    let b    = builder.ins().load(types::I8, MemFlags::trusted(), addr, PAYLOAD_OFFSET);
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
                builder.ins().call(hr_throw, &[frame_val, ctx_val, rv, line_val, col_val]);
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

/// True iff `reg_types[dst]`, `reg_types[a]`, `reg_types[b]` are all
/// `IrType::I64`. Out-of-range or `Unknown` regs fall back to the slow
/// (helper-call) path.
#[inline]
fn is_i64_typed(func: &Function, dst: u32, a: u32, b: u32) -> bool {
    let rt = &func.reg_types;
    let get = |i: u32| rt.get(i as usize).copied().unwrap_or(IrType::Unknown);
    get(dst).is_i64() && get(a).is_i64() && get(b).is_i64()
}

/// Binary op kind passed to `emit_i64_binop`. Mirrors the subset of
/// `Instruction` variants we specialize so far.
///
/// review.md C2 P1 follow-up (2026-05-30): bitwise + shift opcodes added.
/// `Shl` / `Shr` mask the shift amount by 63 to match the helper
/// `jit_shl` / `jit_shr` behavior (`x << (y & 63)`).
#[derive(Clone, Copy)]
enum BinopKind { Add, Sub, Mul, BitAnd, BitOr, BitXor, Shl, Shr }

/// Comparison op kind for `emit_i64_cmp`.
#[derive(Clone, Copy)]
enum CmpKind { Eq, Ne, Lt, Le, Gt, Ge }

/// Bool binary op kind for `emit_bool_binop`.
#[derive(Clone, Copy)]
enum BoolBinopKind { And, Or }

/// I64 comparison fast-path predicate. Output is always `Bool` regardless
/// of input — we only need to check operand types are I64.
#[inline]
fn is_i64_cmp(func: &Function, a: u32, b: u32) -> bool {
    let rt = &func.reg_types;
    let get = |i: u32| rt.get(i as usize).copied().unwrap_or(IrType::Unknown);
    get(a).is_i64() && get(b).is_i64()
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

/// I64 unary-op predicate (BitNot / Neg-i64-fast-path): both regs are I64.
#[inline]
fn is_i64_typed_unary(func: &Function, dst: u32, src: u32) -> bool {
    let rt = &func.reg_types;
    let is_i64 = |i: u32| rt.get(i as usize).copied() == Some(IrType::I64);
    is_i64(dst) && is_i64(src)
}

/// Emit Cranelift native code for `frame.regs[dst] = Value::I64(op(a, b))`,
/// loading both operands' i64 payloads via raw pointer arithmetic against
/// the cached `regs_base` and storing back with the I64 discriminant byte.
///
/// Layout assumption (pinned by `value_size_observed` +
/// `value_*_payload_at_offset_8` tests):
///   * Value stride 24 B, alignment 8
///   * u8 discriminant at offset 0 (TAG_I64 = 0)
///   * i64 payload at offset 8
///
/// Safety: caller must have verified `reg_types[dst] == I64` so the
/// pre-existing slot value is either `Null` (initial) or `I64`, both of
/// which have no Drop work — raw bit-copy is sound.
fn emit_i64_binop(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, a: u32, b: u32,
    op: BinopKind,
) {
    const VALUE_STRIDE:   i64 = 24;
    const PAYLOAD_OFFSET: i32 = 8;
    const TAG_I64:        u8  = 0;

    // Compute slot addresses: regs_base + idx * 24.
    let off_a   = builder.ins().iconst(types::I64, (a   as i64) * VALUE_STRIDE);
    let off_b   = builder.ins().iconst(types::I64, (b   as i64) * VALUE_STRIDE);
    let off_dst = builder.ins().iconst(types::I64, (dst as i64) * VALUE_STRIDE);
    let addr_a   = builder.ins().iadd(regs_base, off_a);
    let addr_b   = builder.ins().iadd(regs_base, off_b);
    let addr_dst = builder.ins().iadd(regs_base, off_dst);

    // Load payload i64s.
    let ai = builder.ins().load(types::I64, MemFlags::trusted(), addr_a, PAYLOAD_OFFSET);
    let bi = builder.ins().load(types::I64, MemFlags::trusted(), addr_b, PAYLOAD_OFFSET);

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

    // Store discriminant (u8 0 = TAG_I64) then i64 payload.
    let tag = builder.ins().iconst(types::I8, TAG_I64 as i64);
    builder.ins().store(MemFlags::trusted(), tag, addr_dst, 0);
    builder.ins().store(MemFlags::trusted(), result, addr_dst, PAYLOAD_OFFSET);
}

/// Emit native I64-source integer convert (Convert opcode fast path).
/// All narrow ints (I8/I16/I32/U8/U16/U32) are stored as Value::I64
/// payload internally, so the conversion is just a sign-trunc or
/// zero-trunc of the i64 bits — output type tag stays TAG_I64.
///
/// Caller must have verified `reg_types[src] == I64` and `to_tag` ∈
/// {T_I8, T_I16, T_I32, T_I64, T_U8, T_U16, T_U32, T_U64}.
fn emit_i64_convert(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, src: u32, to_tag: u8,
) {
    const VALUE_STRIDE:   i64 = 24;
    const PAYLOAD_OFFSET: i32 = 8;
    const TAG_I64:        u8  = 0;
    let off_src  = builder.ins().iconst(types::I64, (src as i64) * VALUE_STRIDE);
    let off_dst  = builder.ins().iconst(types::I64, (dst as i64) * VALUE_STRIDE);
    let addr_src = builder.ins().iadd(regs_base, off_src);
    let addr_dst = builder.ins().iadd(regs_base, off_dst);
    let si       = builder.ins().load(types::I64, MemFlags::trusted(), addr_src, PAYLOAD_OFFSET);

    // Tag constants — mirror exec_value module-private T_*.
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

    let tag = builder.ins().iconst(types::I8, TAG_I64 as i64);
    builder.ins().store(MemFlags::trusted(), tag,    addr_dst, 0);
    builder.ins().store(MemFlags::trusted(), result, addr_dst, PAYLOAD_OFFSET);
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
    const VALUE_STRIDE:   i64 = 24;
    const PAYLOAD_OFFSET: i32 = 8;
    let off_src  = builder.ins().iconst(types::I64, (src as i64) * VALUE_STRIDE);
    let off_dst  = builder.ins().iconst(types::I64, (dst as i64) * VALUE_STRIDE);
    let addr_src = builder.ins().iadd(regs_base, off_src);
    let addr_dst = builder.ins().iadd(regs_base, off_dst);
    let tag      = builder.ins().load(types::I8,  MemFlags::trusted(), addr_src, 0);
    let payload  = builder.ins().load(types::I64, MemFlags::trusted(), addr_src, PAYLOAD_OFFSET);
    builder.ins().store(MemFlags::trusted(), tag,     addr_dst, 0);
    builder.ins().store(MemFlags::trusted(), payload, addr_dst, PAYLOAD_OFFSET);
}

/// Emit native `frame.regs[dst] = Value::I64(-src)` — integer negate
/// via Cranelift `ineg` (wrapping; `ineg(i64::MIN) == i64::MIN` matching
/// the helper's release-mode `-n` semantics). Caller must have verified
/// `reg_types[dst] == reg_types[src] == I64`.
fn emit_i64_neg(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, src: u32,
) {
    const VALUE_STRIDE:   i64 = 24;
    const PAYLOAD_OFFSET: i32 = 8;
    const TAG_I64:        u8  = 0;
    let off_src  = builder.ins().iconst(types::I64, (src as i64) * VALUE_STRIDE);
    let off_dst  = builder.ins().iconst(types::I64, (dst as i64) * VALUE_STRIDE);
    let addr_src = builder.ins().iadd(regs_base, off_src);
    let addr_dst = builder.ins().iadd(regs_base, off_dst);
    let si       = builder.ins().load(types::I64, MemFlags::trusted(), addr_src, PAYLOAD_OFFSET);
    let result   = builder.ins().ineg(si);
    let tag      = builder.ins().iconst(types::I8, TAG_I64 as i64);
    builder.ins().store(MemFlags::trusted(), tag,    addr_dst, 0);
    builder.ins().store(MemFlags::trusted(), result, addr_dst, PAYLOAD_OFFSET);
}

/// Emit native `frame.regs[dst] = Value::I64(!src)` — bitwise NOT on i64
/// via Cranelift `bnot`. Caller must have verified `reg_types[dst] ==
/// reg_types[src] == I64` (review.md C2 P1 follow-up, 2026-05-30).
fn emit_i64_bit_not(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, src: u32,
) {
    const VALUE_STRIDE:   i64 = 24;
    const PAYLOAD_OFFSET: i32 = 8;
    const TAG_I64:        u8  = 0;
    let off_src  = builder.ins().iconst(types::I64, (src as i64) * VALUE_STRIDE);
    let off_dst  = builder.ins().iconst(types::I64, (dst as i64) * VALUE_STRIDE);
    let addr_src = builder.ins().iadd(regs_base, off_src);
    let addr_dst = builder.ins().iadd(regs_base, off_dst);
    let si       = builder.ins().load(types::I64, MemFlags::trusted(), addr_src, PAYLOAD_OFFSET);
    let result   = builder.ins().bnot(si);
    let tag      = builder.ins().iconst(types::I8, TAG_I64 as i64);
    builder.ins().store(MemFlags::trusted(), tag,    addr_dst, 0);
    builder.ins().store(MemFlags::trusted(), result, addr_dst, PAYLOAD_OFFSET);
}

/// Emit Cranelift native `icmp <pred>` for `frame.regs[dst] = Value::Bool(a OP b)`
/// when both `a` and `b` are statically I64. Result discriminant is `TAG_BOOL`,
/// payload is the i8 comparison result.
fn emit_i64_cmp(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, a: u32, b: u32,
    kind: CmpKind,
) {
    use cranelift_codegen::ir::condcodes::IntCC;
    const VALUE_STRIDE:   i64 = 24;
    const PAYLOAD_OFFSET: i32 = 8;
    const TAG_BOOL:       u8  = 2;

    let off_a   = builder.ins().iconst(types::I64, (a   as i64) * VALUE_STRIDE);
    let off_b   = builder.ins().iconst(types::I64, (b   as i64) * VALUE_STRIDE);
    let off_dst = builder.ins().iconst(types::I64, (dst as i64) * VALUE_STRIDE);
    let addr_a   = builder.ins().iadd(regs_base, off_a);
    let addr_b   = builder.ins().iadd(regs_base, off_b);
    let addr_dst = builder.ins().iadd(regs_base, off_dst);

    let ai = builder.ins().load(types::I64, MemFlags::trusted(), addr_a, PAYLOAD_OFFSET);
    let bi = builder.ins().load(types::I64, MemFlags::trusted(), addr_b, PAYLOAD_OFFSET);

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

    let tag = builder.ins().iconst(types::I8, TAG_BOOL as i64);
    builder.ins().store(MemFlags::trusted(), tag,       addr_dst, 0);
    builder.ins().store(MemFlags::trusted(), result_i8, addr_dst, PAYLOAD_OFFSET);
}

/// Emit Cranelift native `band`/`bor` on Bool operands.
/// `frame.regs[dst] = Value::Bool(a OP b)` for And/Or, statically Bool inputs.
fn emit_bool_binop(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, a: u32, b: u32,
    kind: BoolBinopKind,
) {
    const VALUE_STRIDE:   i64 = 24;
    const PAYLOAD_OFFSET: i32 = 8;
    const TAG_BOOL:       u8  = 2;

    let off_a   = builder.ins().iconst(types::I64, (a   as i64) * VALUE_STRIDE);
    let off_b   = builder.ins().iconst(types::I64, (b   as i64) * VALUE_STRIDE);
    let off_dst = builder.ins().iconst(types::I64, (dst as i64) * VALUE_STRIDE);
    let addr_a   = builder.ins().iadd(regs_base, off_a);
    let addr_b   = builder.ins().iadd(regs_base, off_b);
    let addr_dst = builder.ins().iadd(regs_base, off_dst);

    // Bool payload is a single u8 at offset 8.
    let ai = builder.ins().load(types::I8, MemFlags::trusted(), addr_a, PAYLOAD_OFFSET);
    let bi = builder.ins().load(types::I8, MemFlags::trusted(), addr_b, PAYLOAD_OFFSET);

    let result = match kind {
        BoolBinopKind::And => builder.ins().band(ai, bi),
        BoolBinopKind::Or  => builder.ins().bor(ai, bi),
    };

    let tag = builder.ins().iconst(types::I8, TAG_BOOL as i64);
    builder.ins().store(MemFlags::trusted(), tag,    addr_dst, 0);
    builder.ins().store(MemFlags::trusted(), result, addr_dst, PAYLOAD_OFFSET);
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
    const VALUE_STRIDE:   i64 = 24;
    const PAYLOAD_OFFSET: i32 = 8;
    const TAG_BOOL:       u8  = 2;

    let off_src = builder.ins().iconst(types::I64, (src as i64) * VALUE_STRIDE);
    let off_dst = builder.ins().iconst(types::I64, (dst as i64) * VALUE_STRIDE);
    let addr_src = builder.ins().iadd(regs_base, off_src);
    let addr_dst = builder.ins().iadd(regs_base, off_dst);

    let si = builder.ins().load(types::I8, MemFlags::trusted(), addr_src, PAYLOAD_OFFSET);
    let one = builder.ins().iconst(types::I8, 1);
    let result = builder.ins().bxor(si, one);

    let tag = builder.ins().iconst(types::I8, TAG_BOOL as i64);
    builder.ins().store(MemFlags::trusted(), tag,    addr_dst, 0);
    builder.ins().store(MemFlags::trusted(), result, addr_dst, PAYLOAD_OFFSET);
}

/// Predicate: `reg_types[reg]` is `expected`. Used by const-emit fast paths.
#[inline]
fn is_typed(func: &Function, reg: u32, expected: IrType) -> bool {
    func.reg_types.get(reg as usize).copied() == Some(expected)
}

/// Emit native `frame.regs[dst] = Value::I64(val)` — store TAG_I64 + i64
/// payload at known offsets, no helper call. Caller must have verified
/// `reg_types[dst] == I64` (so the old slot value is Null or I64 = Drop-free).
fn emit_const_i64(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, val: i64,
) {
    const VALUE_STRIDE:   i64 = 24;
    const PAYLOAD_OFFSET: i32 = 8;
    const TAG_I64:        u8  = 0;
    let off_dst  = builder.ins().iconst(types::I64, (dst as i64) * VALUE_STRIDE);
    let addr_dst = builder.ins().iadd(regs_base, off_dst);
    let v        = builder.ins().iconst(types::I64, val);
    let tag      = builder.ins().iconst(types::I8, TAG_I64 as i64);
    builder.ins().store(MemFlags::trusted(), tag, addr_dst, 0);
    builder.ins().store(MemFlags::trusted(), v,   addr_dst, PAYLOAD_OFFSET);
}

/// Emit native `frame.regs[dst] = Value::F64(val)`.
fn emit_const_f64(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, val: f64,
) {
    const VALUE_STRIDE:   i64 = 24;
    const PAYLOAD_OFFSET: i32 = 8;
    const TAG_F64:        u8  = 1;
    let off_dst  = builder.ins().iconst(types::I64, (dst as i64) * VALUE_STRIDE);
    let addr_dst = builder.ins().iadd(regs_base, off_dst);
    let v        = builder.ins().f64const(val);
    let tag      = builder.ins().iconst(types::I8, TAG_F64 as i64);
    builder.ins().store(MemFlags::trusted(), tag, addr_dst, 0);
    builder.ins().store(MemFlags::trusted(), v,   addr_dst, PAYLOAD_OFFSET);
}

/// Emit native `frame.regs[dst] = Value::Bool(val)`.
fn emit_const_bool(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, val: bool,
) {
    const VALUE_STRIDE:   i64 = 24;
    const PAYLOAD_OFFSET: i32 = 8;
    const TAG_BOOL:       u8  = 2;
    let off_dst  = builder.ins().iconst(types::I64, (dst as i64) * VALUE_STRIDE);
    let addr_dst = builder.ins().iadd(regs_base, off_dst);
    let v        = builder.ins().iconst(types::I8, if val { 1 } else { 0 });
    let tag      = builder.ins().iconst(types::I8, TAG_BOOL as i64);
    builder.ins().store(MemFlags::trusted(), tag, addr_dst, 0);
    builder.ins().store(MemFlags::trusted(), v,   addr_dst, PAYLOAD_OFFSET);
}

/// Emit native `frame.regs[dst] = Value::Char(val)` — store TAG_CHAR + 4 B
/// codepoint payload. Caller must have verified `reg_types[dst] == Char`
/// (review.md C11 #4, 2026-05-30).
fn emit_const_char(
    builder: &mut FunctionBuilder,
    regs_base: cranelift_codegen::ir::Value,
    dst: u32, val: char,
) {
    const VALUE_STRIDE:   i64 = 24;
    const PAYLOAD_OFFSET: i32 = 8;
    const TAG_CHAR:       u8  = 3;
    let off_dst  = builder.ins().iconst(types::I64, (dst as i64) * VALUE_STRIDE);
    let addr_dst = builder.ins().iadd(regs_base, off_dst);
    let v        = builder.ins().iconst(types::I32, val as u32 as i64);
    let tag      = builder.ins().iconst(types::I8, TAG_CHAR as i64);
    builder.ins().store(MemFlags::trusted(), tag, addr_dst, 0);
    builder.ins().store(MemFlags::trusted(), v,   addr_dst, PAYLOAD_OFFSET);
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
    const VALUE_STRIDE: i64 = 24;
    const TAG_NULL:     u8  = 5;
    let off_dst  = builder.ins().iconst(types::I64, (dst as i64) * VALUE_STRIDE);
    let addr_dst = builder.ins().iadd(regs_base, off_dst);
    let tag      = builder.ins().iconst(types::I8, TAG_NULL as i64);
    builder.ins().store(MemFlags::trusted(), tag, addr_dst, 0);
    // Payload slot is left as-is; the discriminant alone defines `Null`.
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
