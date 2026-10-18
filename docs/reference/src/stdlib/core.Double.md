<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Double

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Primitives/Double.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L4)

内置 `double` 类型（64 位 IEEE-754 浮点）的包装结构体，实现 IComparable / IEquatable / INumber。

```z42
public struct Double : IComparable, IEquatable, INumber {
    static double MaxValue
    static double MinValue
    static double Epsilon
    static double NaN
    static double PositiveInfinity
    static double NegativeInfinity
    static extern double Parse(string s)
    extern string ToString()
    static extern bool TryParse(string s, ref double v)
    static bool IsNaN(double v)
    static bool IsInfinity(double v)
    static bool IsPositiveInfinity(double v)
    static bool IsNegativeInfinity(double v)
    static bool IsFinite(double v)
    bool Equals(double other)
    int GetHashCode()
    int CompareTo(double other)
    static override double op_Add(double a, double b)
    static override double op_Subtract(double a, double b)
    static override double op_Multiply(double a, double b)
    static override double op_Divide(double a, double b)
    static override double op_Modulo(double a, double b)
}
```

成员：[`MaxValue`](#Std.Double.MaxValue) · [`MinValue`](#Std.Double.MinValue) · [`Epsilon`](#Std.Double.Epsilon) · [`NaN`](#Std.Double.NaN) · [`PositiveInfinity`](#Std.Double.PositiveInfinity) · [`NegativeInfinity`](#Std.Double.NegativeInfinity) · [`Parse`](#Std.Double.Parse(string)) · [`ToString`](#Std.Double.ToString()) · [`TryParse`](#Std.Double.TryParse(string,double)) · [`IsNaN`](#Std.Double.IsNaN(double)) · [`IsInfinity`](#Std.Double.IsInfinity(double)) · [`IsPositiveInfinity`](#Std.Double.IsPositiveInfinity(double)) · [`IsNegativeInfinity`](#Std.Double.IsNegativeInfinity(double)) · [`IsFinite`](#Std.Double.IsFinite(double)) · [`Equals`](#Std.Double.Equals(double)) · [`GetHashCode`](#Std.Double.GetHashCode()) · [`CompareTo`](#Std.Double.CompareTo(double)) · [`op_Add`](#Std.Double.op_Add(double,double)) · [`op_Subtract`](#Std.Double.op_Subtract(double,double)) · [`op_Multiply`](#Std.Double.op_Multiply(double,double)) · [`op_Divide`](#Std.Double.op_Divide(double,double)) · [`op_Modulo`](#Std.Double.op_Modulo(double,double))

<a id="Std.Double.MaxValue"></a>
### static double MaxValue

[`src/libraries/z42.core/src/Primitives/Double.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L6)

最大有限值。

<a id="Std.Double.MinValue"></a>
### static double MinValue

[`src/libraries/z42.core/src/Primitives/Double.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L8)

最小有限值。

<a id="Std.Double.Epsilon"></a>
### static double Epsilon

[`src/libraries/z42.core/src/Primitives/Double.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L10)

最小正数（最小可表示的正规 / 次正规差值）。

<a id="Std.Double.NaN"></a>
### static double NaN

[`src/libraries/z42.core/src/Primitives/Double.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L13)

非数值（NaN）。

<a id="Std.Double.PositiveInfinity"></a>
### static double PositiveInfinity

[`src/libraries/z42.core/src/Primitives/Double.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L15)

正无穷。

<a id="Std.Double.NegativeInfinity"></a>
### static double NegativeInfinity

[`src/libraries/z42.core/src/Primitives/Double.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L17)

负无穷。

<a id="Std.Double.Parse(string)"></a>
### static extern double Parse(string s)

[`src/libraries/z42.core/src/Primitives/Double.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L20)

解析浮点字符串；格式错误抛 FormatException，越界抛 OverflowException，null 抛 ArgumentNullException。

<a id="Std.Double.ToString()"></a>
### extern string ToString()

[`src/libraries/z42.core/src/Primitives/Double.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L22)

返回字符串表示。

<a id="Std.Double.TryParse(string,double)"></a>
### static extern bool TryParse(string s, ref double v)

[`src/libraries/z42.core/src/Primitives/Double.z42:26`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L26)

非抛出解析：成功把值写入 `v` 并返回 true；格式错误 / 越界 / null 返回 false 且 `v` 置 0.0。

<a id="Std.Double.IsNaN(double)"></a>
### static bool IsNaN(double v)

[`src/libraries/z42.core/src/Primitives/Double.z42:29`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L29)

v 是否为 NaN（NaN 是唯一不等于自身的值）。

<a id="Std.Double.IsInfinity(double)"></a>
### static bool IsInfinity(double v)

[`src/libraries/z42.core/src/Primitives/Double.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L32)

v 是否为正无穷或负无穷。

<a id="Std.Double.IsPositiveInfinity(double)"></a>
### static bool IsPositiveInfinity(double v)

[`src/libraries/z42.core/src/Primitives/Double.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L36)

v 是否为正无穷。

<a id="Std.Double.IsNegativeInfinity(double)"></a>
### static bool IsNegativeInfinity(double v)

[`src/libraries/z42.core/src/Primitives/Double.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L38)

v 是否为负无穷。

<a id="Std.Double.IsFinite(double)"></a>
### static bool IsFinite(double v)

[`src/libraries/z42.core/src/Primitives/Double.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L40)

v 是否为有限值（非 NaN 且非无穷）。

<a id="Std.Double.Equals(double)"></a>
### bool Equals(double other)

[`src/libraries/z42.core/src/Primitives/Double.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L43)

值相等比较（NaN != NaN）。

<a id="Std.Double.GetHashCode()"></a>
### int GetHashCode()

[`src/libraries/z42.core/src/Primitives/Double.z42:45`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L45)

返回哈希码（折叠 IEEE-754 位模式高低 32 位）。

<a id="Std.Double.CompareTo(double)"></a>
### int CompareTo(double other)

[`src/libraries/z42.core/src/Primitives/Double.z42:51`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L51)

比较大小，返回负 / 0 / 正。

<a id="Std.Double.op_Add(double,double)"></a>
### static override double op_Add(double a, double b)

[`src/libraries/z42.core/src/Primitives/Double.z42:67`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L67)

加法。

<a id="Std.Double.op_Subtract(double,double)"></a>
### static override double op_Subtract(double a, double b)

[`src/libraries/z42.core/src/Primitives/Double.z42:69`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L69)

减法。

<a id="Std.Double.op_Multiply(double,double)"></a>
### static override double op_Multiply(double a, double b)

[`src/libraries/z42.core/src/Primitives/Double.z42:71`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L71)

乘法。

<a id="Std.Double.op_Divide(double,double)"></a>
### static override double op_Divide(double a, double b)

[`src/libraries/z42.core/src/Primitives/Double.z42:73`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L73)

除法。

<a id="Std.Double.op_Modulo(double,double)"></a>
### static override double op_Modulo(double a, double b)

[`src/libraries/z42.core/src/Primitives/Double.z42:75`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Double.z42#L75)

取模。

