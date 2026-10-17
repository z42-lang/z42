<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Boolean

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Primitives/Boolean.z42:4`

内置 `bool` 类型的包装结构体，实现 IEquatable。

```z42
public struct Boolean : IEquatable {
    static bool Parse(string s)
    static bool TryParse(string s, ref bool v)
    bool Equals(bool other)
    int GetHashCode()
    string ToString()
}
```

### static bool Parse(string s)

`src/libraries/z42.core/src/Primitives/Boolean.z42:7`

解析布尔字符串：去首尾空白、大小写不敏感的 "true" / "false"；其余抛 FormatException，null 抛 ArgumentNullException。

### static bool TryParse(string s, ref bool v)

`src/libraries/z42.core/src/Primitives/Boolean.z42:15`

非抛出解析：null / 不认识的文本返回 false 且 `v` 置 false。

### bool Equals(bool other)

`src/libraries/z42.core/src/Primitives/Boolean.z42:24`

值相等比较。

### int GetHashCode()

`src/libraries/z42.core/src/Primitives/Boolean.z42:29`

返回哈希码（true → 1，false → 0）。

### string ToString()

`src/libraries/z42.core/src/Primitives/Boolean.z42:34`

返回小写字符串表示（"true" / "false"，与 BCL 的 "True" / "False" 不同）。

