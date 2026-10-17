<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Double

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Primitives/Double.z42:4`

内置 `double` 类型（64 位 IEEE-754 浮点）的包装结构体，实现 IComparable / IEquatable / INumber。

```z42
public struct Double : IComparable, IEquatable, INumber {
    static double MaxValue
    static double MinValue
    static double Epsilon
    static double NaN
    static double PositiveInfinity
    static double NegativeInfinity
    static extern double Parse(string s)
    extern string ToString()
    static extern bool TryParse(string s, ref double v)
    static bool IsNaN(double v)
    static bool IsInfinity(double v)
    static bool IsPositiveInfinity(double v)
    static bool IsNegativeInfinity(double v)
    static bool IsFinite(double v)
    bool Equals(double other)
    int GetHashCode()
    int CompareTo(double other)
    static override double op_Add(double a, double b)
    static override double op_Subtract(double a, double b)
    static override double op_Multiply(double a, double b)
    static override double op_Divide(double a, double b)
    static override double op_Modulo(double a, double b)
}
```

### static double MaxValue

`src/libraries/z42.core/src/Primitives/Double.z42:6`

最大有限值。

### static double MinValue

`src/libraries/z42.core/src/Primitives/Double.z42:8`

最小有限值。

### static double Epsilon

`src/libraries/z42.core/src/Primitives/Double.z42:10`

最小正数（最小可表示的正规 / 次正规差值）。

### static double NaN

`src/libraries/z42.core/src/Primitives/Double.z42:13`

非数值（NaN）。

### static double PositiveInfinity

`src/libraries/z42.core/src/Primitives/Double.z42:15`

正无穷。

### static double NegativeInfinity

`src/libraries/z42.core/src/Primitives/Double.z42:17`

负无穷。

### static extern double Parse(string s)

`src/libraries/z42.core/src/Primitives/Double.z42:20`

解析浮点字符串；格式错误抛 FormatException，越界抛 OverflowException，null 抛 ArgumentNullException。

### extern string ToString()

`src/libraries/z42.core/src/Primitives/Double.z42:22`

返回字符串表示。

### static extern bool TryParse(string s, ref double v)

`src/libraries/z42.core/src/Primitives/Double.z42:26`

非抛出解析：成功把值写入 `v` 并返回 true；格式错误 / 越界 / null 返回 false 且 `v` 置 0.0。

### static bool IsNaN(double v)

`src/libraries/z42.core/src/Primitives/Double.z42:29`

v 是否为 NaN（NaN 是唯一不等于自身的值）。

### static bool IsInfinity(double v)

`src/libraries/z42.core/src/Primitives/Double.z42:32`

v 是否为正无穷或负无穷。

### static bool IsPositiveInfinity(double v)

`src/libraries/z42.core/src/Primitives/Double.z42:36`

v 是否为正无穷。

### static bool IsNegativeInfinity(double v)

`src/libraries/z42.core/src/Primitives/Double.z42:38`

v 是否为负无穷。

### static bool IsFinite(double v)

`src/libraries/z42.core/src/Primitives/Double.z42:40`

v 是否为有限值（非 NaN 且非无穷）。

### bool Equals(double other)

`src/libraries/z42.core/src/Primitives/Double.z42:43`

值相等比较（NaN != NaN）。

### int GetHashCode()

`src/libraries/z42.core/src/Primitives/Double.z42:45`

返回哈希码（折叠 IEEE-754 位模式高低 32 位）。

### int CompareTo(double other)

`src/libraries/z42.core/src/Primitives/Double.z42:51`

比较大小，返回负 / 0 / 正。

### static override double op_Add(double a, double b)

`src/libraries/z42.core/src/Primitives/Double.z42:67`

加法。

### static override double op_Subtract(double a, double b)

`src/libraries/z42.core/src/Primitives/Double.z42:69`

减法。

### static override double op_Multiply(double a, double b)

`src/libraries/z42.core/src/Primitives/Double.z42:71`

乘法。

### static override double op_Divide(double a, double b)

`src/libraries/z42.core/src/Primitives/Double.z42:73`

除法。

### static override double op_Modulo(double a, double b)

`src/libraries/z42.core/src/Primitives/Double.z42:75`

取模。

