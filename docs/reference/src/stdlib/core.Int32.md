<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Int32

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Primitives/Int32.z42:4`

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

### static int MaxValue

`src/libraries/z42.core/src/Primitives/Int32.z42:6`

最大值：2147483647。

### static int MinValue

`src/libraries/z42.core/src/Primitives/Int32.z42:8`

最小值：-2147483648。

### static extern int Parse(string s)

`src/libraries/z42.core/src/Primitives/Int32.z42:12`

解析十进制整数字符串；格式错误抛 FormatException，越界抛 OverflowException， null 抛 ArgumentNullException。

### extern string ToString()

`src/libraries/z42.core/src/Primitives/Int32.z42:14`

返回十进制字符串表示。

### static extern bool TryParse(string s, ref int v)

`src/libraries/z42.core/src/Primitives/Int32.z42:18`

非抛出解析：成功把值写入 `v` 并返回 true；格式错误 / 越界 / null 返回 false 且 `v` 置 0。

### bool Equals(int other)

`src/libraries/z42.core/src/Primitives/Int32.z42:21`

值相等比较。

### int GetHashCode()

`src/libraries/z42.core/src/Primitives/Int32.z42:23`

返回哈希码（int 的哈希是自身）。

### int CompareTo(int other)

`src/libraries/z42.core/src/Primitives/Int32.z42:26`

比较大小，返回负 / 0 / 正。

### static override int op_Add(int a, int b)

`src/libraries/z42.core/src/Primitives/Int32.z42:34`

加法。

### static override int op_Subtract(int a, int b)

`src/libraries/z42.core/src/Primitives/Int32.z42:36`

减法。

### static override int op_Multiply(int a, int b)

`src/libraries/z42.core/src/Primitives/Int32.z42:38`

乘法。

### static override int op_Divide(int a, int b)

`src/libraries/z42.core/src/Primitives/Int32.z42:40`

除法。

### static override int op_Modulo(int a, int b)

`src/libraries/z42.core/src/Primitives/Int32.z42:42`

取模。

