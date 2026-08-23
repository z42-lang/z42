#![allow(dangerous_implicit_autorefs)]
//! Arithmetic, comparison, logical, unary, and bitwise helpers.

use crate::corelib::convert::value_to_str;
use crate::metadata::Value;
// converge-vm-arith-semantics (H3): scalar rules come from the single source of
// truth `crate::semantics` (shared with interp), not a JIT-local copy.
use crate::semantics;
use super::super::frame::{JitFrame, JitModuleCtx};
use super::{set_exception, vm_ctx_ref};

// ── Arithmetic ───────────────────────────────────────────────────────────────

// 2026-04-28 vm-wrapping-int-arith: Add/Sub/Mul 用 wrapping，与 interp 对齐 +
// C# unchecked / Java int / Rust release default 一致。Div/Rem 不变（panic
// on /0 是不同语义）。

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_add(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, a: u32, b: u32,
) -> u8 {
    // Fast path: both I64
    let regs = &(*frame).regs;
    if let (Value::I64(x), Value::I64(y)) = (&regs[a as usize], &regs[b as usize]) {
        (*frame).regs[dst as usize] = Value::I64(x.wrapping_add(*y));
        return 0;
    }
    // Build the owned result under a scoped borrow — no operand clones. The
    // string-concat path (common in z42c name mangling / message building)
    // previously cloned both operands just to drop the regs borrow before the
    // write; `format!` already produces a fresh owned String.
    let result = {
        let va = &regs[a as usize];
        let vb = &regs[b as usize];
        match (va, vb) {
            (Value::Str(sa), Value::Str(sb)) => Value::Str(format!("{}{}", sa, sb).into()),
            (Value::Str(sa), vb) => Value::Str(format!("{}{}", sa, value_to_str(vb)).into()),
            (va, Value::Str(sb)) => Value::Str(format!("{}{}", value_to_str(va), sb).into()),
            _ => match semantics::int_binop(va, vb, i64::wrapping_add, |x, y| x + y) {
                Ok(r)  => r,
                Err(e) => { set_exception(vm_ctx_ref(ctx), Value::Str(e.to_string().into())); return 1; }
            }
        }
    };
    (*frame).regs[dst as usize] = result;
    0
}

macro_rules! arith_op {
    ($name:ident, $int_op:expr, $float_op:expr) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(
            frame: *mut JitFrame, ctx: *const JitModuleCtx,
            dst: u32, a: u32, b: u32,
        ) -> u8 {
            // Fast path: both I64 — no clone, no match dispatch
            let regs = &(*frame).regs;
            if let (Value::I64(x), Value::I64(y)) = (&regs[a as usize], &regs[b as usize]) {
                let int_op: fn(i64, i64) -> i64 = $int_op;
                (*frame).regs[dst as usize] = Value::I64(int_op(*x, *y));
                return 0;
            }
            let va = regs[a as usize].clone();
            let vb = regs[b as usize].clone();
            match semantics::int_binop(&va, &vb, $int_op, $float_op) {
                Ok(r)  => { (*frame).regs[dst as usize] = r; 0 }
                Err(e) => { set_exception(vm_ctx_ref(ctx), Value::Str(e.to_string().into())); 1 }
            }
        }
    };
}

arith_op!(jit_sub, i64::wrapping_sub, |x, y| x - y);
arith_op!(jit_mul, i64::wrapping_mul, |x, y| x * y);

// Div / Rem: integer divide-by-zero must throw `Std.DivideByZeroException`
// (catchable) rather than panic the VM via Rust's `x / 0` (which traps
// SIGFPE on x86_64 in release / panics in debug). fix-jit-int-div-by-zero
// (2026-05-30): pre-fix the helper macro called `int_op(x, y)` directly
// in the I64 fast path; for y == 0 that panicked the VM, diverging from
// interp behavior. Now matches `interp::exec_value::check_int_div_by_zero`.
//
// F64 / 0 → IEEE 754 Infinity (existing behavior preserved via the slow
// path's `float_op`); only integer y == 0 hits the throw.

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_div(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, a: u32, b: u32,
) -> u8 {
    let regs = &(*frame).regs;
    if let (Value::I64(x), Value::I64(y)) = (&regs[a as usize], &regs[b as usize]) {
        if *y == 0 {
            return throw_int_div_by_zero(ctx, "/");
        }
        (*frame).regs[dst as usize] = Value::I64(x / y);
        return 0;
    }
    let va = regs[a as usize].clone();
    let vb = regs[b as usize].clone();
    if semantics::is_int_div_by_zero(&vb) {
        return throw_int_div_by_zero(ctx, "/");
    }
    match semantics::int_binop(&va, &vb, |x, y| x / y, |x, y| x / y) {
        Ok(r)  => { (*frame).regs[dst as usize] = r; 0 }
        Err(e) => { set_exception(vm_ctx_ref(ctx), Value::Str(e.to_string().into())); 1 }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_rem(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, a: u32, b: u32,
) -> u8 {
    let regs = &(*frame).regs;
    if let (Value::I64(x), Value::I64(y)) = (&regs[a as usize], &regs[b as usize]) {
        if *y == 0 {
            return throw_int_div_by_zero(ctx, "%");
        }
        (*frame).regs[dst as usize] = Value::I64(x % y);
        return 0;
    }
    let va = regs[a as usize].clone();
    let vb = regs[b as usize].clone();
    if semantics::is_int_div_by_zero(&vb) {
        return throw_int_div_by_zero(ctx, "%");
    }
    match semantics::int_binop(&va, &vb, |x, y| x % y, |x, y| x % y) {
        Ok(r)  => { (*frame).regs[dst as usize] = r; 0 }
        Err(e) => { set_exception(vm_ctx_ref(ctx), Value::Str(e.to_string().into())); 1 }
    }
}

/// Stamp `Std.DivideByZeroException` via `make_stdlib_exception` then
/// `set_exception`. Matches interp's `check_int_div_by_zero` flow.
/// Returns 1 (the "thrown" sentinel) so the caller can `return` it
/// directly.
unsafe fn throw_int_div_by_zero(ctx: *const JitModuleCtx, op: &str) -> u8 {
    let vm_ctx = vm_ctx_ref(ctx);
    let module = &*(*ctx).module;
    let exc = crate::exception::make_stdlib_exception(
        vm_ctx, module, semantics::DIV_BY_ZERO_EXC,
        semantics::div_by_zero_msg(op),
    ).unwrap_or_else(|e| Value::Str(format!("{e}").into()));
    set_exception(vm_ctx, exc);
    1
}

// ── Comparison ───────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_eq(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    dst: u32, a: u32, b: u32,
) {
    // Compute the result under a scoped immutable borrow, then write — no
    // operand clones (`Value: PartialEq` compares by reference). The previous
    // version cloned both operands on the non-I64 path (string / object
    // equality, very common in compiler token/name comparison).
    let regs = &(*frame).regs;
    let result = regs[a as usize] == regs[b as usize];
    (*frame).regs[dst as usize] = Value::Bool(result);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_ne(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    dst: u32, a: u32, b: u32,
) {
    // Clone-free: compare by reference under a scoped borrow (see jit_eq).
    let regs = &(*frame).regs;
    let result = regs[a as usize] != regs[b as usize];
    (*frame).regs[dst as usize] = Value::Bool(result);
}

macro_rules! cmp_op {
    ($name:ident, $i64_op:expr, $lt_swap:expr, $negate:expr) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(
            frame: *mut JitFrame, ctx: *const JitModuleCtx,
            dst: u32, a: u32, b: u32,
        ) -> u8 {
            let regs = &(*frame).regs;
            // Fast path: both I64
            if let (Value::I64(x), Value::I64(y)) = (&regs[a as usize], &regs[b as usize]) {
                let cmp: fn(&i64, &i64) -> bool = $i64_op;
                (*frame).regs[dst as usize] = Value::Bool(cmp(x, y));
                return 0;
            }
            let (va, vb) = if $lt_swap {
                (regs[b as usize].clone(), regs[a as usize].clone())
            } else {
                (regs[a as usize].clone(), regs[b as usize].clone())
            };
            match semantics::numeric_lt(&va, &vb) {
                Ok(r)  => { (*frame).regs[dst as usize] = Value::Bool(if $negate { !r } else { r }); 0 }
                Err(e) => { set_exception(vm_ctx_ref(ctx), Value::Str(e.to_string().into())); 1 }
            }
        }
    };
}

cmp_op!(jit_lt, |x: &i64, y: &i64| x < y,  false, false);
cmp_op!(jit_le, |x: &i64, y: &i64| x <= y, true,  true);
cmp_op!(jit_gt, |x: &i64, y: &i64| x > y,  true,  false);
cmp_op!(jit_ge, |x: &i64, y: &i64| x >= y, false, true);

// ── Logical ──────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_and(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, a: u32, b: u32,
) -> u8 {
    match (&(*frame).regs[a as usize], &(*frame).regs[b as usize]) {
        (Value::Bool(va), Value::Bool(vb)) => { (*frame).regs[dst as usize] = Value::Bool(*va && *vb); 0 }
        (va, vb) => {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("And: expected bool, got {:?} and {:?}", va, vb).into()));
            1
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_or(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, a: u32, b: u32,
) -> u8 {
    match (&(*frame).regs[a as usize], &(*frame).regs[b as usize]) {
        (Value::Bool(va), Value::Bool(vb)) => { (*frame).regs[dst as usize] = Value::Bool(*va || *vb); 0 }
        (va, vb) => {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("Or: expected bool, got {:?} and {:?}", va, vb).into()));
            1
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_not(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, src: u32,
) -> u8 {
    match &(*frame).regs[src as usize] {
        Value::Bool(v) => { let b = *v; (*frame).regs[dst as usize] = Value::Bool(!b); 0 }
        other => {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("Not: expected bool, got {:?}", other).into()));
            1
        }
    }
}

// ── Unary arithmetic ─────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_neg(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, src: u32,
) -> u8 {
    let result = match &(*frame).regs[src as usize] {
        Value::I64(n) => Value::I64(-n),
        Value::F64(f) => Value::F64(-f),
        other => {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("Neg: expected numeric, got {:?}", other).into()));
            return 1;
        }
    };
    (*frame).regs[dst as usize] = result;
    0
}

// ── Bitwise ──────────────────────────────────────────────────────────────────

macro_rules! bitwise_op {
    ($name:ident, $op:expr) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(
            frame: *mut JitFrame, ctx: *const JitModuleCtx,
            dst: u32, a: u32, b: u32,
        ) -> u8 {
            let va = (*frame).regs[a as usize].clone();
            let vb = (*frame).regs[b as usize].clone();
            match semantics::int_bitop(&va, &vb, $op) {
                Ok(r)  => { (*frame).regs[dst as usize] = r; 0 }
                Err(e) => { set_exception(vm_ctx_ref(ctx), Value::Str(e.to_string().into())); 1 }
            }
        }
    };
}

bitwise_op!(jit_bit_and, |x, y| x & y);
bitwise_op!(jit_bit_or,  |x, y| x | y);
bitwise_op!(jit_bit_xor, |x, y| x ^ y);
bitwise_op!(jit_shl,     |x, y| x << (y & 63));
bitwise_op!(jit_shr,     |x, y| x >> (y & 63));

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_bit_not(
    frame: *mut JitFrame, ctx: *const JitModuleCtx,
    dst: u32, src: u32,
) -> u8 {
    let result = match &(*frame).regs[src as usize] {
        Value::I64(n) => Value::I64(!n),
        other => {
            set_exception(vm_ctx_ref(ctx), Value::Str(format!("BitNot: expected integral, got {:?}", other).into()));
            return 1;
        }
    };
    (*frame).regs[dst as usize] = result;
    0
}
