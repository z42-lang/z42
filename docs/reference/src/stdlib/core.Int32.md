<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Int32

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Primitives/Int32.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int32.z42#L4)

内置 `int` 类型（32 位有符号整数）的包装结构体，实现 IComparable / IEquatable / INumber。

```z42
public struct Int32 : IComparable, IEquatable, INumber {
    static int MaxValue
    static int MinValue
    static extern int Parse(string s)
    extern string ToString()
    static extern bool TryParse(string s, ref int v)
    bool Equals(int other)
    int GetHashCode()
    int CompareTo(int other)
    static override int op_Add(int a, int b)
    static override int op_Subtract(int a, int b)
    static override int op_Multiply(int a, int b)
    static override int op_Divide(int a, int b)
    static override int op_Modulo(int a, int b)
}
```

成员：[`MaxValue`](#Std.Int32.MaxValue) · [`MinValue`](#Std.Int32.MinValue) · [`Parse`](#Std.Int32.Parse(string)) · [`ToString`](#Std.Int32.ToString()) · [`TryParse`](#Std.Int32.TryParse(string,int)) · [`Equals`](#Std.Int32.Equals(int)) · [`GetHashCode`](#Std.Int32.GetHashCode()) · [`CompareTo`](#Std.Int32.CompareTo(int)) · [`op_Add`](#Std.Int32.op_Add(int,int)) · [`op_Subtract`](#Std.Int32.op_Subtract(int,int)) · [`op_Multiply`](#Std.Int32.op_Multiply(int,int)) · [`op_Divide`](#Std.Int32.op_Divide(int,int)) · [`op_Modulo`](#Std.Int32.op_Modulo(int,int))

<a id="Std.Int32.MaxValue"></a>
### static int MaxValue

[`src/libraries/z42.core/src/Primitives/Int32.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int32.z42#L6)

最大值：2147483647。

<a id="Std.Int32.MinValue"></a>
### static int MinValue

[`src/libraries/z42.core/src/Primitives/Int32.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int32.z42#L8)

最小值：-2147483648。

<a id="Std.Int32.Parse(string)"></a>
### static extern int Parse(string s)

[`src/libraries/z42.core/src/Primitives/Int32.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int32.z42#L13)

解析十进制整数字符串。

格式错误抛 FormatException，越界抛 OverflowException，null 抛 ArgumentNullException。

<a id="Std.Int32.ToString()"></a>
### extern string ToString()

[`src/libraries/z42.core/src/Primitives/Int32.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int32.z42#L15)

返回十进制字符串表示。

<a id="Std.Int32.TryParse(string,int)"></a>
### static extern bool TryParse(string s, ref int v)

[`src/libraries/z42.core/src/Primitives/Int32.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int32.z42#L19)

非抛出解析：成功把值写入 `v` 并返回 true；格式错误 / 越界 / null 返回 false 且 `v` 置 0。

<a id="Std.Int32.Equals(int)"></a>
### bool Equals(int other)

[`src/libraries/z42.core/src/Primitives/Int32.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int32.z42#L22)

值相等比较。

<a id="Std.Int32.GetHashCode()"></a>
### int GetHashCode()

[`src/libraries/z42.core/src/Primitives/Int32.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int32.z42#L24)

返回哈希码（int 的哈希是自身）。

<a id="Std.Int32.CompareTo(int)"></a>
### int CompareTo(int other)

[`src/libraries/z42.core/src/Primitives/Int32.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int32.z42#L27)

比较大小，返回负 / 0 / 正。

<a id="Std.Int32.op_Add(int,int)"></a>
### static override int op_Add(int a, int b)

[`src/libraries/z42.core/src/Primitives/Int32.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int32.z42#L35)

加法。

<a id="Std.Int32.op_Subtract(int,int)"></a>
### static override int op_Subtract(int a, int b)

[`src/libraries/z42.core/src/Primitives/Int32.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int32.z42#L37)

减法。

<a id="Std.Int32.op_Multiply(int,int)"></a>
### static override int op_Multiply(int a, int b)

[`src/libraries/z42.core/src/Primitives/Int32.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int32.z42#L39)

乘法。

<a id="Std.Int32.op_Divide(int,int)"></a>
### static override int op_Divide(int a, int b)

[`src/libraries/z42.core/src/Primitives/Int32.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int32.z42#L41)

除法。

<a id="Std.Int32.op_Modulo(int,int)"></a>
### static override int op_Modulo(int a, int b)

[`src/libraries/z42.core/src/Primitives/Int32.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int32.z42#L43)

取模。

