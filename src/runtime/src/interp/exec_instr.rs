/// Single-instruction dispatch for the interpreter.
///
/// This file is a thin dispatcher: each `Instruction` variant matches one arm
/// that delegates to a per-category helper (see sibling `exec_*.rs` modules).
/// The match is **exhaustive** (deliberately no `_` catch-all arm);
/// adding a new `Instruction` variant produces a
/// compile error here, forcing the matching helper / category decision.
///
/// Helpers that may propagate a callee user exception return
/// `Result<Option<Value>>` and the dispatcher checks `is_some()` to forward
/// the throw upstack. All other helpers return `Result<()>`.
///
/// 热循环布局: this match is inlined into the execution loop (`exec_function_body`),
/// so every helper LLVM inlines here becomes part of one function, sharing its
/// register allocation and stack frame with the hot dispatch loop. Big or rare
/// helpers (object / array allocation, `is` / `as`, statics, address-of, reflection,
/// value structs, native interop, the string arm of `Add`) are `#[inline(never)]` so
/// their bodies — and their live values across calls — stay out of the loop; the arm
/// keeps only the operand decode and a direct call. Inline stay the arithmetic /
/// compare / copy handlers, `Call` / `VCall` / `Builtin`, and the field and element
/// accesses (`FieldGet` / `FieldSet` / `ArrayGet` / `ArraySet`): moving those out
/// measured slower — a call per access costs store-heavy loops more than the tighter
/// loop gains. The loop's own slow paths live in `exec_body_cold.rs`.

use crate::metadata::{Function, Instruction, Module, Value};
use crate::metadata::{
    BuiltinInsn, CallInsn, CallNativeInsn, FieldGetInsn, FieldSetInsn, MkClosInsn, ObjNewInsn,
    StaticGetInsn, StaticSetInsn, VCallInsn,
};
use crate::metadata::resolver::ResolvedTokens;
use crate::metadata::tokens::UNRESOLVED;
use crate::vm_context::VmContext;
use anyhow::Result;

use super::Frame;

/// Execute a single instruction.
/// Returns:
///   Ok(None)       — normal completion
///   Ok(Some(val))  — a callee threw a user exception (value-based propagation)
///   Err(e)         — internal VM error
///
/// `func` / `resolved` / `block_idx` / `instr_idx` are passed through for the
/// introduce-method-token Phase 4 dispatch fast path. Token-bearing
/// helpers (Call / Builtin / ObjNew / VCall / FieldGet / FieldSet /
/// StaticGet / StaticSet / LoadFn / MkClos) read `resolved.site_index[block_idx]
/// [instr_idx]` to find their per-kind cache slot. Non-token-bearing
/// instructions ignore these parameters.
#[allow(clippy::too_many_arguments)]
pub fn exec_instr(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    func: &Function, resolved: Option<&ResolvedTokens>, block_idx: usize, instr_idx: usize,
    instr: &Instruction,
) -> Result<Option<Value>> {
    use super::{exec_address, exec_array, exec_call, exec_object, exec_struct, exec_value, exec_vcall};
    #[cfg(feature = "native-interop")]
    use super::exec_ffi;

    // `resolved` = `func.resolved`, read once per activation by the execution loop
    // (not one acquire load per instruction). None only when the resolver never ran
    // for `func` (unit tests calling exec_function without resolver hookup) — helpers
    // fall back to string lookup then.
    // S1 (perf-interp-hot-paths): the per-site index is a nested `Vec<Vec<u32>>`
    // lookup (two pointer chases + two bounds checks). Only the 10 token-bearing
    // arms below consume it, so compute it lazily *inside* those arms via
    // `site_idx!()` rather than unconditionally here — the hot arithmetic /
    // compare / copy / branch instructions no longer pay for it every iteration.
    macro_rules! site_idx {
        () => {
            resolved
                .and_then(|r| r.site_index.get(block_idx))
                .and_then(|b| b.get(instr_idx).copied())
                .unwrap_or(UNRESOLVED)
        };
    }
    // Token-cache load: read `func.resolved.<field>[$site]` on the hot path,
    // yielding `None` when the resolver hasn't run or this site is UNRESOLVED
    // (a cross-zpkg site before its first hit). `$site` is the arm's precomputed
    // `_site_idx`; callers append `.copied()` / `.map(..)` as the table demands.
    macro_rules! cached_token {
        ($site:expr, $field:ident) => {
            resolved
                .filter(|_| $site != UNRESOLVED)
                .and_then(|r| r.$field.get($site as usize))
        };
    }

    match instr {
        // ── Constants ────────────────────────────────────────────────────────
        Instruction::ConstStr  { dst, idx } => exec_value::const_str(ctx, module, frame, *dst, *idx)?,
        Instruction::ConstI32  { dst, val } => exec_value::const_i32(frame, *dst, *val),
        Instruction::ConstI64  { dst, val } => exec_value::const_i64(frame, *dst, *val),
        Instruction::ConstF64  { dst, val } => exec_value::const_f64(frame, *dst, *val),
        Instruction::ConstBool { dst, val } => exec_value::const_bool(frame, *dst, *val),
        Instruction::ConstChar { dst, val } => exec_value::const_char(frame, *dst, *val),
        Instruction::ConstNull { dst }      => exec_value::const_null(frame, *dst),
        Instruction::Copy      { dst, src } => exec_value::copy(frame, *dst, *src)?,

        // ── Arithmetic ───────────────────────────────────────────────────────
        // A user `ToString` that throws during string concatenation comes back as
        // `Err` + the exception in `pending_thrown` (see `dispatch::tostring_threw`).
        Instruction::Add { dst, a, b } => {
            if let Err(e) = exec_value::add(ctx, module, frame, *dst, *a, *b) {
                return ctx.take_pending_thrown().map(Some).ok_or(e);
            }
        }
        Instruction::Sub { dst, a, b } => exec_value::sub(frame, *dst, *a, *b)?,
        Instruction::Mul { dst, a, b } => exec_value::mul(frame, *dst, *a, *b)?,
        Instruction::Div { dst, a, b } => {
            // fix-int-div-by-zero-panic : div/rem now return
            // Option<Value> so int-by-zero can surface as a catchable
            // Std.DivideByZeroException instead of panicking.
            if let Some(thrown) = exec_value::div(ctx, module, frame, *dst, *a, *b)? {
                return Ok(Some(thrown));
            }
        }
        Instruction::Rem { dst, a, b } => {
            if let Some(thrown) = exec_value::rem(ctx, module, frame, *dst, *a, *b)? {
                return Ok(Some(thrown));
            }
        }

        // ── Comparison ───────────────────────────────────────────────────────
        Instruction::Eq { dst, a, b } => exec_value::eq(frame, *dst, *a, *b)?,
        Instruction::Ne { dst, a, b } => exec_value::ne(frame, *dst, *a, *b)?,
        Instruction::Lt { dst, a, b } => exec_value::lt(frame, *dst, *a, *b)?,
        Instruction::Le { dst, a, b } => exec_value::le(frame, *dst, *a, *b)?,
        Instruction::Gt { dst, a, b } => exec_value::gt(frame, *dst, *a, *b)?,
        Instruction::Ge { dst, a, b } => exec_value::ge(frame, *dst, *a, *b)?,

        // ── Logical ──────────────────────────────────────────────────────────
        Instruction::And { dst, a, b } => exec_value::and(frame, *dst, *a, *b)?,
        Instruction::Or  { dst, a, b } => exec_value::or(frame, *dst, *a, *b)?,
        Instruction::Not { dst, src }  => exec_value::not(frame, *dst, *src)?,

        // ── Unary ────────────────────────────────────────────────────────────
        Instruction::Neg { dst, src } => exec_value::neg(frame, *dst, *src)?,

        // ── Bitwise ──────────────────────────────────────────────────────────
        Instruction::BitAnd { dst, a, b } => exec_value::bit_and(frame, *dst, *a, *b)?,
        Instruction::BitOr  { dst, a, b } => exec_value::bit_or(frame, *dst, *a, *b)?,
        Instruction::BitXor { dst, a, b } => exec_value::bit_xor(frame, *dst, *a, *b)?,
        Instruction::BitNot { dst, src }  => exec_value::bit_not(frame, *dst, *src)?,
        Instruction::Shl    { dst, a, b } => exec_value::shl(frame, *dst, *a, *b)?,
        Instruction::Shr    { dst, a, b } => exec_value::shr(frame, *dst, *a, *b)?,

        // ── String formation ─────────────────────────────────────────────────
        Instruction::StrConcat { dst, a, b } => exec_value::str_concat(ctx, frame, *dst, *a, *b)?,
        Instruction::ToStr     { dst, src }  => {
            if let Err(e) = exec_value::to_str(ctx, module, frame, *dst, *src) {
                return ctx.take_pending_thrown().map(Some).ok_or(e);
            }
        }

        // ── Address-load (spec impl-ref-out-in-runtime) ─────────────────────
        Instruction::LoadLocalAddr { dst, slot } => exec_address::load_local_addr(ctx, frame, *dst, *slot),
        Instruction::LoadElemAddr { dst, arr, idx } => {
            if let Some(thrown) = exec_address::load_elem_addr(ctx, module, frame, *dst, *arr, *idx)? {
                return Ok(Some(thrown));
            }
        }
        Instruction::LoadFieldAddr(insn) => {
            if let Some(thrown) = exec_address::load_field_addr(ctx, module, frame, insn.dst, insn.obj, &insn.field_name)? {
                return Ok(Some(thrown));
            }
        }

        // ── Generic default(T) at runtime (D-8b-3 Phase 2) ──────────────────
        Instruction::DefaultOf { dst, param_index } => exec_address::default_of(frame, *dst, *param_index),

        // ── Method-level generics (add-generic-methods) ─────────────────────
        Instruction::MethodTypeArg { dst, param_index } => exec_address::method_type_arg(ctx, frame, *dst, *param_index),
        Instruction::MethodDefault { dst, param_index } => exec_address::method_default(frame, *dst, *param_index),

        // ── Numeric cast (fix-numeric-cast-lowering) ────────────
        Instruction::Convert { dst, src, to_tag } => {
            // make-hard-cast-fail-properly：硬转换失败 → 可 catch 的真异常（走 Ok(Some) 通道）。
            if let Some(thrown) = exec_value::convert(ctx, module, frame, *dst, *src, *to_tag)? {
                return Ok(Some(thrown));
            }
        }

        // ── Calls ────────────────────────────────────────────────────────────
        Instruction::Call(insn) => {
            let CallInsn { dst, func: fname, args, method_type_args } = &**insn;
            let _site_idx = site_idx!();
            // exception-stack-trace: stamp current site's code
            // offset on this frame's VmFrame before descending into the
            // callee, so a downstream `throw` snapshot shows our call site.
            stamp_call_site(ctx, func, block_idx, instr_idx);

            // Hot path: the site's `FnId` token (P1-2) names the callee directly —
            // merged and lazily loaded alike; UNRESOLVED binds by name on first dispatch.
            let method_token = cached_token!(_site_idx, method_tokens);
            if let Some(thrown) = exec_call::call(ctx, module, frame, *dst, fname, args, method_token, method_type_args)? {
                return Ok(Some(thrown));
            }
            // add-gc-safepoint : post-Call safepoint — long-running
            // callees pop their FrameGuard before returning here, so checking
            // upon resumption catches GC requests that arrived while we were
            // in the callee.
            crate::gc::safepoint::check_safepoint(ctx);
        }
        Instruction::Builtin(insn) => {
            let BuiltinInsn { dst, name, args } = &**insn;
            let _site_idx = site_idx!();
            // Hot path: resolver populates Function.resolved.builtin_tokens
            // with BuiltinId per site at load time (closed set, all hits).
            // Fallback to name lookup when resolver hasn't run.
            let builtin_id = cached_token!(_site_idx, builtin_tokens).copied();
            // make-corelib-errors-catchable : builtin errors now
            // surface as catchable `Std.Exception` instances (see exec_call::builtin).
            if let Some(thrown) = exec_call::builtin(ctx, module, frame, *dst, name, args, builtin_id)? {
                return Ok(Some(thrown));
            }
        }
        Instruction::LoadFn(insn) => {
            let _site_idx = site_idx!();
            // P1-2 PR 8: the site's target `FnId` — bound once (first execution or load-time
            // pre-fill), afterwards one load.
            let token = cached_token!(_site_idx, fn_ref_tokens);
            if let Some(thrown) = exec_call::load_fn(ctx, module, frame, insn.dst, &insn.func, token) {
                return Ok(Some(thrown));
            }
        }
        Instruction::CallIndirect { dst, callee, args } => {
            stamp_call_site(ctx, func, block_idx, instr_idx);
            if let Some(thrown) = exec_call::call_indirect(ctx, module, frame, *dst, *callee, args)? {
                return Ok(Some(thrown));
            }
            crate::gc::safepoint::check_safepoint(ctx);
        }
        Instruction::MkClos(insn) => {
            let MkClosInsn { dst, fn_name, captures } = &**insn;
            let _site_idx = site_idx!();
            let token = cached_token!(_site_idx, fn_ref_tokens);
            if let Some(thrown) = exec_call::mk_clos(ctx, module, frame, *dst, fn_name, captures, token)? {
                return Ok(Some(thrown));
            }
        }

        // ── Arrays ───────────────────────────────────────────────────────────
        Instruction::ArrayNew(insn) => {
            if let Some(thrown) = exec_array::array_new(ctx, module, frame, insn.dst, insn.size, insn.elem_tag, insn.element_type, insn.stack_alloc, insn.type_param_kind, insn.type_param_index)? {
                return Ok(Some(thrown));
            }
        }
        Instruction::ArrayNewLit(insn) => {
            if let Some(thrown) = exec_array::array_new_lit(ctx, module, frame, insn.dst, &insn.elems, insn.element_type, insn.stack_alloc)? {
                return Ok(Some(thrown));
            }
        }
        Instruction::ArrayGet { dst, arr, idx } => {
            if let Some(thrown) = exec_array::array_get(ctx, module, frame, *dst, *arr, *idx)? {
                return Ok(Some(thrown));
            }
        }
        Instruction::ArraySet { arr, idx, val } => {
            if let Some(thrown) = exec_array::array_set(ctx, module, frame, *arr, *idx, *val)? {
                return Ok(Some(thrown));
            }
        }
        Instruction::ArrayLen { dst, arr } => {
            if let Some(thrown) = exec_array::array_len(ctx, module, frame, *dst, *arr)? {
                return Ok(Some(thrown));
            }
        }

        // ── Objects ──────────────────────────────────────────────────────────
        Instruction::ObjNew(insn) => {
            // add-escape-analysis-stack-alloc: stack_alloc forwarded to obj_new,
            // which allocates in the per-context arena when set (+ runtime-enabled).
            let ObjNewInsn { dst, class_name, ctor_name, args, type_args, stack_alloc, ctor_known } = &**insn;
            let _site_idx = site_idx!();
            // P1-2 PR 5: the site cache (class descriptor + ctor FnId) — hit = no hashing.
            let site = cached_token!(_site_idx, obj_new);
            // fix-ctor-throw-propagation : mirror Call / Builtin —
            // propagate user `throw` from the ctor body to the enclosing
            // try/catch instead of silently dropping it.
            if let Some(thrown) = exec_object::obj_new(
                ctx, module, frame, *dst, class_name, ctor_name, args, *type_args, site,
                *stack_alloc, *ctor_known,
            )? {
                return Ok(Some(thrown));
            }
        }
        Instruction::Typeof(insn) => exec_address::type_of(ctx, frame, insn),
        Instruction::FieldGet(insn) => {
            let FieldGetInsn { dst, obj, field_name } = &**insn;
            let _site_idx = site_idx!();
            let field_ic = cached_token!(_site_idx, field_ic);
            if let Some(thrown) = exec_object::field_get(ctx, module, frame, *dst, *obj, field_name, field_ic)? {
                return Ok(Some(thrown));
            }
        }
        Instruction::FieldSet(insn) => {
            let FieldSetInsn { obj, field_name, val } = &**insn;
            let _site_idx = site_idx!();
            let field_ic = cached_token!(_site_idx, field_ic);
            if let Some(thrown) = exec_object::field_set(ctx, module, frame, *obj, field_name, *val, field_ic)? {
                return Ok(Some(thrown));
            }
        }
        Instruction::VCall(insn) => {
            let VCallInsn { dst, obj, method, args, method_type_args } = &**insn;
            let _site_idx = site_idx!();
            stamp_call_site(ctx, func, block_idx, instr_idx);
            // Hot path: monomorphic inline cache fires when receiver TypeId
            // matches the cached one at this site (same site + same recv type).
            // Polymorphic sites overwrite the slot each time (Phase 1 mono IC).
            let vcall_ic = cached_token!(_site_idx, vcall_ic);
            if let Some(thrown) = exec_vcall::vcall(ctx, module, frame, *dst, *obj, method, args, vcall_ic, method_type_args)? {
                return Ok(Some(thrown));
            }
        }
        Instruction::IsInstance(insn) => exec_object::is_instance(ctx, module, frame, insn.dst, insn.obj, &insn.class_name, &insn.target)?,
        Instruction::AsCast(insn) => exec_object::as_cast(ctx, module, frame, insn.dst, insn.obj, &insn.class_name, &insn.target)?,
        Instruction::StaticGet(insn) => {
            let StaticGetInsn { dst, field } = &**insn;
            let _site_idx = site_idx!();
            // Hot path: pre-resolved StaticFieldId → direct Vec index.
            use std::sync::atomic::Ordering;
            let field_id = cached_token!(_site_idx, static_field_tokens)
                .map(|atom| atom.load(Ordering::Relaxed))
                .filter(|&id| id != UNRESOLVED);
            if let Some(thrown) = exec_object::static_get(ctx, module, frame, *dst, field, field_id)? {
                return Ok(Some(thrown));
            }
        }
        Instruction::StaticSet(insn) => {
            let StaticSetInsn { field, val } = &**insn;
            let _site_idx = site_idx!();
            use std::sync::atomic::Ordering;
            let field_id = cached_token!(_site_idx, static_field_tokens)
                .map(|atom| atom.load(Ordering::Relaxed))
                .filter(|&id| id != UNRESOLVED);
            if let Some(thrown) = exec_object::static_set(ctx, module, frame, field, *val, field_id)? {
                return Ok(Some(thrown));
            }
        }

        // ── Native interop ───────────────────────────────────────────────────
        // add-platform-wasm Stage 0: feature `native-interop`
        // gates all four opcodes. wasm builds bail with a clear message
        // (these opcodes shouldn't appear in a wasm-targeted .zbc anyway,
        // but malformed input shouldn't UAF the interp either).
        #[cfg(feature = "native-interop")]
        Instruction::CallNative(insn) => {
            let CallNativeInsn { dst, module: m, type_name, symbol, args } = &**insn;
            // retire-z-codes: marshal failures throw
            // Std.InvalidMarshalException via Ok(Some(exc)).
            if let Some(thrown) = exec_ffi::ffi_call(ctx, module, frame, *dst, m, type_name, symbol, args)? {
                return Ok(Some(thrown));
            }
        }
        #[cfg(feature = "native-interop")]
        Instruction::CallNativeVtable { vtable_slot, .. } => exec_ffi::ffi_call_vtable(*vtable_slot)?,
        #[cfg(feature = "native-interop")]
        Instruction::PinPtr   { dst, src }   => {
            if let Some(thrown) = exec_ffi::pin_ptr(ctx, module, frame, *dst, *src)? {
                return Ok(Some(thrown));
            }
        }
        #[cfg(feature = "native-interop")]
        Instruction::UnpinPtr { pinned }     => exec_ffi::unpin_ptr(ctx, frame, *pinned)?,
        #[cfg(not(feature = "native-interop"))]
        Instruction::CallNative(_)
        | Instruction::CallNativeVtable { .. }
        | Instruction::PinPtr { .. }
        | Instruction::UnpinPtr { .. } => {
            anyhow::bail!(
                "native interop opcode encountered in a build with `native-interop` feature disabled"
            );
        }
        // add-struct-value-semantics Phase A: blob value type instructions,
        // executed on the per-context byte arena. z42c does not emit these until
        // A-use (2c); until then they only appear in Rust unit tests.
        Instruction::StructAlloc(insn) =>
            exec_struct::struct_alloc(ctx, frame, insn.dst, &insn.type_name, insn.size)?,
        Instruction::StructCopy { dst, src, size } =>
            exec_struct::struct_copy(ctx, frame, *dst, *src, *size)?,
        Instruction::StructFieldGetPrim(insn) =>
            exec_struct::struct_field_get_prim(ctx, frame, insn)?,
        Instruction::StructFieldSetPrim(insn) =>
            exec_struct::struct_field_set_prim(ctx, frame, insn)?,
    }
    Ok(None)
}

/// exception-stack-trace: stamp the code offset of a call-class
/// instruction onto the executing frame's `VmFrame::pc` so a downstream
/// `throw` can show the call site (not 0). O(1) — the line / column are
/// resolved from the offset only when a stack trace is built.
#[inline]
fn stamp_call_site(ctx: &VmContext, func: &Function, block_idx: usize, instr_idx: usize) {
    ctx.set_top_frame_pc(func.linear_offset(block_idx as u32, instr_idx as u32));
}
