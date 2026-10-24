<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Int64

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Primitives/Int64.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int64.z42#L4)

内置 `long` 类型（64 位有符号整数）的包装结构体，实现 IComparable / IEquatable / INumber。

```z42
public struct Int64 : IComparable, IEquatable, INumber {
    static long MaxValue
    static long MinValue
    static extern long Parse(string s)
    extern string ToString()
    static extern bool TryParse(string s, ref long v)
    bool Equals(long other)
    int GetHashCode()
    int CompareTo(long other)
    static override long op_Add(long a, long b)
    static override long op_Subtract(long a, long b)
    static override long op_Multiply(long a, long b)
    static override long op_Divide(long a, long b)
    static override long op_Modulo(long a, long b)
}
```

成员：[`MaxValue`](#Std.Int64.MaxValue) · [`MinValue`](#Std.Int64.MinValue) · [`Parse`](#Std.Int64.Parse(string)) · [`ToString`](#Std.Int64.ToString()) · [`TryParse`](#Std.Int64.TryParse(string,long)) · [`Equals`](#Std.Int64.Equals(long)) · [`GetHashCode`](#Std.Int64.GetHashCode()) · [`CompareTo`](#Std.Int64.CompareTo(long)) · [`op_Add`](#Std.Int64.op_Add(long,long)) · [`op_Subtract`](#Std.Int64.op_Subtract(long,long)) · [`op_Multiply`](#Std.Int64.op_Multiply(long,long)) · [`op_Divide`](#Std.Int64.op_Divide(long,long)) · [`op_Modulo`](#Std.Int64.op_Modulo(long,long))

<a id="Std.Int64.MaxValue"></a>
### static long MaxValue

[`src/libraries/z42.core/src/Primitives/Int64.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int64.z42#L6)

最大值：9223372036854775807。

<a id="Std.Int64.MinValue"></a>
### static long MinValue

[`src/libraries/z42.core/src/Primitives/Int64.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int64.z42#L8)

最小值：-9223372036854775808。

<a id="Std.Int64.Parse(string)"></a>
### static extern long Parse(string s)

[`src/libraries/z42.core/src/Primitives/Int64.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int64.z42#L13)

解析整数字符串。

格式错误抛 FormatException，越界抛 OverflowException，null 抛 ArgumentNullException。

<a id="Std.Int64.ToString()"></a>
### extern string ToString()

[`src/libraries/z42.core/src/Primitives/Int64.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int64.z42#L16)

返回十进制字符串表示。

<a id="Std.Int64.TryParse(string,long)"></a>
### static extern bool TryParse(string s, ref long v)

[`src/libraries/z42.core/src/Primitives/Int64.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int64.z42#L20)

非抛出解析：成功把值写入 `v` 并返回 true；格式错误 / 越界 / null 返回 false 且 `v` 置 0。

<a id="Std.Int64.Equals(long)"></a>
### bool Equals(long other)

[`src/libraries/z42.core/src/Primitives/Int64.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int64.z42#L23)

值相等比较。

<a id="Std.Int64.GetHashCode()"></a>
### int GetHashCode()

[`src/libraries/z42.core/src/Primitives/Int64.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int64.z42#L25)

返回哈希码（折叠高低 32 位，避免仅高位不同的键全撞同一哈希）。

<a id="Std.Int64.CompareTo(long)"></a>
### int CompareTo(long other)

[`src/libraries/z42.core/src/Primitives/Int64.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int64.z42#L28)

比较大小，返回负 / 0 / 正。

<a id="Std.Int64.op_Add(long,long)"></a>
### static override long op_Add(long a, long b)

[`src/libraries/z42.core/src/Primitives/Int64.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int64.z42#L35)

加法。

<a id="Std.Int64.op_Subtract(long,long)"></a>
### static override long op_Subtract(long a, long b)

[`src/libraries/z42.core/src/Primitives/Int64.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int64.z42#L37)

减法。

<a id="Std.Int64.op_Multiply(long,long)"></a>
### static override long op_Multiply(long a, long b)

[`src/libraries/z42.core/src/Primitives/Int64.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int64.z42#L39)

乘法。

<a id="Std.Int64.op_Divide(long,long)"></a>
### static override long op_Divide(long a, long b)

[`src/libraries/z42.core/src/Primitives/Int64.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int64.z42#L41)

除法。

<a id="Std.Int64.op_Modulo(long,long)"></a>
### static override long op_Modulo(long a, long b)

[`src/libraries/z42.core/src/Primitives/Int64.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Int64.z42#L43)

取模。

