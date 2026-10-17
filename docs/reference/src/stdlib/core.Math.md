<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Math

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Math.z42:8`

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

### static double Pi

`src/libraries/z42.core/src/Math.z42:11`

圆周率 π。

### static double E

`src/libraries/z42.core/src/Math.z42:13`

自然常数 e。

### static double Tau

`src/libraries/z42.core/src/Math.z42:15`

圆周率 × 2（τ）。

### static double Abs(double x)

`src/libraries/z42.core/src/Math.z42:22`

绝对值。

### static int AbsInt(int x)

`src/libraries/z42.core/src/Math.z42:27`

绝对值（int 保持型）。

### static double Max(double a, double b)

`src/libraries/z42.core/src/Math.z42:32`

较大者。

### static int MaxInt(int a, int b)

`src/libraries/z42.core/src/Math.z42:37`

较大者（int 保持型）。

### static double Min(double a, double b)

`src/libraries/z42.core/src/Math.z42:42`

较小者。

### static int MinInt(int a, int b)

`src/libraries/z42.core/src/Math.z42:47`

较小者（int 保持型）。

### static int Sign(double x)

`src/libraries/z42.core/src/Math.z42:53`

符号：-1 / 0 / +1。

### static int SignInt(int x)

`src/libraries/z42.core/src/Math.z42:59`

符号（int 保持型）：-1 / 0 / +1。

### static double Clamp(double x, double lo, double hi)

`src/libraries/z42.core/src/Math.z42:66`

把 `x` 夹到 `[lo, hi]`（假定 lo <= hi）。

### static int ClampInt(int x, int lo, int hi)

`src/libraries/z42.core/src/Math.z42:72`

把 `x` 夹到 `[lo, hi]`（int 保持型）。

### static double Truncate(double x)

`src/libraries/z42.core/src/Math.z42:81`

向零取整（丢弃小数部分），同 System.Math.Truncate。

### static double Round(double x, int digits)

`src/libraries/z42.core/src/Math.z42:86`

四舍五入到 `digits` 位小数。放大到整数范围、经单参 Round 后再缩回。

### static extern double Pow(double base, double exp)

`src/libraries/z42.core/src/Math.z42:93`

幂 `base^exp`。

### static extern double Sqrt(double x)

`src/libraries/z42.core/src/Math.z42:95`

平方根。

### static extern double Floor(double x)

`src/libraries/z42.core/src/Math.z42:97`

向下取整（不大于 x 的最大整数）。

### static extern double Ceiling(double x)

`src/libraries/z42.core/src/Math.z42:99`

向上取整（不小于 x 的最小整数）。

### static extern double Round(double x)

`src/libraries/z42.core/src/Math.z42:101`

四舍五入到最近的整数。

### static extern double Log(double x)

`src/libraries/z42.core/src/Math.z42:103`

自然对数（以 e 为底）。

### static extern double Log10(double x)

`src/libraries/z42.core/src/Math.z42:105`

常用对数（以 10 为底）。

### static extern double Sin(double x)

`src/libraries/z42.core/src/Math.z42:107`

正弦（弧度）。

### static extern double Cos(double x)

`src/libraries/z42.core/src/Math.z42:109`

余弦（弧度）。

### static extern double Tan(double x)

`src/libraries/z42.core/src/Math.z42:111`

正切（弧度）。

### static extern double Atan2(double y, double x)

`src/libraries/z42.core/src/Math.z42:113`

反正切（`atan2(y, x)`，结果在 (-π, π]）。

### static extern double Exp(double x)

`src/libraries/z42.core/src/Math.z42:115`

自然指数 `e^x`。

### static extern double Asin(double x)

`src/libraries/z42.core/src/Math.z42:117`

反正弦。

### static extern double Acos(double x)

`src/libraries/z42.core/src/Math.z42:119`

反余弦。

### static extern double Atan(double x)

`src/libraries/z42.core/src/Math.z42:121`

反正切。

### static extern double Sinh(double x)

`src/libraries/z42.core/src/Math.z42:123`

双曲正弦。

### static extern double Cosh(double x)

`src/libraries/z42.core/src/Math.z42:125`

双曲余弦。

### static extern double Tanh(double x)

`src/libraries/z42.core/src/Math.z42:127`

双曲正切。

### static extern double Cbrt(double x)

`src/libraries/z42.core/src/Math.z42:129`

立方根。

### static extern double Log2(double x)

`src/libraries/z42.core/src/Math.z42:131`

以 2 为底的对数。

