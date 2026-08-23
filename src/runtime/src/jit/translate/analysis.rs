//! Per-function prescans: 2B cache participation + 2C promotable-reg residency.
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).

use super::*;

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
pub(super) fn instr_uses_int_cache(func: &Function, instr: &Instruction) -> bool {
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
pub(super) fn compute_promotable_regs(func: &Function, enable: bool) -> Vec<bool> {
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
                | I::ConstNull { dst } | I::DefaultOf { dst, .. }
                | I::MethodTypeArg { dst, .. } | I::MethodDefault { dst, .. } => disq.push(*dst),
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

// ═════════════════════════════════════════════════════════════════════════════
// max_reg — largest register index used in a function
// ═════════════════════════════════════════════════════════════════════════════

/// The highest register **index** used by `func` — the JIT pre-sizes its
/// register file to this + 1. Thin wrapper over `Function::reg_file_len` (the
/// COUNT), which is the single source of truth for frame sizing shared with the
/// interp frame pre-sizing (folds params / every `dst` / exception-table catch
/// registers). `reg_file_len` is always ≥ 1, so the subtraction never underflows.
pub(crate) fn max_reg(func: &Function) -> usize {
    func.reg_file_len() as usize - 1
}
