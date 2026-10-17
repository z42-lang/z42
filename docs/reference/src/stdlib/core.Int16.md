<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Int16

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Primitives/Int16.z42:4`

内置 `short` 类型（16 位有符号整数）的包装结构体，实现 IComparable / IEquatable / INumber。

```z42
public struct Int16 : IComparable, IEquatable, INumber {
    static short MaxValue
    static short MinValue
    static extern short Parse(string s)
    extern string ToString()
    static extern bool TryParse(string s, ref short v)
    bool Equals(short other)
    int GetHashCode()
    int CompareTo(short other)
    static override short op_Add(short a, short b)
    static override short op_Subtract(short a, short b)
    static override short op_Multiply(short a, short b)
    static override short op_Divide(short a, short b)
    static override short op_Modulo(short a, short b)
}
```

### static short MaxValue

`src/libraries/z42.core/src/Primitives/Int16.z42:6`

最大值：32767。

### static short MinValue

`src/libraries/z42.core/src/Primitives/Int16.z42:8`

最小值：-32768。

### static extern short Parse(string s)

`src/libraries/z42.core/src/Primitives/Int16.z42:11`

解析整数字符串；格式错误抛 FormatException，越界抛 OverflowException，null 抛 ArgumentNullException。

### extern string ToString()

`src/libraries/z42.core/src/Primitives/Int16.z42:14`

返回十进制字符串表示。

### static extern bool TryParse(string s, ref short v)

`src/libraries/z42.core/src/Primitives/Int16.z42:17`

非抛出解析：成功把值写入 `v` 并返回 true；格式错误 / 越界 / null 返回 false 且 `v` 置 0。

### bool Equals(short other)

`src/libraries/z42.core/src/Primitives/Int16.z42:20`

值相等比较。

### int GetHashCode()

`src/libraries/z42.core/src/Primitives/Int16.z42:22`

返回哈希码。

### int CompareTo(short other)

`src/libraries/z42.core/src/Primitives/Int16.z42:25`

比较大小，返回负 / 0 / 正。

### static override short op_Add(short a, short b)

`src/libraries/z42.core/src/Primitives/Int16.z42:32`

加法。

### static override short op_Subtract(short a, short b)

`src/libraries/z42.core/src/Primitives/Int16.z42:34`

减法。

### static override short op_Multiply(short a, short b)

`src/libraries/z42.core/src/Primitives/Int16.z42:36`

乘法。

### static override short op_Divide(short a, short b)

`src/libraries/z42.core/src/Primitives/Int16.z42:38`

除法。

### static override short op_Modulo(short a, short b)

`src/libraries/z42.core/src/Primitives/Int16.z42:40`

取模。

