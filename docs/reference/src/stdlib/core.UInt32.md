<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.UInt32

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Primitives/UInt32.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt32.z42#L4)

内置 `uint` 类型（32 位无符号整数）的包装结构体，实现 IComparable / IEquatable / INumber。

```z42
public struct UInt32 : IComparable, IEquatable, INumber {
    static uint MaxValue
    static uint MinValue
    static extern uint Parse(string s)
    extern string ToString()
    static extern bool TryParse(string s, ref uint v)
    bool Equals(uint other)
    int GetHashCode()
    int CompareTo(uint other)
    static override uint op_Add(uint a, uint b)
    static override uint op_Subtract(uint a, uint b)
    static override uint op_Multiply(uint a, uint b)
    static override uint op_Divide(uint a, uint b)
    static override uint op_Modulo(uint a, uint b)
}
```

成员：[`MaxValue`](#Std.UInt32.MaxValue) · [`MinValue`](#Std.UInt32.MinValue) · [`Parse`](#Std.UInt32.Parse(string)) · [`ToString`](#Std.UInt32.ToString()) · [`TryParse`](#Std.UInt32.TryParse(string,uint)) · [`Equals`](#Std.UInt32.Equals(uint)) · [`GetHashCode`](#Std.UInt32.GetHashCode()) · [`CompareTo`](#Std.UInt32.CompareTo(uint)) · [`op_Add`](#Std.UInt32.op_Add(uint,uint)) · [`op_Subtract`](#Std.UInt32.op_Subtract(uint,uint)) · [`op_Multiply`](#Std.UInt32.op_Multiply(uint,uint)) · [`op_Divide`](#Std.UInt32.op_Divide(uint,uint)) · [`op_Modulo`](#Std.UInt32.op_Modulo(uint,uint))

<a id="Std.UInt32.MaxValue"></a>
### static uint MaxValue

[`src/libraries/z42.core/src/Primitives/UInt32.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt32.z42#L6)

最大值：4294967295。

<a id="Std.UInt32.MinValue"></a>
### static uint MinValue

[`src/libraries/z42.core/src/Primitives/UInt32.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt32.z42#L8)

最小值：0。

<a id="Std.UInt32.Parse(string)"></a>
### static extern uint Parse(string s)

[`src/libraries/z42.core/src/Primitives/UInt32.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt32.z42#L11)

解析无符号整数字符串；格式错误抛 FormatException，越界抛 OverflowException，null 抛 ArgumentNullException。

<a id="Std.UInt32.ToString()"></a>
### extern string ToString()

[`src/libraries/z42.core/src/Primitives/UInt32.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt32.z42#L14)

返回十进制字符串表示。

<a id="Std.UInt32.TryParse(string,uint)"></a>
### static extern bool TryParse(string s, ref uint v)

[`src/libraries/z42.core/src/Primitives/UInt32.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt32.z42#L17)

非抛出解析：成功把值写入 `v` 并返回 true；格式错误 / 越界 / null 返回 false 且 `v` 置 0。

<a id="Std.UInt32.Equals(uint)"></a>
### bool Equals(uint other)

[`src/libraries/z42.core/src/Primitives/UInt32.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt32.z42#L20)

值相等比较。

<a id="Std.UInt32.GetHashCode()"></a>
### int GetHashCode()

[`src/libraries/z42.core/src/Primitives/UInt32.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt32.z42#L22)

返回哈希码。

<a id="Std.UInt32.CompareTo(uint)"></a>
### int CompareTo(uint other)

[`src/libraries/z42.core/src/Primitives/UInt32.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt32.z42#L25)

比较大小，返回负 / 0 / 正。

<a id="Std.UInt32.op_Add(uint,uint)"></a>
### static override uint op_Add(uint a, uint b)

[`src/libraries/z42.core/src/Primitives/UInt32.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt32.z42#L32)

加法。

<a id="Std.UInt32.op_Subtract(uint,uint)"></a>
### static override uint op_Subtract(uint a, uint b)

[`src/libraries/z42.core/src/Primitives/UInt32.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt32.z42#L34)

减法。

<a id="Std.UInt32.op_Multiply(uint,uint)"></a>
### static override uint op_Multiply(uint a, uint b)

[`src/libraries/z42.core/src/Primitives/UInt32.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt32.z42#L36)

乘法。

<a id="Std.UInt32.op_Divide(uint,uint)"></a>
### static override uint op_Divide(uint a, uint b)

[`src/libraries/z42.core/src/Primitives/UInt32.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt32.z42#L38)

除法。

<a id="Std.UInt32.op_Modulo(uint,uint)"></a>
### static override uint op_Modulo(uint a, uint b)

[`src/libraries/z42.core/src/Primitives/UInt32.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/UInt32.z42#L40)

取模。

