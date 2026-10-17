<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Byte

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Primitives/Byte.z42:4`

内置 `byte` 类型（8 位无符号整数）的包装结构体，实现 IComparable / IEquatable / INumber。

```z42
public struct Byte : IComparable, IEquatable, INumber {
    static byte MaxValue
    static byte MinValue
    static extern byte Parse(string s)
    extern string ToString()
    static extern bool TryParse(string s, ref byte v)
    bool Equals(byte other)
    int GetHashCode()
    int CompareTo(byte other)
    static override byte op_Add(byte a, byte b)
    static override byte op_Subtract(byte a, byte b)
    static override byte op_Multiply(byte a, byte b)
    static override byte op_Divide(byte a, byte b)
    static override byte op_Modulo(byte a, byte b)
}
```

### static byte MaxValue

`src/libraries/z42.core/src/Primitives/Byte.z42:6`

最大值：255。

### static byte MinValue

`src/libraries/z42.core/src/Primitives/Byte.z42:8`

最小值：0。

### static extern byte Parse(string s)

`src/libraries/z42.core/src/Primitives/Byte.z42:11`

解析无符号整数字符串；格式错误抛 FormatException，越界抛 OverflowException，null 抛 ArgumentNullException。

### extern string ToString()

`src/libraries/z42.core/src/Primitives/Byte.z42:14`

返回十进制字符串表示。

### static extern bool TryParse(string s, ref byte v)

`src/libraries/z42.core/src/Primitives/Byte.z42:17`

非抛出解析：成功把值写入 `v` 并返回 true；格式错误 / 越界 / null 返回 false 且 `v` 置 0。

### bool Equals(byte other)

`src/libraries/z42.core/src/Primitives/Byte.z42:20`

值相等比较。

### int GetHashCode()

`src/libraries/z42.core/src/Primitives/Byte.z42:22`

返回哈希码。

### int CompareTo(byte other)

`src/libraries/z42.core/src/Primitives/Byte.z42:25`

比较大小，返回负 / 0 / 正。

### static override byte op_Add(byte a, byte b)

`src/libraries/z42.core/src/Primitives/Byte.z42:32`

加法。

### static override byte op_Subtract(byte a, byte b)

`src/libraries/z42.core/src/Primitives/Byte.z42:34`

减法。

### static override byte op_Multiply(byte a, byte b)

`src/libraries/z42.core/src/Primitives/Byte.z42:36`

乘法。

### static override byte op_Divide(byte a, byte b)

`src/libraries/z42.core/src/Primitives/Byte.z42:38`

除法。

### static override byte op_Modulo(byte a, byte b)

`src/libraries/z42.core/src/Primitives/Byte.z42:40`

取模。

