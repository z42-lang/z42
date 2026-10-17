<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.UInt16

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Primitives/UInt16.z42:4`

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

### static ushort MaxValue

`src/libraries/z42.core/src/Primitives/UInt16.z42:6`

最大值：65535。

### static ushort MinValue

`src/libraries/z42.core/src/Primitives/UInt16.z42:8`

最小值：0。

### static extern ushort Parse(string s)

`src/libraries/z42.core/src/Primitives/UInt16.z42:11`

解析无符号整数字符串；格式错误抛 FormatException，越界抛 OverflowException，null 抛 ArgumentNullException。

### extern string ToString()

`src/libraries/z42.core/src/Primitives/UInt16.z42:14`

返回十进制字符串表示。

### static extern bool TryParse(string s, ref ushort v)

`src/libraries/z42.core/src/Primitives/UInt16.z42:17`

非抛出解析：成功把值写入 `v` 并返回 true；格式错误 / 越界 / null 返回 false 且 `v` 置 0。

### bool Equals(ushort other)

`src/libraries/z42.core/src/Primitives/UInt16.z42:20`

值相等比较。

### int GetHashCode()

`src/libraries/z42.core/src/Primitives/UInt16.z42:22`

返回哈希码。

### int CompareTo(ushort other)

`src/libraries/z42.core/src/Primitives/UInt16.z42:25`

比较大小，返回负 / 0 / 正。

### static override ushort op_Add(ushort a, ushort b)

`src/libraries/z42.core/src/Primitives/UInt16.z42:32`

加法。

### static override ushort op_Subtract(ushort a, ushort b)

`src/libraries/z42.core/src/Primitives/UInt16.z42:34`

减法。

### static override ushort op_Multiply(ushort a, ushort b)

`src/libraries/z42.core/src/Primitives/UInt16.z42:36`

乘法。

### static override ushort op_Divide(ushort a, ushort b)

`src/libraries/z42.core/src/Primitives/UInt16.z42:38`

除法。

### static override ushort op_Modulo(ushort a, ushort b)

`src/libraries/z42.core/src/Primitives/UInt16.z42:40`

取模。

