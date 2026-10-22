//! `Std.Math` 的标量后端 —— builtin ABI v2(P1-8 批次 10):全部
//! `f64 → f64`(双参两个)的 PURE 函数,z42 侧声明即 `double X(double)`,
//! 整数实参由类型系统先转换;`pure` 让 JIT 可做 intrinsic。
//!
//! wave1-math-script: Abs/Max/Min 是 z42 脚本(int + double
//! overload),不经这里。反三角 / 双曲 / cbrt / log2(augment-corelib-parity
//! backlog #9)由 libm 承载,边界(溢出、2 的幂上的 log2)跟平台数学库。

use crate::corelib::abi::BuiltinError;
use crate::vm_context::VmContext;


#[inline]
pub fn math_sqrt(x: f64) -> f64 {
    x.sqrt()
}

#[inline]
pub fn math_floor(x: f64) -> f64 {
    x.floor()
}

#[inline]
pub fn math_ceiling(x: f64) -> f64 {
    x.ceil()
}

#[inline]
pub fn math_round(x: f64) -> f64 {
    x.round()
}

#[inline]
pub fn math_log(x: f64) -> f64 {
    x.ln()
}

#[inline]
pub fn math_log10(x: f64) -> f64 {
    x.log10()
}

#[inline]
pub fn math_sin(x: f64) -> f64 {
    x.sin()
}

#[inline]
pub fn math_cos(x: f64) -> f64 {
    x.cos()
}

#[inline]
pub fn math_tan(x: f64) -> f64 {
    x.tan()
}

#[inline]
pub fn math_exp(x: f64) -> f64 {
    x.exp()
}

#[inline]
pub fn math_asin(x: f64) -> f64 {
    x.asin()
}

#[inline]
pub fn math_acos(x: f64) -> f64 {
    x.acos()
}

#[inline]
pub fn math_atan(x: f64) -> f64 {
    x.atan()
}

#[inline]
pub fn math_sinh(x: f64) -> f64 {
    x.sinh()
}

#[inline]
pub fn math_cosh(x: f64) -> f64 {
    x.cosh()
}

#[inline]
pub fn math_tanh(x: f64) -> f64 {
    x.tanh()
}

#[inline]
pub fn math_cbrt(x: f64) -> f64 {
    x.cbrt()
}

#[inline]
pub fn math_log2(x: f64) -> f64 {
    x.log2()
}

#[inline]
pub fn math_pow(base: f64, exp: f64) -> f64 {
    base.powf(exp)
}

#[inline]
pub fn math_atan2(y: f64, x: f64) -> f64 {
    y.atan2(x)
}

/// `Std.Math` 迁 builtin ABI v2(P1-8 批次 10):PURE 标量,签名即声明
/// (`double X(double)`),整数实参由 z42 类型系统先转换成 double。
#[z42_macros::builtins]
pub mod typed {

    #[builtin(name = "__math_sqrt", pure)]
    pub fn sqrt(x: f64) -> f64 { super::math_sqrt(x) }

    #[builtin(name = "__math_floor", pure)]
    pub fn floor(x: f64) -> f64 { super::math_floor(x) }

    #[builtin(name = "__math_ceiling", pure)]
    pub fn ceiling(x: f64) -> f64 { super::math_ceiling(x) }

    #[builtin(name = "__math_round", pure)]
    pub fn round(x: f64) -> f64 { super::math_round(x) }

    #[builtin(name = "__math_log", pure)]
    pub fn log(x: f64) -> f64 { super::math_log(x) }

    #[builtin(name = "__math_log10", pure)]
    pub fn log10(x: f64) -> f64 { super::math_log10(x) }

    #[builtin(name = "__math_sin", pure)]
    pub fn sin(x: f64) -> f64 { super::math_sin(x) }

    #[builtin(name = "__math_cos", pure)]
    pub fn cos(x: f64) -> f64 { super::math_cos(x) }

    #[builtin(name = "__math_tan", pure)]
    pub fn tan(x: f64) -> f64 { super::math_tan(x) }

    #[builtin(name = "__math_exp", pure)]
    pub fn exp(x: f64) -> f64 { super::math_exp(x) }

    #[builtin(name = "__math_asin", pure)]
    pub fn asin(x: f64) -> f64 { super::math_asin(x) }

    #[builtin(name = "__math_acos", pure)]
    pub fn acos(x: f64) -> f64 { super::math_acos(x) }

    #[builtin(name = "__math_atan", pure)]
    pub fn atan(x: f64) -> f64 { super::math_atan(x) }

    #[builtin(name = "__math_sinh", pure)]
    pub fn sinh(x: f64) -> f64 { super::math_sinh(x) }

    #[builtin(name = "__math_cosh", pure)]
    pub fn cosh(x: f64) -> f64 { super::math_cosh(x) }

    #[builtin(name = "__math_tanh", pure)]
    pub fn tanh(x: f64) -> f64 { super::math_tanh(x) }

    #[builtin(name = "__math_cbrt", pure)]
    pub fn cbrt(x: f64) -> f64 { super::math_cbrt(x) }

    #[builtin(name = "__math_log2", pure)]
    pub fn log2(x: f64) -> f64 { super::math_log2(x) }

    #[builtin(name = "__math_pow", pure)]
    pub fn pow(base: f64, exp: f64) -> f64 { super::math_pow(base, exp) }

    #[builtin(name = "__math_atan2", pure)]
    pub fn atan2(y: f64, x: f64) -> f64 { super::math_atan2(y, x) }
}
