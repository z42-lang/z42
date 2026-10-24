<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.UInt16

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Primitives/UInt16.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt16.z42#L4)

内置 `ushort` 类型（16 位无符号整数）的包装结构体，实现 IComparable / IEquatable / INumber。

```z42
public struct UInt16 : IComparable, IEquatable, INumber {
    static ushort MaxValue
    static ushort MinValue
    static extern ushort Parse(string s)
    extern string ToString()
    static extern bool TryParse(string s, ref ushort v)
    bool Equals(ushort other)
    int GetHashCode()
    int CompareTo(ushort other)
    static override ushort op_Add(ushort a, ushort b)
    static override ushort op_Subtract(ushort a, ushort b)
    static override ushort op_Multiply(ushort a, ushort b)
    static override ushort op_Divide(ushort a, ushort b)
    static override ushort op_Modulo(ushort a, ushort b)
}
```

成员：[`MaxValue`](#Std.UInt16.MaxValue) · [`MinValue`](#Std.UInt16.MinValue) · [`Parse`](#Std.UInt16.Parse(string)) · [`ToString`](#Std.UInt16.ToString()) · [`TryParse`](#Std.UInt16.TryParse(string,ushort)) · [`Equals`](#Std.UInt16.Equals(ushort)) · [`GetHashCode`](#Std.UInt16.GetHashCode()) · [`CompareTo`](#Std.UInt16.CompareTo(ushort)) · [`op_Add`](#Std.UInt16.op_Add(ushort,ushort)) · [`op_Subtract`](#Std.UInt16.op_Subtract(ushort,ushort)) · [`op_Multiply`](#Std.UInt16.op_Multiply(ushort,ushort)) · [`op_Divide`](#Std.UInt16.op_Divide(ushort,ushort)) · [`op_Modulo`](#Std.UInt16.op_Modulo(ushort,ushort))

<a id="Std.UInt16.MaxValue"></a>
### static ushort MaxValue

[`src/libraries/z42.core/src/Primitives/UInt16.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt16.z42#L6)

最大值：65535。

<a id="Std.UInt16.MinValue"></a>
### static ushort MinValue

[`src/libraries/z42.core/src/Primitives/UInt16.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt16.z42#L8)

最小值：0。

<a id="Std.UInt16.Parse(string)"></a>
### static extern ushort Parse(string s)

[`src/libraries/z42.core/src/Primitives/UInt16.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt16.z42#L13)

解析无符号整数字符串。

格式错误抛 FormatException，越界抛 OverflowException，null 抛 ArgumentNullException。

<a id="Std.UInt16.ToString()"></a>
### extern string ToString()

[`src/libraries/z42.core/src/Primitives/UInt16.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt16.z42#L16)

返回十进制字符串表示。

<a id="Std.UInt16.TryParse(string,ushort)"></a>
### static extern bool TryParse(string s, ref ushort v)

[`src/libraries/z42.core/src/Primitives/UInt16.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt16.z42#L19)

非抛出解析：成功把值写入 `v` 并返回 true；格式错误 / 越界 / null 返回 false 且 `v` 置 0。

<a id="Std.UInt16.Equals(ushort)"></a>
### bool Equals(ushort other)

[`src/libraries/z42.core/src/Primitives/UInt16.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt16.z42#L22)

值相等比较。

<a id="Std.UInt16.GetHashCode()"></a>
### int GetHashCode()

[`src/libraries/z42.core/src/Primitives/UInt16.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt16.z42#L24)

返回哈希码。

<a id="Std.UInt16.CompareTo(ushort)"></a>
### int CompareTo(ushort other)

[`src/libraries/z42.core/src/Primitives/UInt16.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt16.z42#L27)

比较大小，返回负 / 0 / 正。

<a id="Std.UInt16.op_Add(ushort,ushort)"></a>
### static override ushort op_Add(ushort a, ushort b)

[`src/libraries/z42.core/src/Primitives/UInt16.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt16.z42#L34)

加法。

<a id="Std.UInt16.op_Subtract(ushort,ushort)"></a>
### static override ushort op_Subtract(ushort a, ushort b)

[`src/libraries/z42.core/src/Primitives/UInt16.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt16.z42#L36)

减法。

<a id="Std.UInt16.op_Multiply(ushort,ushort)"></a>
### static override ushort op_Multiply(ushort a, ushort b)

[`src/libraries/z42.core/src/Primitives/UInt16.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt16.z42#L38)

乘法。

<a id="Std.UInt16.op_Divide(ushort,ushort)"></a>
### static override ushort op_Divide(ushort a, ushort b)

[`src/libraries/z42.core/src/Primitives/UInt16.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt16.z42#L40)

除法。

<a id="Std.UInt16.op_Modulo(ushort,ushort)"></a>
### static override ushort op_Modulo(ushort a, ushort b)

[`src/libraries/z42.core/src/Primitives/UInt16.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt16.z42#L42)

取模。

