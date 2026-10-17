<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.UInt64

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Primitives/UInt64.z42:5`

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

### static ulong MaxValue

`src/libraries/z42.core/src/Primitives/UInt64.z42:7`

最大值：18446744073709551615（全 1 位）。

### static ulong MinValue

`src/libraries/z42.core/src/Primitives/UInt64.z42:9`

最小值：0。

### static extern ulong Parse(string s)

`src/libraries/z42.core/src/Primitives/UInt64.z42:13`

解析无符号整数字符串；格式错误抛 FormatException，越界抛 OverflowException， null 抛 ArgumentNullException。

### extern string ToString()

`src/libraries/z42.core/src/Primitives/UInt64.z42:15`

返回十进制字符串表示。

### static extern bool TryParse(string s, ref ulong v)

`src/libraries/z42.core/src/Primitives/UInt64.z42:19`

非抛出解析：成功把值写入 `v` 并返回 true；格式错误 / 越界 / null 返回 false 且 `v` 置 0。

### bool Equals(ulong other)

`src/libraries/z42.core/src/Primitives/UInt64.z42:22`

值相等比较。

### int GetHashCode()

`src/libraries/z42.core/src/Primitives/UInt64.z42:24`

返回哈希码（折叠高低 32 位，同 Int64）。

### int CompareTo(ulong other)

`src/libraries/z42.core/src/Primitives/UInt64.z42:27`

比较大小，返回负 / 0 / 正。

### static override ulong op_Add(ulong a, ulong b)

`src/libraries/z42.core/src/Primitives/UInt64.z42:34`

加法。

### static override ulong op_Subtract(ulong a, ulong b)

`src/libraries/z42.core/src/Primitives/UInt64.z42:36`

减法。

### static override ulong op_Multiply(ulong a, ulong b)

`src/libraries/z42.core/src/Primitives/UInt64.z42:38`

乘法。

### static override ulong op_Divide(ulong a, ulong b)

`src/libraries/z42.core/src/Primitives/UInt64.z42:40`

除法。

### static override ulong op_Modulo(ulong a, ulong b)

`src/libraries/z42.core/src/Primitives/UInt64.z42:42`

取模。

