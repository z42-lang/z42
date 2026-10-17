<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Guid

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/Guid.z42:13`

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

### Guid(byte[] b)

`src/libraries/z42.core/src/Guid.z42:17`

由恰好 16 字节构造（防御性拷贝）。

### static Guid Empty()

`src/libraries/z42.core/src/Guid.z42:31`

全零 GUID（`00000000-0000-0000-0000-000000000000`）。

### static Guid NewGuid()

`src/libraries/z42.core/src/Guid.z42:36`

生成一个 RFC 4122 v4（随机）GUID，熵来自 OS。

### static Guid Parse(string s)

`src/libraries/z42.core/src/Guid.z42:48`

解析规范 "D" 形式（36 字符，带连字符）或 "N" 形式（32 个十六进制字符）。 其它长度或非法字符抛 FormatException。

### static bool TryParse(string s, ref Guid v)

`src/libraries/z42.core/src/Guid.z42:63`

不抛异常的解析：成功时写入 `v` 并返回 true；非法输入返回 false 且 `v` 为 `Guid.Empty()`。`Guid` 是 struct ⇒ 不可空，故 out 值走 `ref`。

### bool Equals(Guid other)

`src/libraries/z42.core/src/Guid.z42:101`

按 16 字节值比较相等。

### int GetHashCode()

`src/libraries/z42.core/src/Guid.z42:117`

内容哈希码。

### override string ToString()

`src/libraries/z42.core/src/Guid.z42:130`

规范带连字符的小写形式，如 `"0f8fad5b-d9cb-469f-a165-70867728950e"`。

### string ToString(string format)

`src/libraries/z42.core/src/Guid.z42:136`

按格式串输出："D"（带连字符，默认）或 "N"（32 位无连字符）；大小写不敏感。 其它格式抛 FormatException。

