<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.SByte

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Primitives/SByte.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/SByte.z42#L4)

内置 `sbyte` 类型（8 位有符号整数）的包装结构体，实现 IComparable / IEquatable / INumber。

```z42
public struct SByte : IComparable, IEquatable, INumber {
    static sbyte MaxValue
    static sbyte MinValue
    static extern sbyte Parse(string s)
    extern string ToString()
    static extern bool TryParse(string s, ref sbyte v)
    bool Equals(sbyte other)
    int GetHashCode()
    int CompareTo(sbyte other)
    static override sbyte op_Add(sbyte a, sbyte b)
    static override sbyte op_Subtract(sbyte a, sbyte b)
    static override sbyte op_Multiply(sbyte a, sbyte b)
    static override sbyte op_Divide(sbyte a, sbyte b)
    static override sbyte op_Modulo(sbyte a, sbyte b)
}
```

成员：[`MaxValue`](#Std.SByte.MaxValue) · [`MinValue`](#Std.SByte.MinValue) · [`Parse`](#Std.SByte.Parse(string)) · [`ToString`](#Std.SByte.ToString()) · [`TryParse`](#Std.SByte.TryParse(string,sbyte)) · [`Equals`](#Std.SByte.Equals(sbyte)) · [`GetHashCode`](#Std.SByte.GetHashCode()) · [`CompareTo`](#Std.SByte.CompareTo(sbyte)) · [`op_Add`](#Std.SByte.op_Add(sbyte,sbyte)) · [`op_Subtract`](#Std.SByte.op_Subtract(sbyte,sbyte)) · [`op_Multiply`](#Std.SByte.op_Multiply(sbyte,sbyte)) · [`op_Divide`](#Std.SByte.op_Divide(sbyte,sbyte)) · [`op_Modulo`](#Std.SByte.op_Modulo(sbyte,sbyte))

<a id="Std.SByte.MaxValue"></a>
### static sbyte MaxValue

[`src/libraries/z42.core/src/Primitives/SByte.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/SByte.z42#L6)

最大值：127。

<a id="Std.SByte.MinValue"></a>
### static sbyte MinValue

[`src/libraries/z42.core/src/Primitives/SByte.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/SByte.z42#L8)

最小值：-128。

<a id="Std.SByte.Parse(string)"></a>
### static extern sbyte Parse(string s)

[`src/libraries/z42.core/src/Primitives/SByte.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/SByte.z42#L13)

解析整数字符串。

格式错误抛 FormatException，越界抛 OverflowException，null 抛 ArgumentNullException。

<a id="Std.SByte.ToString()"></a>
### extern string ToString()

[`src/libraries/z42.core/src/Primitives/SByte.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/SByte.z42#L16)

返回十进制字符串表示。

<a id="Std.SByte.TryParse(string,sbyte)"></a>
### static extern bool TryParse(string s, ref sbyte v)

[`src/libraries/z42.core/src/Primitives/SByte.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/SByte.z42#L19)

非抛出解析：成功把值写入 `v` 并返回 true；格式错误 / 越界 / null 返回 false 且 `v` 置 0。

<a id="Std.SByte.Equals(sbyte)"></a>
### bool Equals(sbyte other)

[`src/libraries/z42.core/src/Primitives/SByte.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/SByte.z42#L22)

值相等比较。

<a id="Std.SByte.GetHashCode()"></a>
### int GetHashCode()

[`src/libraries/z42.core/src/Primitives/SByte.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/SByte.z42#L24)

返回哈希码。

<a id="Std.SByte.CompareTo(sbyte)"></a>
### int CompareTo(sbyte other)

[`src/libraries/z42.core/src/Primitives/SByte.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/SByte.z42#L27)

比较大小，返回负 / 0 / 正。

<a id="Std.SByte.op_Add(sbyte,sbyte)"></a>
### static override sbyte op_Add(sbyte a, sbyte b)

[`src/libraries/z42.core/src/Primitives/SByte.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/SByte.z42#L34)

加法。

<a id="Std.SByte.op_Subtract(sbyte,sbyte)"></a>
### static override sbyte op_Subtract(sbyte a, sbyte b)

[`src/libraries/z42.core/src/Primitives/SByte.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/SByte.z42#L36)

减法。

<a id="Std.SByte.op_Multiply(sbyte,sbyte)"></a>
### static override sbyte op_Multiply(sbyte a, sbyte b)

[`src/libraries/z42.core/src/Primitives/SByte.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/SByte.z42#L38)

乘法。

<a id="Std.SByte.op_Divide(sbyte,sbyte)"></a>
### static override sbyte op_Divide(sbyte a, sbyte b)

[`src/libraries/z42.core/src/Primitives/SByte.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/SByte.z42#L40)

除法。

<a id="Std.SByte.op_Modulo(sbyte,sbyte)"></a>
### static override sbyte op_Modulo(sbyte a, sbyte b)

[`src/libraries/z42.core/src/Primitives/SByte.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/SByte.z42#L42)

取模。

