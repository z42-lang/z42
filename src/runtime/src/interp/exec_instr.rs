/// Single-instruction dispatch for the interpreter.
///
/// This file is a thin dispatcher: each `Instruction` variant matches one arm
/// that delegates to a per-category helper (see sibling `exec_*.rs` modules).
/// The match is **exhaustive** ([runtime-rust.md](../../../../.claude/rules/runtime-rust.md)
/// "不允许有 `_` 通配兜底"); adding a new `Instruction` variant produces a
/// compile error here, forcing the matching helper / category decision.
///
/// Helpers that may propagate a callee user exception return
/// `Result<Option<Value>>` and the dispatcher checks `is_some()` to forward
/// the throw upstack. All other helpers return `Result<()>`.

use crate::metadata::{Function, Instruction, Module, Value};
use crate::metadata::{
    BuiltinInsn, CallInsn, CallNativeInsn, FieldGetInsn, FieldSetInsn, MkClosInsn, ObjNewInsn,
    StaticGetInsn, StaticSetInsn, VCallInsn,
};
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
/// `func` / `block_idx` / `instr_idx` are passed through for the
/// introduce-method-token Phase 4 dispatch fast path. Token-bearing
/// helpers (Call / Builtin / ObjNew / VCall / FieldGet / FieldSet /
/// StaticGet / StaticSet) read `func.resolved.site_index[block_idx]
/// [instr_idx]` to find their per-kind cache slot. Non-token-bearing
/// instructions ignore these parameters.
pub fn exec_instr(
    ctx: &VmContext, module: &Module, frame: &mut Frame,
    func: &Function, block_idx: usize, instr_idx: usize,
    instr: &Instruction,
) -> Result<Option<Value>> {
    use super::{exec_address, exec_array, exec_call, exec_object, exec_struct, exec_value, exec_vcall};
    #[cfg(feature = "native-interop")]
    use super::exec_native;

    // `resolved` is None only if Vm::run hasn't been called yet (e.g. unit tests
    // calling exec_function directly without resolver hookup) — helpers fall back
    // to string lookup in that case. Cheap: one OnceLock atomic load.
    let resolved = func.resolved.get();
    // S1 (perf-interp-hot-paths): the per-site index is a nested `Vec<Vec<u32>>`
    // lookup (two pointer chases + two bounds checks). Only the 8 token-bearing
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
        Instruction::Add { dst, a, b } => exec_value::add(ctx, frame, *dst, *a, *b)?,
        Instruction::Sub { dst, a, b } => exec_value::sub(frame, *dst, *a, *b)?,
        Instruction::Mul { dst, a, b } => exec_value::mul(frame, *dst, *a, *b)?,
        Instruction::Div { dst, a, b } => {
            // fix-int-div-by-zero-panic (2026-05-25): div/rem now return
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
        Instruction::ToStr     { dst, src }  => exec_value::to_str(ctx, module, frame, *dst, *src)?,

        // ── Address-load (spec impl-ref-out-in-runtime) ─────────────────────
        Instruction::LoadLocalAddr { dst, slot } => exec_address::load_local_addr(ctx, frame, *dst, *slot),
        Instruction::LoadElemAddr  { dst, arr, idx } => exec_address::load_elem_addr(ctx, frame, *dst, *arr, *idx)?,
        Instruction::LoadFieldAddr(insn) => exec_address::load_field_addr(ctx, frame, insn.dst, insn.obj, &insn.field_name)?,

        // ── Generic default(T) at runtime (D-8b-3 Phase 2) ──────────────────
        Instruction::DefaultOf { dst, param_index } => exec_address::default_of(frame, *dst, *param_index),

        // ── Method-level generics (add-generic-methods) ─────────────────────
        Instruction::MethodTypeArg { dst, param_index } => exec_address::method_type_arg(ctx, frame, *dst, *param_index),
        Instruction::MethodDefault { dst, param_index } => exec_address::method_default(frame, *dst, *param_index),

        // ── Numeric cast (fix-numeric-cast-lowering, 2026-05-13) ────────────
        Instruction::Convert { dst, src, to_tag } => exec_value::convert(frame, *dst, *src, *to_tag)?,

        // ── Calls ────────────────────────────────────────────────────────────
        Instruction::Call(insn) => {
            let CallInsn { dst, func: fname, args, method_type_args } = &**insn;
            let _site_idx = site_idx!();
            // 2026-05-10 exception-stack-trace: stamp current site's source
            // line on this frame's FrameInfo before descending into the
            // callee, so a downstream `throw` snapshot shows our call site.
            update_caller_line(ctx, func, block_idx, instr_idx);

            // Hot path: pre-resolved MethodId direct-indexes module.functions.
            // Cross-zpkg cache (UNRESOLVED at load) backfills on first hit.
            let method_token = cached_token!(_site_idx, method_tokens);
            // review.md C7: per-site cross-zpkg target cache (parallel to
            // method_tokens). Borrowed on hit; backfilled on first cross-zpkg call.
            let cross_cell = cached_token!(_site_idx, cross_module_targets);
            if let Some(thrown) = exec_call::call(ctx, module, frame, *dst, fname, args, method_token, cross_cell, method_type_args)? {
                return Ok(Some(thrown));
            }
            // add-gc-safepoint (2026-05-20): post-Call safepoint — long-running
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
            // make-corelib-errors-catchable (2026-05-15): builtin errors now
            // surface as catchable `Std.Exception` instances (see exec_call::builtin).
            if let Some(thrown) = exec_call::builtin(ctx, module, frame, *dst, name, args, builtin_id)? {
                return Ok(Some(thrown));
            }
        }
        Instruction::LoadFn(insn) => exec_call::load_fn(frame, insn.dst, &insn.func),
        Instruction::LoadFnCached(insn) => exec_call::load_fn_cached(ctx, frame, insn.dst, &insn.func, insn.slot_id),
        Instruction::CallIndirect { dst, callee, args } => {
            update_caller_line(ctx, func, block_idx, instr_idx);
            if let Some(thrown) = exec_call::call_indirect(ctx, module, frame, *dst, *callee, args)? {
                return Ok(Some(thrown));
            }
            crate::gc::safepoint::check_safepoint(ctx);
        }
        Instruction::MkClos(insn) => {
            let MkClosInsn { dst, fn_name, captures, stack_alloc } = &**insn;
            if let Some(thrown) = exec_call::mk_clos(ctx, module, frame, *dst, fn_name, captures, *stack_alloc)? {
                return Ok(Some(thrown));
            }
        }

        // ── Arrays ───────────────────────────────────────────────────────────
        Instruction::ArrayNew(insn) => {
            if let Some(thrown) = exec_array::array_new(ctx, module, frame, insn.dst, insn.size, insn.elem_tag, &insn.element_type, insn.stack_alloc, insn.type_param_kind, insn.type_param_index)? {
                return Ok(Some(thrown));
            }
        }
        Instruction::ArrayNewLit(insn) => {
            if let Some(thrown) = exec_array::array_new_lit(ctx, module, frame, insn.dst, &insn.elems, &insn.element_type, insn.stack_alloc)? {
                return Ok(Some(thrown));
            }
        }
        Instruction::ArrayGet    { dst, arr, idx }  => exec_array::array_get(ctx, frame, *dst, *arr, *idx)?,
        Instruction::ArraySet    { arr, idx, val }  => exec_array::array_set(ctx, frame, *arr, *idx, *val)?,
        Instruction::ArrayLen    { dst, arr }       => exec_array::array_len(ctx, frame, *dst, *arr)?,

        // ── Objects ──────────────────────────────────────────────────────────
        Instruction::ObjNew(insn) => {
            // add-escape-analysis-stack-alloc: stack_alloc forwarded to obj_new,
            // which allocates in the per-context arena when set (+ runtime-enabled).
            let ObjNewInsn { dst, class_name, ctor_name, args, type_args, stack_alloc } = &**insn;
            let _site_idx = site_idx!();
            // Hot path: pass type_token cache for repopulation. Dispatch via
            // type_registry / lazy_loader unchanged.
            let type_token = cached_token!(_site_idx, type_tokens);
            // fix-ctor-throw-propagation (2026-05-24): mirror Call / Builtin —
            // propagate user `throw` from the ctor body to the enclosing
            // try/catch instead of silently dropping it.
            if let Some(thrown) = exec_object::obj_new(
                ctx, module, frame, *dst, class_name, ctor_name, args, type_args, type_token,
                *stack_alloc,
            )? {
                return Ok(Some(thrown));
            }
        }
        Instruction::Typeof(insn) => {
            // add-reflection-generic-type-definition: build a Std.Type from the
            // FQ type name + structured generic instantiation args.
            let v = crate::corelib::reflection::make_constructed_type(
                ctx, &insn.type_name, &insn.type_args,
            );
            frame.set(insn.dst, v);
        }
        Instruction::FieldGet(insn) => {
            let FieldGetInsn { dst, obj, field_name } = &**insn;
            let _site_idx = site_idx!();
            let field_ic = cached_token!(_site_idx, field_ic);
            exec_object::field_get(ctx, frame, *dst, *obj, field_name, field_ic)?;
        }
        Instruction::FieldSet(insn) => {
            let FieldSetInsn { obj, field_name, val } = &**insn;
            let _site_idx = site_idx!();
            let field_ic = cached_token!(_site_idx, field_ic);
            exec_object::field_set(ctx, frame, *obj, field_name, *val, field_ic)?;
        }
        Instruction::VCall(insn) => {
            let VCallInsn { dst, obj, method, args, method_type_args } = &**insn;
            let _site_idx = site_idx!();
            update_caller_line(ctx, func, block_idx, instr_idx);
            // Hot path: monomorphic inline cache fires when receiver TypeId
            // matches the cached one at this site (same site + same recv type).
            // Polymorphic sites overwrite the slot each time (Phase 1 mono IC).
            let vcall_ic = cached_token!(_site_idx, vcall_ic);
            if let Some(thrown) = exec_vcall::vcall(ctx, module, frame, *dst, *obj, method, args, vcall_ic, method_type_args)? {
                return Ok(Some(thrown));
            }
        }
        Instruction::IsInstance(insn) => exec_object::is_instance(ctx, module, frame, insn.dst, insn.obj, &insn.class_name)?,
        Instruction::AsCast(insn) => exec_object::as_cast(ctx, module, frame, insn.dst, insn.obj, &insn.class_name)?,
        Instruction::StaticGet(insn) => {
            let StaticGetInsn { dst, field } = &**insn;
            let _site_idx = site_idx!();
            // Hot path: pre-resolved StaticFieldId → direct Vec index.
            use std::sync::atomic::Ordering;
            let field_id = cached_token!(_site_idx, static_field_tokens)
                .map(|atom| atom.load(Ordering::Relaxed))
                .filter(|&id| id != UNRESOLVED);
            exec_object::static_get(ctx, frame, *dst, field, field_id);
        }
        Instruction::StaticSet(insn) => {
            let StaticSetInsn { field, val } = &**insn;
            let _site_idx = site_idx!();
            use std::sync::atomic::Ordering;
            let field_id = cached_token!(_site_idx, static_field_tokens)
                .map(|atom| atom.load(Ordering::Relaxed))
                .filter(|&id| id != UNRESOLVED);
            exec_object::static_set(ctx, frame, field, *val, field_id)?;
        }

        // ── Native interop ───────────────────────────────────────────────────
        // 2026-05-12 add-platform-wasm Stage 0: feature `native-interop`
        // gates all four opcodes. wasm builds bail with a clear message
        // (these opcodes shouldn't appear in a wasm-targeted .zbc anyway,
        // but malformed input shouldn't UAF the interp either).
        #[cfg(feature = "native-interop")]
        Instruction::CallNative(insn) => {
            let CallNativeInsn { dst, module: m, type_name, symbol, args } = &**insn;
            // 2026-05-11 retire-z-codes: marshal failures throw
            // Std.InvalidMarshalException via Ok(Some(exc)).
            if let Some(thrown) = exec_native::call_native(ctx, module, frame, *dst, m, type_name, symbol, args)? {
                return Ok(Some(thrown));
            }
        }
        #[cfg(feature = "native-interop")]
        Instruction::CallNativeVtable { vtable_slot, .. } => exec_native::call_native_vtable(*vtable_slot)?,
        #[cfg(feature = "native-interop")]
        Instruction::PinPtr   { dst, src }   => {
            if let Some(thrown) = exec_native::pin_ptr(ctx, module, frame, *dst, *src)? {
                return Ok(Some(thrown));
            }
        }
        #[cfg(feature = "native-interop")]
        Instruction::UnpinPtr { pinned }     => exec_native::unpin_ptr(ctx, frame, *pinned)?,
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
        Instruction::StructFieldGetPrim { dst, base, byte_off, kind } =>
            exec_struct::struct_field_get_prim(ctx, frame, *dst, *base, *byte_off, *kind)?,
        Instruction::StructFieldSetPrim { base, byte_off, kind, val } =>
            exec_struct::struct_field_set_prim(ctx, frame, *base, *byte_off, *kind, *val)?,
    }
    Ok(None)
}

/// 2026-05-10 exception-stack-trace: stamp the current source line of a
/// call-class instruction onto the executing frame's `FrameInfo` so a
/// downstream `throw` can format the call site (not 0). Cheap — one line
/// table linear scan + Cell::set.
#[inline]
fn update_caller_line(ctx: &VmContext, func: &Function, block_idx: usize, instr_idx: usize) {
    let (line, column) = super::resolve_line(func.line_table(), block_idx as u32, instr_idx as u32);
    // add-offline-symbolication: stamp line/col + linearized code offset in one
    // lock so a stripped-release trace (empty line table → line 0) still carries
    // an offline-resolvable `+0x<offset>` key at this call site (no extra lock).
    ctx.update_top_frame_pos(line, column, func.linear_offset(block_idx as u32, instr_idx as u32));
}
