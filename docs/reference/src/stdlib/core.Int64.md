<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Int64

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Primitives/Int64.z42:4`

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

### static long MaxValue

`src/libraries/z42.core/src/Primitives/Int64.z42:6`

最大值：9223372036854775807。

### static long MinValue

`src/libraries/z42.core/src/Primitives/Int64.z42:8`

最小值：-9223372036854775808。

### static extern long Parse(string s)

`src/libraries/z42.core/src/Primitives/Int64.z42:11`

解析整数字符串；格式错误抛 FormatException，越界抛 OverflowException，null 抛 ArgumentNullException。

### extern string ToString()

`src/libraries/z42.core/src/Primitives/Int64.z42:14`

返回十进制字符串表示。

### static extern bool TryParse(string s, ref long v)

`src/libraries/z42.core/src/Primitives/Int64.z42:18`

非抛出解析：成功把值写入 `v` 并返回 true；格式错误 / 越界 / null 返回 false 且 `v` 置 0。

### bool Equals(long other)

`src/libraries/z42.core/src/Primitives/Int64.z42:21`

值相等比较。

### int GetHashCode()

`src/libraries/z42.core/src/Primitives/Int64.z42:23`

返回哈希码（折叠高低 32 位，避免仅高位不同的键全撞同一哈希）。

### int CompareTo(long other)

`src/libraries/z42.core/src/Primitives/Int64.z42:26`

比较大小，返回负 / 0 / 正。

### static override long op_Add(long a, long b)

`src/libraries/z42.core/src/Primitives/Int64.z42:33`

加法。

### static override long op_Subtract(long a, long b)

`src/libraries/z42.core/src/Primitives/Int64.z42:35`

减法。

### static override long op_Multiply(long a, long b)

`src/libraries/z42.core/src/Primitives/Int64.z42:37`

乘法。

### static override long op_Divide(long a, long b)

`src/libraries/z42.core/src/Primitives/Int64.z42:39`

除法。

### static override long op_Modulo(long a, long b)

`src/libraries/z42.core/src/Primitives/Int64.z42:41`

取模。

