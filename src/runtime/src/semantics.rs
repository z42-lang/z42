//! 语言标量语义的唯一真相源（single source of truth）。
//!
//! 同一算术 / 比较 / 数值转换语义，运行时有三条执行路径各自表述：
//!
//! 1. **interp 执行循环** —— `interp/ops.rs`（寄存器级包装）+ `interp/exec_value.rs`（指令处理）
//! 2. **JIT runtime helper** —— `jit/helpers/{arith,object}.rs` 的 `extern "C"` 函数
//! 3. **JIT 内联 Cranelift** —— `jit/translate/*` 的 `emit_*`，当 `reg_types` 证明操作数同型时
//!    绕过 helper 直发 Cranelift 原语
//!
//! 路径 1 与 2 **可在运行期共享 Rust 代码**：两者都调用本模块的值级函数（`int_binop` /
//! `numeric_lt` / `eval_cmp` / `convert_value`），不再各写一遍（此前 `jit/helpers/mod.rs`
//! 的 `int_binop_helper` / `numeric_lt_helper` 是 `interp/ops.rs` 的逐字副本，已删除）。
//!
//! 路径 3 **无法运行期调用 Rust**（它发机器码），只能以 `// SEMANTICS: semantics::<fn>`
//! 锚注释引用本模块对应函数，并由 `jit/translate/semantics_jit_diff_tests.rs` 的**差分测试**
//! 跑同一批边界输入断言 byte-identical，把「注释担保」升级为「测试担保」。
//!
//! ## 载重语义决策（改这里 = 改语言语义，三路必须同步）
//!
//! | 决策 | 规则 | 内联镜像（路径 3） |
//! |------|------|---------------------|
//! | 整数 add/sub/mul 溢出 | wrapping（同 Rust release / C# unchecked / Java int） | `emit_i64_binop`：`iadd`/`isub`/`imul` |
//! | 整数 div/rem 除零 | 抛 `Std.DivideByZeroException`（可 catch），非 panic / 非 Infinity | `emit_int_divrem`：冷路由 `b ∈ {0,-1}` 到 helper |
//! | float→int | 饱和 + NaN→0（Rust `as`）；`U64` 目标按 signed i64 饱和 | `emit_f64_to_int`：`fcvt_to_sint_sat` |
//! | int→float | 全 f64 精度（F32 目标也走 f64，无 f32 舍入） | `emit_int_to_f64`：`fcvt_from_sint` |
//! | 数值比较 | signed ordered；`Ne` 用 unordered `NotEqual`（`NaN != NaN → true`） | `emit_i64_cmp` / `emit_f64_cmp` |
//! | 整数移位量 | mask 到低 6 位（`& SHIFT_MASK`） | `emit_i64_binop`：`Shl`/`Shr` 前 `band 63` |
//!
//! 混合 I64/F64 运算（`int_binop` 自动加宽）永不走内联（内联仅在 `reg_types` 证明全 I64 或
//! 全 F64 时触发），故加宽规则只在本模块 + JIT helper 两路，无内联镜像。

use crate::metadata::Value;
use crate::metadata::superinstr::CmpOp;
use anyhow::{bail, Result};

/// 整数移位量 mask（低 6 位）—— interp `shl`/`shr` 与 JIT 内联 `Shl`/`Shr` 共用此常量。
pub const SHIFT_MASK: i64 = 63;

/// `Std.DivideByZeroException` 全限定名 —— 整数除零三路共用。
pub const DIV_BY_ZERO_EXC: &str = "Std.DivideByZeroException";

// ── 算术 / 位运算 ─────────────────────────────────────────────────────────────

/// 整数 / 浮点二元算术，含 I64/F64 自动加宽。具体规则（`wrapping_add` / `x / y` 等）
/// 由调用方以闭包传入，故本函数同时服务 add/sub/mul/div/rem——差异只在闭包。
pub fn int_binop(
    va: &Value, vb: &Value,
    int_op: impl Fn(i64, i64) -> i64,
    float_op: impl Fn(f64, f64) -> f64,
) -> Result<Value> {
    Ok(match (va, vb) {
        (Value::I64(x), Value::I64(y)) => Value::I64(int_op(*x, *y)),
        (Value::F64(x), Value::F64(y)) => Value::F64(float_op(*x, *y)),
        (Value::F64(x), Value::I64(y)) => Value::F64(float_op(*x, *y as f64)),
        (Value::I64(x), Value::F64(y)) => Value::F64(float_op(*x as f64, *y)),
        (a, b) => bail!("type mismatch in arithmetic: {:?} vs {:?}", a, b),
    })
}

/// 整数位运算 / 移位（拒绝浮点操作数）。移位调用方需在闭包内 `& SHIFT_MASK`。
pub fn int_bitop(
    va: &Value, vb: &Value, op: impl Fn(i64, i64) -> i64,
) -> Result<Value> {
    Ok(match (va, vb) {
        (Value::I64(x), Value::I64(y)) => Value::I64(op(*x, *y)),
        (a, b) => bail!("bitwise op requires integral operands, got {:?} and {:?}", a, b),
    })
}

// ── 比较 ──────────────────────────────────────────────────────────────────────

/// 数值 `<` 比较，含 I64/F64 自动加宽 + Char/I64 加宽。
///
/// fix-char-comparison (2026-05-24)：`Value::Char` 臂使 `c < '0'` / `c >= '9'` 等在用户
/// 代码里无需显式 `(int)c` 即可工作；Char-Char 比码点，混合 Char/I64 把 Char 加宽到 I64。
pub fn numeric_lt(va: &Value, vb: &Value) -> Result<bool> {
    Ok(match (va, vb) {
        (Value::I64(x), Value::I64(y)) => x < y,
        (Value::F64(x), Value::F64(y)) => x < y,
        (Value::F64(x), Value::I64(y)) => *x < (*y as f64),
        (Value::I64(x), Value::F64(y)) => (*x as f64) < *y,
        (Value::Char(x), Value::Char(y)) => x < y,
        (Value::Char(x), Value::I64(y))  => (*x as u32 as i64) < *y,
        (Value::I64(x),  Value::Char(y)) => *x < (*y as u32 as i64),
        (a, b) => bail!("type mismatch in comparison: {:?} vs {:?}", a, b),
    })
}

/// 六种比较的统一求值。`Lt`/`Le`/`Gt`/`Ge` 走 [`numeric_lt`]（含加宽），`Eq`/`Ne` 走
/// `Value` 的 `PartialEq`（对所有类型，含引用类型 / 浮点 ordered 相等）。
///
/// interp 标准 cmp 处理器、interp 融合的 `CmpBr` 超指令、JIT helper 的 `jit_lt` 等三处
/// 共用此原语，比较逻辑只此一份。
pub fn eval_cmp(op: CmpOp, va: &Value, vb: &Value) -> Result<bool> {
    Ok(match op {
        CmpOp::Lt => numeric_lt(va, vb)?,
        CmpOp::Le => !numeric_lt(vb, va)?,
        CmpOp::Gt => numeric_lt(vb, va)?,
        CmpOp::Ge => !numeric_lt(va, vb)?,
        CmpOp::Eq => va == vb,
        CmpOp::Ne => va != vb,
    })
}

// ── 整数除零（三路共用决策；异常对象构造留调用点，因其需 VmContext/Module）──────────

/// 整数除零判定：`divisor` 为 `I64(0)` 时为真 → 应抛 [`DIV_BY_ZERO_EXC`]。
/// 浮点 / 混合 I64-F64 除零走 IEEE 754（Infinity / NaN），返回 `false` 由 `float_op` 处理。
#[inline]
pub fn is_int_div_by_zero(divisor: &Value) -> bool {
    matches!(divisor, Value::I64(0))
}

/// 整数除零异常消息（三路共用文案）。`op` 为 `"/"` 或 `"%"`。
pub fn div_by_zero_msg(op: &str) -> String {
    format!("integer {op} by zero")
}

// ── 数值转换（cast）───────────────────────────────────────────────────────────

/// 目标类型标签常量 —— 镜像 `compiler/z42.IR/BinaryFormat/Opcodes.cs::TypeTags`。
/// 就近内联于此（而非读 `metadata::tokens`）保持 `convert_value` 自足；权威在 C# 侧。
pub const T_BOOL: u8 = 0x01;
pub const T_I8:   u8 = 0x02;
pub const T_I16:  u8 = 0x03;
pub const T_I32:  u8 = 0x04;
pub const T_I64:  u8 = 0x05;
pub const T_U8:   u8 = 0x06;
pub const T_U16:  u8 = 0x07;
pub const T_U32:  u8 = 0x08;
pub const T_U64:  u8 = 0x09;
pub const T_F32:  u8 = 0x0A;
pub const T_F64:  u8 = 0x0B;
pub const T_CHAR:   u8 = 0x0C;
pub const T_STR:    u8 = 0x0D;
pub const T_OBJECT: u8 = 0x20;
pub const T_ARRAY:  u8 = 0x21;

/// 纯数值转换 —— 无 frame 读写副作用，故 interp `convert` 处理器与 JIT `jit_convert`
/// helper 都复用它。
///
/// 引用类型 identity cast 原样透传。编译器为每个 cast（含 `(string)obj` / `(byte[])obj` /
/// `(SomeClass)obj`）都发单个 `Convert` IR，故运行期需识别「窄化静态类型、值已匹配」为 no-op
/// 而非数值转换。`Null` 可 cast 到任意引用目标。(add-std-process, 2026-05-13.)
pub fn convert_value(v: Value, to_tag: u8) -> Result<Value> {
    // add-primitive-value-boxing → unify Phase 2 R3：`(T)o` 于装箱基元 → 先拆箱再转标量。
    // 基元盒现是 `BoxedStruct`（标量存 struct_bytes）；非整数盒 boxed_prim_i64 返 None → 不拦截。
    if let Value::BoxedStruct(gc) = &v {
        if let Some(n) = gc.borrow().boxed_prim_i64() {
            return convert_value(Value::I64(n), to_tag);
        }
    }
    // 引用类型 identity cast —— 值的动态种类已匹配窄化后的静态目标。
    match (&v, to_tag) {
        (Value::Str(_),    T_STR)    => return Ok(v),
        (Value::Array(_),  T_ARRAY)  => return Ok(v),
        (Value::Object(_), T_OBJECT) => return Ok(v),
        // 装箱 bool 拆箱：`(bool)o`（o 持 Bool）。bool 无数值转换臂（bool↔数值在 TypeCheck 拒），
        // 故此 identity 匹配是唯一有效 bool 拆箱路径。(add-boxing-conversions)
        (Value::Bool(_),   T_BOOL)   => return Ok(v),
        // Null → 任意引用目标。
        (Value::Null,      T_STR | T_OBJECT | T_ARRAY) => return Ok(v),
        _ => {}
    }
    match v {
        Value::F64(f)  => convert_from_f64(f, to_tag),
        Value::I64(x)  => convert_from_i64(x, to_tag),
        Value::Char(c) => convert_from_char(c, to_tag),
        // bool / str / object 等 —— TypeChecker 应已拒；防御性 bail。
        other => bail!("InvalidCastException: cannot convert {:?} to type tag 0x{:02X}", other, to_tag),
    }
}

fn convert_from_f64(f: f64, to_tag: u8) -> Result<Value> {
    Ok(match to_tag {
        T_F32 | T_F64 => Value::F64(f),
        T_I8  => Value::I64((f as i8) as i64),
        T_I16 => Value::I64((f as i16) as i64),
        T_I32 => Value::I64((f as i32) as i64),
        T_I64 => Value::I64(f as i64),
        T_U8  => Value::I64((f as u8) as i64),
        T_U16 => Value::I64((f as u16) as i64),
        T_U32 => Value::I64((f as u32) as i64),
        T_U64 => Value::I64(f as i64),  // saturating same as f → i64
        T_CHAR => {
            let u = f as u32;
            char::from_u32(u)
                .map(Value::Char)
                .ok_or_else(|| anyhow::anyhow!(
                    "InvalidCastException: 0x{:X} not a valid Unicode scalar", u))?
        }
        T_BOOL => bail!("InvalidCastException: cannot cast f64 to bool"),
        _ => bail!("InvalidCastException: unknown target tag 0x{:02X} for f64 source", to_tag),
    })
}

fn convert_from_i64(x: i64, to_tag: u8) -> Result<Value> {
    Ok(match to_tag {
        T_I8  => Value::I64((x as i8) as i64),
        T_I16 => Value::I64((x as i16) as i64),
        T_I32 => Value::I64((x as i32) as i64),
        T_I64 => Value::I64(x),
        T_U8  => Value::I64((x as u8) as i64),
        T_U16 => Value::I64((x as u16) as i64),
        T_U32 => Value::I64((x as u32) as i64),
        T_U64 => Value::I64(x),
        T_F32 | T_F64 => Value::F64(x as f64),
        T_CHAR => {
            let u = x as u32;
            char::from_u32(u)
                .map(Value::Char)
                .ok_or_else(|| anyhow::anyhow!(
                    "InvalidCastException: 0x{:X} not a valid Unicode scalar", u))?
        }
        T_BOOL => bail!("InvalidCastException: cannot cast int to bool"),
        _ => bail!("InvalidCastException: unknown target tag 0x{:02X} for i64 source", to_tag),
    })
}

fn convert_from_char(c: char, to_tag: u8) -> Result<Value> {
    let u = c as u32;
    Ok(match to_tag {
        T_I8  => Value::I64((u as i8) as i64),
        T_I16 => Value::I64((u as i16) as i64),
        T_I32 => Value::I64(u as i32 as i64),
        T_I64 => Value::I64(u as i64),
        T_U8  => Value::I64((u as u8) as i64),
        T_U16 => Value::I64((u as u16) as i64),
        T_U32 => Value::I64(u as i64),
        T_U64 => Value::I64(u as i64),
        T_F32 | T_F64 => Value::F64(u as f64),
        T_CHAR => Value::Char(c),
        T_BOOL => bail!("InvalidCastException: cannot cast char to bool"),
        _ => bail!("InvalidCastException: unknown target tag 0x{:02X} for char source", to_tag),
    })
}

#[cfg(test)]
#[path = "semantics_tests.rs"]
mod semantics_tests;
