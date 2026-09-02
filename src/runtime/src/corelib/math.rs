use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

// 2026-04-27 wave1-math-script: builtin_math_abs/_max/_min removed.
// `Std.Math.Math.Abs/Max/Min` 现在是 z42 脚本（int + double overload）。

pub fn builtin_math_pow(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match (args.first(), args.get(1)) {
        (Some(Value::I64(base)), Some(Value::I64(exp))) => Ok(Value::I64(base.pow(*exp as u32))),
        (Some(Value::F64(base)), Some(Value::F64(exp))) => Ok(Value::F64(base.powf(*exp))),
        _ => bail!("Math.Pow: expected two numeric arguments"),
    }
}
pub fn builtin_math_sqrt(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::F64(f)) => Ok(Value::F64(f.sqrt())),
        Some(Value::I64(n)) => Ok(Value::F64((*n as f64).sqrt())),
        _ => bail!("Math.Sqrt: expected numeric argument"),
    }
}
pub fn builtin_math_floor(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::F64(f)) => Ok(Value::F64(f.floor())),
        Some(Value::I64(n)) => Ok(Value::I64(*n)),
        _ => bail!("Math.Floor: expected numeric argument"),
    }
}
pub fn builtin_math_ceiling(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::F64(f)) => Ok(Value::F64(f.ceil())),
        Some(Value::I64(n)) => Ok(Value::I64(*n)),
        _ => bail!("Math.Ceiling: expected numeric argument"),
    }
}
pub fn builtin_math_round(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::F64(f)) => Ok(Value::F64(f.round())),
        Some(Value::I64(n)) => Ok(Value::I64(*n)),
        _ => bail!("Math.Round: expected numeric argument"),
    }
}
pub fn builtin_math_log(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::F64(f)) => Ok(Value::F64(f.ln())),
        Some(Value::I64(n)) => Ok(Value::F64((*n as f64).ln())),
        _ => bail!("Math.Log: expected numeric argument"),
    }
}
pub fn builtin_math_log10(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::F64(f)) => Ok(Value::F64(f.log10())),
        Some(Value::I64(n)) => Ok(Value::F64((*n as f64).log10())),
        _ => bail!("Math.Log10: expected numeric argument"),
    }
}
pub fn builtin_math_sin(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::F64(f)) => Ok(Value::F64(f.sin())),
        Some(Value::I64(n)) => Ok(Value::F64((*n as f64).sin())),
        _ => bail!("Math.Sin: expected numeric argument"),
    }
}
pub fn builtin_math_cos(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::F64(f)) => Ok(Value::F64(f.cos())),
        Some(Value::I64(n)) => Ok(Value::F64((*n as f64).cos())),
        _ => bail!("Math.Cos: expected numeric argument"),
    }
}
pub fn builtin_math_tan(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::F64(f)) => Ok(Value::F64(f.tan())),
        Some(Value::I64(n)) => Ok(Value::F64((*n as f64).tan())),
        _ => bail!("Math.Tan: expected numeric argument"),
    }
}
pub fn builtin_math_atan2(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match (args.first(), args.get(1)) {
        (Some(Value::F64(y)), Some(Value::F64(x))) => Ok(Value::F64(y.atan2(*x))),
        _ => bail!("Math.Atan2: expected two f64 arguments"),
    }
}
pub fn builtin_math_exp(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::F64(f)) => Ok(Value::F64(f.exp())),
        Some(Value::I64(n)) => Ok(Value::F64((*n as f64).exp())),
        _ => bail!("Math.Exp: expected numeric argument"),
    }
}

// augment-corelib-parity (2026-09-02, backlog #9): inverse-trig, hyperbolic,
// cube-root and base-2 log rounding out System.Math parity. Backed by libm via
// Rust f64 methods so edge cases (overflow of sinh/cosh, exactness of log2 on
// powers of two) match the platform math library rather than a lossy z42-script
// derivation.
pub fn builtin_math_asin(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::F64(f)) => Ok(Value::F64(f.asin())),
        Some(Value::I64(n)) => Ok(Value::F64((*n as f64).asin())),
        _ => bail!("Math.Asin: expected numeric argument"),
    }
}
pub fn builtin_math_acos(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::F64(f)) => Ok(Value::F64(f.acos())),
        Some(Value::I64(n)) => Ok(Value::F64((*n as f64).acos())),
        _ => bail!("Math.Acos: expected numeric argument"),
    }
}
pub fn builtin_math_atan(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::F64(f)) => Ok(Value::F64(f.atan())),
        Some(Value::I64(n)) => Ok(Value::F64((*n as f64).atan())),
        _ => bail!("Math.Atan: expected numeric argument"),
    }
}
pub fn builtin_math_sinh(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::F64(f)) => Ok(Value::F64(f.sinh())),
        Some(Value::I64(n)) => Ok(Value::F64((*n as f64).sinh())),
        _ => bail!("Math.Sinh: expected numeric argument"),
    }
}
pub fn builtin_math_cosh(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::F64(f)) => Ok(Value::F64(f.cosh())),
        Some(Value::I64(n)) => Ok(Value::F64((*n as f64).cosh())),
        _ => bail!("Math.Cosh: expected numeric argument"),
    }
}
pub fn builtin_math_tanh(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::F64(f)) => Ok(Value::F64(f.tanh())),
        Some(Value::I64(n)) => Ok(Value::F64((*n as f64).tanh())),
        _ => bail!("Math.Tanh: expected numeric argument"),
    }
}
pub fn builtin_math_cbrt(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::F64(f)) => Ok(Value::F64(f.cbrt())),
        Some(Value::I64(n)) => Ok(Value::F64((*n as f64).cbrt())),
        _ => bail!("Math.Cbrt: expected numeric argument"),
    }
}
pub fn builtin_math_log2(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    match args.first() {
        Some(Value::F64(f)) => Ok(Value::F64(f.log2())),
        Some(Value::I64(n)) => Ok(Value::F64((*n as f64).log2())),
        _ => bail!("Math.Log2: expected numeric argument"),
    }
}
