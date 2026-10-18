<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.UInt64

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Primitives/UInt64.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt64.z42#L5)

内置 `ulong` 类型（64 位无符号整数）的包装结构体，实现 IComparable / IEquatable / INumber。

```z42
public struct UInt64 : IComparable, IEquatable, INumber {
    static ulong MaxValue
    static ulong MinValue
    static extern ulong Parse(string s)
    extern string ToString()
    static extern bool TryParse(string s, ref ulong v)
    bool Equals(ulong other)
    int GetHashCode()
    int CompareTo(ulong other)
    static override ulong op_Add(ulong a, ulong b)
    static override ulong op_Subtract(ulong a, ulong b)
    static override ulong op_Multiply(ulong a, ulong b)
    static override ulong op_Divide(ulong a, ulong b)
    static override ulong op_Modulo(ulong a, ulong b)
}
```

成员：[`MaxValue`](#Std.UInt64.MaxValue) · [`MinValue`](#Std.UInt64.MinValue) · [`Parse`](#Std.UInt64.Parse(string)) · [`ToString`](#Std.UInt64.ToString()) · [`TryParse`](#Std.UInt64.TryParse(string,ulong)) · [`Equals`](#Std.UInt64.Equals(ulong)) · [`GetHashCode`](#Std.UInt64.GetHashCode()) · [`CompareTo`](#Std.UInt64.CompareTo(ulong)) · [`op_Add`](#Std.UInt64.op_Add(ulong,ulong)) · [`op_Subtract`](#Std.UInt64.op_Subtract(ulong,ulong)) · [`op_Multiply`](#Std.UInt64.op_Multiply(ulong,ulong)) · [`op_Divide`](#Std.UInt64.op_Divide(ulong,ulong)) · [`op_Modulo`](#Std.UInt64.op_Modulo(ulong,ulong))

<a id="Std.UInt64.MaxValue"></a>
### static ulong MaxValue

[`src/libraries/z42.core/src/Primitives/UInt64.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt64.z42#L7)

最大值：18446744073709551615（全 1 位）。

<a id="Std.UInt64.MinValue"></a>
### static ulong MinValue

[`src/libraries/z42.core/src/Primitives/UInt64.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt64.z42#L9)

最小值：0。

<a id="Std.UInt64.Parse(string)"></a>
### static extern ulong Parse(string s)

[`src/libraries/z42.core/src/Primitives/UInt64.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt64.z42#L13)

解析无符号整数字符串；格式错误抛 FormatException，越界抛 OverflowException， null 抛 ArgumentNullException。

<a id="Std.UInt64.ToString()"></a>
### extern string ToString()

[`src/libraries/z42.core/src/Primitives/UInt64.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt64.z42#L15)

返回十进制字符串表示。

<a id="Std.UInt64.TryParse(string,ulong)"></a>
### static extern bool TryParse(string s, ref ulong v)

[`src/libraries/z42.core/src/Primitives/UInt64.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt64.z42#L19)

非抛出解析：成功把值写入 `v` 并返回 true；格式错误 / 越界 / null 返回 false 且 `v` 置 0。

<a id="Std.UInt64.Equals(ulong)"></a>
### bool Equals(ulong other)

[`src/libraries/z42.core/src/Primitives/UInt64.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt64.z42#L22)

值相等比较。

<a id="Std.UInt64.GetHashCode()"></a>
### int GetHashCode()

[`src/libraries/z42.core/src/Primitives/UInt64.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt64.z42#L24)

返回哈希码（折叠高低 32 位，同 Int64）。

<a id="Std.UInt64.CompareTo(ulong)"></a>
### int CompareTo(ulong other)

[`src/libraries/z42.core/src/Primitives/UInt64.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt64.z42#L27)

比较大小，返回负 / 0 / 正。

<a id="Std.UInt64.op_Add(ulong,ulong)"></a>
### static override ulong op_Add(ulong a, ulong b)

[`src/libraries/z42.core/src/Primitives/UInt64.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt64.z42#L34)

加法。

<a id="Std.UInt64.op_Subtract(ulong,ulong)"></a>
### static override ulong op_Subtract(ulong a, ulong b)

[`src/libraries/z42.core/src/Primitives/UInt64.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt64.z42#L36)

减法。

<a id="Std.UInt64.op_Multiply(ulong,ulong)"></a>
### static override ulong op_Multiply(ulong a, ulong b)

[`src/libraries/z42.core/src/Primitives/UInt64.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt64.z42#L38)

乘法。

<a id="Std.UInt64.op_Divide(ulong,ulong)"></a>
### static override ulong op_Divide(ulong a, ulong b)

[`src/libraries/z42.core/src/Primitives/UInt64.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt64.z42#L40)

除法。

<a id="Std.UInt64.op_Modulo(ulong,ulong)"></a>
### static override ulong op_Modulo(ulong a, ulong b)

[`src/libraries/z42.core/src/Primitives/UInt64.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt64.z42#L42)

取模。

