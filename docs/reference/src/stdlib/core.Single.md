<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Single

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Primitives/Single.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L4)

内置 `float` 类型（32 位 IEEE-754 浮点）的包装结构体，实现 IComparable / IEquatable / INumber。

```z42
public struct Single : IComparable, IEquatable, INumber {
    static float MaxValue
    static float MinValue
    static float Epsilon
    static float NaN
    static float PositiveInfinity
    static float NegativeInfinity
    static float Parse(string s)
    static bool TryParse(string s, ref float v)
    static bool IsNaN(float v)
    static bool IsInfinity(float v)
    static bool IsPositiveInfinity(float v)
    static bool IsNegativeInfinity(float v)
    static bool IsFinite(float v)
    extern string ToString()
    bool Equals(float other)
    int GetHashCode()
    int CompareTo(float other)
    static override float op_Add(float a, float b)
    static override float op_Subtract(float a, float b)
    static override float op_Multiply(float a, float b)
    static override float op_Divide(float a, float b)
    static override float op_Modulo(float a, float b)
}
```

成员：[`MaxValue`](#Std.Single.MaxValue) · [`MinValue`](#Std.Single.MinValue) · [`Epsilon`](#Std.Single.Epsilon) · [`NaN`](#Std.Single.NaN) · [`PositiveInfinity`](#Std.Single.PositiveInfinity) · [`NegativeInfinity`](#Std.Single.NegativeInfinity) · [`Parse`](#Std.Single.Parse(string)) · [`TryParse`](#Std.Single.TryParse(string,float)) · [`IsNaN`](#Std.Single.IsNaN(float)) · [`IsInfinity`](#Std.Single.IsInfinity(float)) · [`IsPositiveInfinity`](#Std.Single.IsPositiveInfinity(float)) · [`IsNegativeInfinity`](#Std.Single.IsNegativeInfinity(float)) · [`IsFinite`](#Std.Single.IsFinite(float)) · [`ToString`](#Std.Single.ToString()) · [`Equals`](#Std.Single.Equals(float)) · [`GetHashCode`](#Std.Single.GetHashCode()) · [`CompareTo`](#Std.Single.CompareTo(float)) · [`op_Add`](#Std.Single.op_Add(float,float)) · [`op_Subtract`](#Std.Single.op_Subtract(float,float)) · [`op_Multiply`](#Std.Single.op_Multiply(float,float)) · [`op_Divide`](#Std.Single.op_Divide(float,float)) · [`op_Modulo`](#Std.Single.op_Modulo(float,float))

<a id="Std.Single.MaxValue"></a>
### static float MaxValue

[`src/libraries/z42.core/src/Primitives/Single.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L6)

最大有限值。

<a id="Std.Single.MinValue"></a>
### static float MinValue

[`src/libraries/z42.core/src/Primitives/Single.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L8)

最小有限值。

<a id="Std.Single.Epsilon"></a>
### static float Epsilon

[`src/libraries/z42.core/src/Primitives/Single.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L10)

最小正数。

<a id="Std.Single.NaN"></a>
### static float NaN

[`src/libraries/z42.core/src/Primitives/Single.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L13)

非数值（NaN）。

<a id="Std.Single.PositiveInfinity"></a>
### static float PositiveInfinity

[`src/libraries/z42.core/src/Primitives/Single.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L15)

正无穷。

<a id="Std.Single.NegativeInfinity"></a>
### static float NegativeInfinity

[`src/libraries/z42.core/src/Primitives/Single.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L18)

负无穷。

<a id="Std.Single.Parse(string)"></a>
### static float Parse(string s)

[`src/libraries/z42.core/src/Primitives/Single.z42:21`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L21)

解析浮点字符串（经 double 解析后窄化为 float）；失败抛异常，语义同 Double.Parse。

<a id="Std.Single.TryParse(string,float)"></a>
### static bool TryParse(string s, ref float v)

[`src/libraries/z42.core/src/Primitives/Single.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L24)

非抛出解析：成功把值写入 `v` 并返回 true；失败返回 false 且 `v` 置 0.0f。

<a id="Std.Single.IsNaN(float)"></a>
### static bool IsNaN(float v)

[`src/libraries/z42.core/src/Primitives/Single.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L32)

v 是否为 NaN（NaN 是唯一不等于自身的值）。

<a id="Std.Single.IsInfinity(float)"></a>
### static bool IsInfinity(float v)

[`src/libraries/z42.core/src/Primitives/Single.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L35)

v 是否为正无穷或负无穷。

<a id="Std.Single.IsPositiveInfinity(float)"></a>
### static bool IsPositiveInfinity(float v)

[`src/libraries/z42.core/src/Primitives/Single.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L39)

v 是否为正无穷。

<a id="Std.Single.IsNegativeInfinity(float)"></a>
### static bool IsNegativeInfinity(float v)

[`src/libraries/z42.core/src/Primitives/Single.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L41)

v 是否为负无穷。

<a id="Std.Single.IsFinite(float)"></a>
### static bool IsFinite(float v)

[`src/libraries/z42.core/src/Primitives/Single.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L43)

v 是否为有限值（非 NaN 且非无穷）。

<a id="Std.Single.ToString()"></a>
### extern string ToString()

[`src/libraries/z42.core/src/Primitives/Single.z42:46`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L46)

返回字符串表示。

<a id="Std.Single.Equals(float)"></a>
### bool Equals(float other)

[`src/libraries/z42.core/src/Primitives/Single.z42:49`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L49)

值相等比较（NaN != NaN）。

<a id="Std.Single.GetHashCode()"></a>
### int GetHashCode()

[`src/libraries/z42.core/src/Primitives/Single.z42:51`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L51)

返回哈希码（IEEE-754 位模式的 32 位值）。

<a id="Std.Single.CompareTo(float)"></a>
### int CompareTo(float other)

[`src/libraries/z42.core/src/Primitives/Single.z42:54`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L54)

比较大小，返回负 / 0 / 正。

<a id="Std.Single.op_Add(float,float)"></a>
### static override float op_Add(float a, float b)

[`src/libraries/z42.core/src/Primitives/Single.z42:69`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L69)

加法。

<a id="Std.Single.op_Subtract(float,float)"></a>
### static override float op_Subtract(float a, float b)

[`src/libraries/z42.core/src/Primitives/Single.z42:71`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L71)

减法。

<a id="Std.Single.op_Multiply(float,float)"></a>
### static override float op_Multiply(float a, float b)

[`src/libraries/z42.core/src/Primitives/Single.z42:73`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L73)

乘法。

<a id="Std.Single.op_Divide(float,float)"></a>
### static override float op_Divide(float a, float b)

[`src/libraries/z42.core/src/Primitives/Single.z42:75`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L75)

除法。

<a id="Std.Single.op_Modulo(float,float)"></a>
### static override float op_Modulo(float a, float b)

[`src/libraries/z42.core/src/Primitives/Single.z42:77`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Single.z42#L77)

取模。

