<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Single

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Primitives/Single.z42:4`

内置 `float` 类型（32 位 IEEE-754 浮点）的包装结构体，实现 IComparable / IEquatable / INumber。

```z42
public struct Single : IComparable, IEquatable, INumber {
    static float MaxValue
    static float MinValue
    static float Epsilon
    static float NaN
    static float PositiveInfinity
    static float NegativeInfinity
    static float Parse(string s)
    static bool TryParse(string s, ref float v)
    static bool IsNaN(float v)
    static bool IsInfinity(float v)
    static bool IsPositiveInfinity(float v)
    static bool IsNegativeInfinity(float v)
    static bool IsFinite(float v)
    extern string ToString()
    bool Equals(float other)
    int GetHashCode()
    int CompareTo(float other)
    static override float op_Add(float a, float b)
    static override float op_Subtract(float a, float b)
    static override float op_Multiply(float a, float b)
    static override float op_Divide(float a, float b)
    static override float op_Modulo(float a, float b)
}
```

### static float MaxValue

`src/libraries/z42.core/src/Primitives/Single.z42:6`

最大有限值。

### static float MinValue

`src/libraries/z42.core/src/Primitives/Single.z42:8`

最小有限值。

### static float Epsilon

`src/libraries/z42.core/src/Primitives/Single.z42:10`

最小正数。

### static float NaN

`src/libraries/z42.core/src/Primitives/Single.z42:13`

非数值（NaN）。

### static float PositiveInfinity

`src/libraries/z42.core/src/Primitives/Single.z42:15`

正无穷。

### static float NegativeInfinity

`src/libraries/z42.core/src/Primitives/Single.z42:18`

负无穷。

### static float Parse(string s)

`src/libraries/z42.core/src/Primitives/Single.z42:21`

解析浮点字符串（经 double 解析后窄化为 float）；失败抛异常，语义同 Double.Parse。

### static bool TryParse(string s, ref float v)

`src/libraries/z42.core/src/Primitives/Single.z42:24`

非抛出解析：成功把值写入 `v` 并返回 true；失败返回 false 且 `v` 置 0.0f。

### static bool IsNaN(float v)

`src/libraries/z42.core/src/Primitives/Single.z42:32`

v 是否为 NaN（NaN 是唯一不等于自身的值）。

### static bool IsInfinity(float v)

`src/libraries/z42.core/src/Primitives/Single.z42:35`

v 是否为正无穷或负无穷。

### static bool IsPositiveInfinity(float v)

`src/libraries/z42.core/src/Primitives/Single.z42:39`

v 是否为正无穷。

### static bool IsNegativeInfinity(float v)

`src/libraries/z42.core/src/Primitives/Single.z42:41`

v 是否为负无穷。

### static bool IsFinite(float v)

`src/libraries/z42.core/src/Primitives/Single.z42:43`

v 是否为有限值（非 NaN 且非无穷）。

### extern string ToString()

`src/libraries/z42.core/src/Primitives/Single.z42:46`

返回字符串表示。

### bool Equals(float other)

`src/libraries/z42.core/src/Primitives/Single.z42:49`

值相等比较（NaN != NaN）。

### int GetHashCode()

`src/libraries/z42.core/src/Primitives/Single.z42:51`

返回哈希码（IEEE-754 位模式的 32 位值）。

### int CompareTo(float other)

`src/libraries/z42.core/src/Primitives/Single.z42:54`

比较大小，返回负 / 0 / 正。

### static override float op_Add(float a, float b)

`src/libraries/z42.core/src/Primitives/Single.z42:69`

加法。

### static override float op_Subtract(float a, float b)

`src/libraries/z42.core/src/Primitives/Single.z42:71`

减法。

### static override float op_Multiply(float a, float b)

`src/libraries/z42.core/src/Primitives/Single.z42:73`

乘法。

### static override float op_Divide(float a, float b)

`src/libraries/z42.core/src/Primitives/Single.z42:75`

除法。

### static override float op_Modulo(float a, float b)

`src/libraries/z42.core/src/Primitives/Single.z42:77`

取模。

