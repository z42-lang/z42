<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Guid

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/Guid.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Guid.z42#L13)

128 位全局唯一标识符（C# System.Guid 对标）。

值不可变；字符串形式为规范 8-4-4-4-12 小写十六进制（"D"），"N" 为 32 位无连字符形式。
`NewGuid()` 生成 RFC 4122 v4（随机）GUID，熵来自 OS。

> ⚠️ z42 struct-default 限制：`default(Guid)` 得 null 值（不是零值），所以必须用 `Guid.Empty()` 取全零 GUID；对 `default(Guid)` 调方法会 trap。

```z42
public struct Guid {
    Guid(byte[] b)
    static Guid Empty()
    static Guid NewGuid()
    static Guid Parse(string s)
    static bool TryParse(string s, ref Guid v)
    bool Equals(Guid other)
    int GetHashCode()
    override string ToString()
    string ToString(string format)
}
```

成员：[`Guid`](#Std.Guid.ctor(byte[])) · [`Empty`](#Std.Guid.Empty()) · [`NewGuid`](#Std.Guid.NewGuid()) · [`Parse`](#Std.Guid.Parse(string)) · [`TryParse`](#Std.Guid.TryParse(string,Guid)) · [`Equals`](#Std.Guid.Equals(Guid)) · [`GetHashCode`](#Std.Guid.GetHashCode()) · [`ToString`](#Std.Guid.ToString()) · [`ToString`](#Std.Guid.ToString(string))

<a id="Std.Guid.ctor(byte[])"></a>
### Guid(byte[] b)

[`src/libraries/z42.core/src/Guid.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Guid.z42#L17)

由恰好 16 字节构造（防御性拷贝）。

<a id="Std.Guid.Empty()"></a>
### static Guid Empty()

[`src/libraries/z42.core/src/Guid.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Guid.z42#L31)

全零 GUID（`00000000-0000-0000-0000-000000000000`）。

<a id="Std.Guid.NewGuid()"></a>
### static Guid NewGuid()

[`src/libraries/z42.core/src/Guid.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Guid.z42#L36)

生成一个 RFC 4122 v4（随机）GUID，熵来自 OS。

<a id="Std.Guid.Parse(string)"></a>
### static Guid Parse(string s)

[`src/libraries/z42.core/src/Guid.z42:48`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Guid.z42#L48)

解析规范 "D" 形式（36 字符，带连字符）或 "N" 形式（32 个十六进制字符）。 其它长度或非法字符抛 FormatException。

<a id="Std.Guid.TryParse(string,Guid)"></a>
### static bool TryParse(string s, ref Guid v)

[`src/libraries/z42.core/src/Guid.z42:65`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Guid.z42#L65)

不抛异常的解析：成功时写入 `v` 并返回 true。

非法输入返回 false 且 `v` 为 `Guid.Empty()`。`Guid` 是 struct ⇒ 不可空，
故 out 值走 `ref`。

<a id="Std.Guid.Equals(Guid)"></a>
### bool Equals(Guid other)

[`src/libraries/z42.core/src/Guid.z42:103`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Guid.z42#L103)

按 16 字节值比较相等。

<a id="Std.Guid.GetHashCode()"></a>
### int GetHashCode()

[`src/libraries/z42.core/src/Guid.z42:119`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Guid.z42#L119)

内容哈希码。

<a id="Std.Guid.ToString()"></a>
### override string ToString()

[`src/libraries/z42.core/src/Guid.z42:132`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Guid.z42#L132)

规范带连字符的小写形式，如 `"0f8fad5b-d9cb-469f-a165-70867728950e"`。

<a id="Std.Guid.ToString(string)"></a>
### string ToString(string format)

[`src/libraries/z42.core/src/Guid.z42:138`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Guid.z42#L138)

按格式串输出："D"（带连字符，默认）或 "N"（32 位无连字符）；大小写不敏感。 其它格式抛 FormatException。

