<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Math

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Math.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L8)

数学常量与数值函数的静态容器。

常量（Pi / E / Tau）与纯脚本辅助函数，加上 libm 的 native 绑定（幂、三角、对数等）。
整数版取独立名字（AbsInt / MaxInt / MinInt / SignInt / ClampInt）：z42 按 arity 消歧、
无法承载两个同 arity 重载，故 int 保持型与 double 版分开命名。

```z42
public static class Math {
    static double Pi
    static double E
    static double Tau
    static double Abs(double x)
    static int AbsInt(int x)
    static double Max(double a, double b)
    static int MaxInt(int a, int b)
    static double Min(double a, double b)
    static int MinInt(int a, int b)
    static int Sign(double x)
    static int SignInt(int x)
    static double Clamp(double x, double lo, double hi)
    static int ClampInt(int x, int lo, int hi)
    static double Truncate(double x)
    static double Round(double x, int digits)
    static extern double Pow(double base, double exp)
    static extern double Sqrt(double x)
    static extern double Floor(double x)
    static extern double Ceiling(double x)
    static extern double Round(double x)
    static extern double Log(double x)
    static extern double Log10(double x)
    static extern double Sin(double x)
    static extern double Cos(double x)
    static extern double Tan(double x)
    static extern double Atan2(double y, double x)
    static extern double Exp(double x)
    static extern double Asin(double x)
    static extern double Acos(double x)
    static extern double Atan(double x)
    static extern double Sinh(double x)
    static extern double Cosh(double x)
    static extern double Tanh(double x)
    static extern double Cbrt(double x)
    static extern double Log2(double x)
}
```

成员：[`Pi`](#Std.Math.Pi) · [`E`](#Std.Math.E) · [`Tau`](#Std.Math.Tau) · [`Abs`](#Std.Math.Abs(double)) · [`AbsInt`](#Std.Math.AbsInt(int)) · [`Max`](#Std.Math.Max(double,double)) · [`MaxInt`](#Std.Math.MaxInt(int,int)) · [`Min`](#Std.Math.Min(double,double)) · [`MinInt`](#Std.Math.MinInt(int,int)) · [`Sign`](#Std.Math.Sign(double)) · [`SignInt`](#Std.Math.SignInt(int)) · [`Clamp`](#Std.Math.Clamp(double,double,double)) · [`ClampInt`](#Std.Math.ClampInt(int,int,int)) · [`Truncate`](#Std.Math.Truncate(double)) · [`Round`](#Std.Math.Round(double,int)) · [`Pow`](#Std.Math.Pow(double,double)) · [`Sqrt`](#Std.Math.Sqrt(double)) · [`Floor`](#Std.Math.Floor(double)) · [`Ceiling`](#Std.Math.Ceiling(double)) · [`Round`](#Std.Math.Round(double)) · [`Log`](#Std.Math.Log(double)) · [`Log10`](#Std.Math.Log10(double)) · [`Sin`](#Std.Math.Sin(double)) · [`Cos`](#Std.Math.Cos(double)) · [`Tan`](#Std.Math.Tan(double)) · [`Atan2`](#Std.Math.Atan2(double,double)) · [`Exp`](#Std.Math.Exp(double)) · [`Asin`](#Std.Math.Asin(double)) · [`Acos`](#Std.Math.Acos(double)) · [`Atan`](#Std.Math.Atan(double)) · [`Sinh`](#Std.Math.Sinh(double)) · [`Cosh`](#Std.Math.Cosh(double)) · [`Tanh`](#Std.Math.Tanh(double)) · [`Cbrt`](#Std.Math.Cbrt(double)) · [`Log2`](#Std.Math.Log2(double))

<a id="Std.Math.Pi"></a>
### static double Pi

[`src/libraries/z42.core/src/Math.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L11)

圆周率 π。

<a id="Std.Math.E"></a>
### static double E

[`src/libraries/z42.core/src/Math.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L13)

自然常数 e。

<a id="Std.Math.Tau"></a>
### static double Tau

[`src/libraries/z42.core/src/Math.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L15)

圆周率 × 2（τ）。

<a id="Std.Math.Abs(double)"></a>
### static double Abs(double x)

[`src/libraries/z42.core/src/Math.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L22)

绝对值。

<a id="Std.Math.AbsInt(int)"></a>
### static int AbsInt(int x)

[`src/libraries/z42.core/src/Math.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L27)

绝对值（int 保持型）。

<a id="Std.Math.Max(double,double)"></a>
### static double Max(double a, double b)

[`src/libraries/z42.core/src/Math.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L32)

较大者。

<a id="Std.Math.MaxInt(int,int)"></a>
### static int MaxInt(int a, int b)

[`src/libraries/z42.core/src/Math.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L37)

较大者（int 保持型）。

<a id="Std.Math.Min(double,double)"></a>
### static double Min(double a, double b)

[`src/libraries/z42.core/src/Math.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L42)

较小者。

<a id="Std.Math.MinInt(int,int)"></a>
### static int MinInt(int a, int b)

[`src/libraries/z42.core/src/Math.z42:47`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L47)

较小者（int 保持型）。

<a id="Std.Math.Sign(double)"></a>
### static int Sign(double x)

[`src/libraries/z42.core/src/Math.z42:53`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L53)

符号：-1 / 0 / +1。

<a id="Std.Math.SignInt(int)"></a>
### static int SignInt(int x)

[`src/libraries/z42.core/src/Math.z42:59`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L59)

符号（int 保持型）：-1 / 0 / +1。

<a id="Std.Math.Clamp(double,double,double)"></a>
### static double Clamp(double x, double lo, double hi)

[`src/libraries/z42.core/src/Math.z42:66`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L66)

把 `x` 夹到 `[lo, hi]`（假定 lo <= hi）。

<a id="Std.Math.ClampInt(int,int,int)"></a>
### static int ClampInt(int x, int lo, int hi)

[`src/libraries/z42.core/src/Math.z42:72`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L72)

把 `x` 夹到 `[lo, hi]`（int 保持型）。

<a id="Std.Math.Truncate(double)"></a>
### static double Truncate(double x)

[`src/libraries/z42.core/src/Math.z42:81`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L81)

向零取整（丢弃小数部分），同 System.Math.Truncate。

<a id="Std.Math.Round(double,int)"></a>
### static double Round(double x, int digits)

[`src/libraries/z42.core/src/Math.z42:86`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L86)

四舍五入到 `digits` 位小数。放大到整数范围、经单参 Round 后再缩回。

<a id="Std.Math.Pow(double,double)"></a>
### static extern double Pow(double base, double exp)

[`src/libraries/z42.core/src/Math.z42:93`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L93)

幂 `base^exp`。

<a id="Std.Math.Sqrt(double)"></a>
### static extern double Sqrt(double x)

[`src/libraries/z42.core/src/Math.z42:95`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L95)

平方根。

<a id="Std.Math.Floor(double)"></a>
### static extern double Floor(double x)

[`src/libraries/z42.core/src/Math.z42:97`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L97)

向下取整（不大于 x 的最大整数）。

<a id="Std.Math.Ceiling(double)"></a>
### static extern double Ceiling(double x)

[`src/libraries/z42.core/src/Math.z42:99`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L99)

向上取整（不小于 x 的最小整数）。

<a id="Std.Math.Round(double)"></a>
### static extern double Round(double x)

[`src/libraries/z42.core/src/Math.z42:101`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L101)

四舍五入到最近的整数。

<a id="Std.Math.Log(double)"></a>
### static extern double Log(double x)

[`src/libraries/z42.core/src/Math.z42:103`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L103)

自然对数（以 e 为底）。

<a id="Std.Math.Log10(double)"></a>
### static extern double Log10(double x)

[`src/libraries/z42.core/src/Math.z42:105`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L105)

常用对数（以 10 为底）。

<a id="Std.Math.Sin(double)"></a>
### static extern double Sin(double x)

[`src/libraries/z42.core/src/Math.z42:107`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L107)

正弦（弧度）。

<a id="Std.Math.Cos(double)"></a>
### static extern double Cos(double x)

[`src/libraries/z42.core/src/Math.z42:109`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L109)

余弦（弧度）。

<a id="Std.Math.Tan(double)"></a>
### static extern double Tan(double x)

[`src/libraries/z42.core/src/Math.z42:111`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L111)

正切（弧度）。

<a id="Std.Math.Atan2(double,double)"></a>
### static extern double Atan2(double y, double x)

[`src/libraries/z42.core/src/Math.z42:113`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L113)

反正切（`atan2(y, x)`，结果在 (-π, π]）。

<a id="Std.Math.Exp(double)"></a>
### static extern double Exp(double x)

[`src/libraries/z42.core/src/Math.z42:115`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L115)

自然指数 `e^x`。

<a id="Std.Math.Asin(double)"></a>
### static extern double Asin(double x)

[`src/libraries/z42.core/src/Math.z42:117`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L117)

反正弦。

<a id="Std.Math.Acos(double)"></a>
### static extern double Acos(double x)

[`src/libraries/z42.core/src/Math.z42:119`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L119)

反余弦。

<a id="Std.Math.Atan(double)"></a>
### static extern double Atan(double x)

[`src/libraries/z42.core/src/Math.z42:121`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L121)

反正切。

<a id="Std.Math.Sinh(double)"></a>
### static extern double Sinh(double x)

[`src/libraries/z42.core/src/Math.z42:123`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L123)

双曲正弦。

<a id="Std.Math.Cosh(double)"></a>
### static extern double Cosh(double x)

[`src/libraries/z42.core/src/Math.z42:125`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L125)

双曲余弦。

<a id="Std.Math.Tanh(double)"></a>
### static extern double Tanh(double x)

[`src/libraries/z42.core/src/Math.z42:127`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L127)

双曲正切。

<a id="Std.Math.Cbrt(double)"></a>
### static extern double Cbrt(double x)

[`src/libraries/z42.core/src/Math.z42:129`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L129)

立方根。

<a id="Std.Math.Log2(double)"></a>
### static extern double Log2(double x)

[`src/libraries/z42.core/src/Math.z42:131`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Math.z42#L131)

以 2 为底的对数。

