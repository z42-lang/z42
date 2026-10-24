<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Int16

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Primitives/Int16.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int16.z42#L4)

内置 `short` 类型（16 位有符号整数）的包装结构体，实现 IComparable / IEquatable / INumber。

```z42
public struct Int16 : IComparable, IEquatable, INumber {
    static short MaxValue
    static short MinValue
    static extern short Parse(string s)
    extern string ToString()
    static extern bool TryParse(string s, ref short v)
    bool Equals(short other)
    int GetHashCode()
    int CompareTo(short other)
    static override short op_Add(short a, short b)
    static override short op_Subtract(short a, short b)
    static override short op_Multiply(short a, short b)
    static override short op_Divide(short a, short b)
    static override short op_Modulo(short a, short b)
}
```

成员：[`MaxValue`](#Std.Int16.MaxValue) · [`MinValue`](#Std.Int16.MinValue) · [`Parse`](#Std.Int16.Parse(string)) · [`ToString`](#Std.Int16.ToString()) · [`TryParse`](#Std.Int16.TryParse(string,short)) · [`Equals`](#Std.Int16.Equals(short)) · [`GetHashCode`](#Std.Int16.GetHashCode()) · [`CompareTo`](#Std.Int16.CompareTo(short)) · [`op_Add`](#Std.Int16.op_Add(short,short)) · [`op_Subtract`](#Std.Int16.op_Subtract(short,short)) · [`op_Multiply`](#Std.Int16.op_Multiply(short,short)) · [`op_Divide`](#Std.Int16.op_Divide(short,short)) · [`op_Modulo`](#Std.Int16.op_Modulo(short,short))

<a id="Std.Int16.MaxValue"></a>
### static short MaxValue

[`src/libraries/z42.core/src/Primitives/Int16.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int16.z42#L6)

最大值：32767。

<a id="Std.Int16.MinValue"></a>
### static short MinValue

[`src/libraries/z42.core/src/Primitives/Int16.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int16.z42#L8)

最小值：-32768。

<a id="Std.Int16.Parse(string)"></a>
### static extern short Parse(string s)

[`src/libraries/z42.core/src/Primitives/Int16.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int16.z42#L13)

解析整数字符串。

格式错误抛 FormatException，越界抛 OverflowException，null 抛 ArgumentNullException。

<a id="Std.Int16.ToString()"></a>
### extern string ToString()

[`src/libraries/z42.core/src/Primitives/Int16.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int16.z42#L16)

返回十进制字符串表示。

<a id="Std.Int16.TryParse(string,short)"></a>
### static extern bool TryParse(string s, ref short v)

[`src/libraries/z42.core/src/Primitives/Int16.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int16.z42#L19)

非抛出解析：成功把值写入 `v` 并返回 true；格式错误 / 越界 / null 返回 false 且 `v` 置 0。

<a id="Std.Int16.Equals(short)"></a>
### bool Equals(short other)

[`src/libraries/z42.core/src/Primitives/Int16.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int16.z42#L22)

值相等比较。

<a id="Std.Int16.GetHashCode()"></a>
### int GetHashCode()

[`src/libraries/z42.core/src/Primitives/Int16.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int16.z42#L24)

返回哈希码。

<a id="Std.Int16.CompareTo(short)"></a>
### int CompareTo(short other)

[`src/libraries/z42.core/src/Primitives/Int16.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int16.z42#L27)

比较大小，返回负 / 0 / 正。

<a id="Std.Int16.op_Add(short,short)"></a>
### static override short op_Add(short a, short b)

[`src/libraries/z42.core/src/Primitives/Int16.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int16.z42#L34)

加法。

<a id="Std.Int16.op_Subtract(short,short)"></a>
### static override short op_Subtract(short a, short b)

[`src/libraries/z42.core/src/Primitives/Int16.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int16.z42#L36)

减法。

<a id="Std.Int16.op_Multiply(short,short)"></a>
### static override short op_Multiply(short a, short b)

[`src/libraries/z42.core/src/Primitives/Int16.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int16.z42#L38)

乘法。

<a id="Std.Int16.op_Divide(short,short)"></a>
### static override short op_Divide(short a, short b)

[`src/libraries/z42.core/src/Primitives/Int16.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int16.z42#L40)

除法。

<a id="Std.Int16.op_Modulo(short,short)"></a>
### static override short op_Modulo(short a, short b)

[`src/libraries/z42.core/src/Primitives/Int16.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int16.z42#L42)

取模。

