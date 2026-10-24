<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Boolean

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Primitives/Boolean.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Boolean.z42#L4)

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

成员：[`Parse`](#Std.Boolean.Parse(string)) · [`TryParse`](#Std.Boolean.TryParse(string,bool)) · [`Equals`](#Std.Boolean.Equals(bool)) · [`GetHashCode`](#Std.Boolean.GetHashCode()) · [`ToString`](#Std.Boolean.ToString())

<a id="Std.Boolean.Parse(string)"></a>
### static bool Parse(string s)

[`src/libraries/z42.core/src/Primitives/Boolean.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Boolean.z42#L8)

解析布尔字符串：去首尾空白、大小写不敏感的 "true" / "false"。

其余抛 FormatException，null 抛 ArgumentNullException。

<a id="Std.Boolean.TryParse(string,bool)"></a>
### static bool TryParse(string s, ref bool v)

[`src/libraries/z42.core/src/Primitives/Boolean.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Boolean.z42#L16)

非抛出解析：null / 不认识的文本返回 false 且 `v` 置 false。

<a id="Std.Boolean.Equals(bool)"></a>
### bool Equals(bool other)

[`src/libraries/z42.core/src/Primitives/Boolean.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Boolean.z42#L25)

值相等比较。

<a id="Std.Boolean.GetHashCode()"></a>
### int GetHashCode()

[`src/libraries/z42.core/src/Primitives/Boolean.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Boolean.z42#L30)

返回哈希码（true → 1，false → 0）。

<a id="Std.Boolean.ToString()"></a>
### string ToString()

[`src/libraries/z42.core/src/Primitives/Boolean.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Primitives/Boolean.z42#L35)

返回小写字符串表示（"true" / "false"，与 BCL 的 "True" / "False" 不同）。

