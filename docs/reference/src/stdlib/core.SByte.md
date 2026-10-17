<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.SByte

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Primitives/SByte.z42:4`

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

### static sbyte MaxValue

`src/libraries/z42.core/src/Primitives/SByte.z42:6`

最大值：127。

### static sbyte MinValue

`src/libraries/z42.core/src/Primitives/SByte.z42:8`

最小值：-128。

### static extern sbyte Parse(string s)

`src/libraries/z42.core/src/Primitives/SByte.z42:11`

解析整数字符串；格式错误抛 FormatException，越界抛 OverflowException，null 抛 ArgumentNullException。

### extern string ToString()

`src/libraries/z42.core/src/Primitives/SByte.z42:14`

返回十进制字符串表示。

### static extern bool TryParse(string s, ref sbyte v)

`src/libraries/z42.core/src/Primitives/SByte.z42:17`

非抛出解析：成功把值写入 `v` 并返回 true；格式错误 / 越界 / null 返回 false 且 `v` 置 0。

### bool Equals(sbyte other)

`src/libraries/z42.core/src/Primitives/SByte.z42:20`

值相等比较。

### int GetHashCode()

`src/libraries/z42.core/src/Primitives/SByte.z42:22`

返回哈希码。

### int CompareTo(sbyte other)

`src/libraries/z42.core/src/Primitives/SByte.z42:25`

比较大小，返回负 / 0 / 正。

### static override sbyte op_Add(sbyte a, sbyte b)

`src/libraries/z42.core/src/Primitives/SByte.z42:32`

加法。

### static override sbyte op_Subtract(sbyte a, sbyte b)

`src/libraries/z42.core/src/Primitives/SByte.z42:34`

减法。

### static override sbyte op_Multiply(sbyte a, sbyte b)

`src/libraries/z42.core/src/Primitives/SByte.z42:36`

乘法。

### static override sbyte op_Divide(sbyte a, sbyte b)

`src/libraries/z42.core/src/Primitives/SByte.z42:38`

除法。

### static override sbyte op_Modulo(sbyte a, sbyte b)

`src/libraries/z42.core/src/Primitives/SByte.z42:40`

取模。

