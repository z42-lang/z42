#![allow(dangerous_implicit_autorefs)]
//! Arithmetic, comparison, logical, unary, and bitwise helpers.

use crate::corelib::convert::with_value_str;
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

/// dispatch-tostring-in-native-stringify: JIT 侧的字符串化 —— 对象/装箱 struct 走
/// `with_stringify_dispatch`（派发用户 `ToString`），其余走 `with_value_str`。
/// 与 interp `exec_value::add` 的 `with_obj_str` 一一对应。perf-str-concat-direct：文本以
/// `&str` 借给 `f`（标量在栈缓冲里格式化、用户 `ToString` 的 GC 串原样借用），不落地中间串。
#[inline]
unsafe fn jit_with_str<R>(ctx: *const JitModuleCtx, v: &Value, f: impl FnOnce(&str) -> R) -> anyhow::Result<R> {
    match v {
        Value::Object(_) | Value::BoxedStruct(_) =>
            crate::interp::dispatch::with_stringify_dispatch(vm_ctx_ref(ctx), v, f),
        other => Ok(with_value_str(other, f)),
    }
}

/// Raise a stringification failure: the user `ToString`'s own exception when it
/// threw (`dispatch::tostring_threw` parks it in `pending_thrown`), otherwise
/// the error text. Returns the "thrown" sentinel.
pub(crate) unsafe fn stringify_failed(ctx: *const JitModuleCtx, e: anyhow::Error) -> u8 {
    let vm = vm_ctx_ref(ctx);
    let exc = vm.take_pending_thrown().unwrap_or_else(|| Value::Str(e.to_string().into()));
    set_exception(vm, exc);
    1
}

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
    // Build the result under a scoped borrow — no operand clones. String concat
    // (common in z42c name mangling / message building) allocates the result as ONE
    // fused GC block (`alloc_str_concat2`), same as the interp `Add` / `StrConcat`.
    let result = {
        let va = &regs[a as usize];
        let vb = &regs[b as usize];
        let heap = vm_ctx_ref(ctx).heap();
        match (va, vb) {
            (Value::Str(sa), Value::Str(sb)) => Value::Str(heap.alloc_str_concat2(sa, sb)),
            // dispatch-tostring-in-native-stringify: interp `exec_value::add` 混合臂的 JIT 对称件
            // —— 对象/装箱 struct 操作数派发用户 `ToString`（只补一侧的话热代码与解释器不一致）。
            // perf-str-concat-direct: 非串操作数直接格式化进结果块，不落地中间串。
            (Value::Str(sa), vb) => {
                let sa = *sa;
                match jit_with_str(ctx, vb, |t| heap.alloc_str_concat2(&sa, t)) {
                    Ok(s)  => Value::Str(s),
                    Err(e) => return stringify_failed(ctx, e),
                }
            }
            (va, Value::Str(sb)) => {
                let sb = *sb;
                match jit_with_str(ctx, va, |t| heap.alloc_str_concat2(t, &sb)) {
                    Ok(s)  => Value::Str(s),
                    Err(e) => return stringify_failed(ctx, e),
                }
            }
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
        (*frame).regs[dst as usize] = Value::I64(semantics::int_div(*x, *y));
        return 0;
    }
    let va = regs[a as usize].clone();
    let vb = regs[b as usize].clone();
    if semantics::is_int_div_by_zero(&vb) {
        return throw_int_div_by_zero(ctx, "/");
    }
    match semantics::int_binop(&va, &vb, semantics::int_div, |x, y| x / y) {
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
        (*frame).regs[dst as usize] = Value::I64(semantics::int_rem(*x, *y));
        return 0;
    }
    let va = regs[a as usize].clone();
    let vb = regs[b as usize].clone();
    if semantics::is_int_div_by_zero(&vb) {
        return throw_int_div_by_zero(ctx, "%");
    }
    match semantics::int_binop(&va, &vb, semantics::int_rem, |x, y| x % y) {
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
    // operand clones (`numeric_eq` takes refs). The previous
    // version cloned both operands on the non-I64 path (string / object
    // equality, very common in compiler token/name comparison).
    //
    // fix-mixed-numeric-equality: 必须走 `semantics::numeric_eq`，不能直接用
    // `Value: PartialEq` —— 后者没有混合数值臂，`int == double` / `char == int` 会恒假。
    // 这条 helper 正是 JIT 处理混合操作数的**唯一**落点（`is_int_cmp` / `is_f64_cmp`
    // 只在两侧静态同类时内联），所以漏掉它就等于 JIT 下 bug 依旧。
    let regs = &(*frame).regs;
    let result = semantics::numeric_eq(&regs[a as usize], &regs[b as usize]);
    (*frame).regs[dst as usize] = Value::Bool(result);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_ne(
    frame: *mut JitFrame, _ctx: *const JitModuleCtx,
    dst: u32, a: u32, b: u32,
) {
    // Clone-free: compare by reference under a scoped borrow (see jit_eq).
    // `!numeric_eq(..)` 而非 `!=`：与 `semantics::eval_cmp` 的 `Ne` 同一套（含加宽 +
    // NaN unordered），保证 interp / JIT 内联 / JIT helper 三路口径一致。
    let regs = &(*frame).regs;
    let result = !semantics::numeric_eq(&regs[a as usize], &regs[b as usize]);
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
