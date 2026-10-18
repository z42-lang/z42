<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.encoding/ -->

# z42.encoding —— 字节与文本的编解码。

> 包路径 `src/libraries/z42.encoding/`；命名空间 `Std.Encoding`

`byte[]` ↔ `string` 的两类转换：**文本编码**（UTF-8 / UTF-16 / UTF-32，把字符串变成字节）
与 **二进制转文本编码**（Hex / Base64 / Base32 的四种变体，把任意字节变成可打印字符）。
全部是静态类，不需要实例化。

所有格式错误统一抛 `Std.FormatException`；只有 `Encoding.GetString` 的越界切片抛
`Std.ArgumentException`。解码一律**严格 fail-fast**，不会静默替换成 U+FFFD。

## 不支持

- **只有一次性全量转换**：没有 streaming 的 `Encoder` / `Decoder` 状态机，输入必须整个 `byte[]` / `string` 在手。
- **没有宽松解码**：所有解码器遇到非法输入都抛异常，没有「替换成 U+FFFD 继续」的 overload。
- **UTF-8 之外的单字节编码没有**：Latin-1 / ASCII / GBK 等都不提供；`Encoding` 类目前只包 UTF-8 一种实现。
- **不处理 BOM**：`Utf16` / `Utf32` 既不写入也不识别 BOM，字节序只由方法名决定。
- **Base32 / Base32Hex 只接受大写**：小写输入报错，需要容错请先自行 `ToUpper`。
- **没有 Base85 / ASCII85**。
- **Crockford 的 check-digit 扩展（`*~$=U`）不支持**。
- **Base64 不支持换行折行**：既不输出 MIME 76 列折行，解码也不跳过 `\r` / `\n`。
- **Hex 无分隔符形态**：不产生也不接受 `de:ad:be:ef` 这类带分隔符的 hex。

## Base32

[`src/libraries/z42.encoding/src/Base32.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Base32.z42#L9)

Base32 编解码（RFC 4648 §6 标准变体）。

字母表 `A-Z2-7`（严格大写），尾部用 `=` 补齐到 8 的倍数；`Decode` 只接受大写。
长度必须是 8 的倍数，padding 个数必须属于 `{0, 1, 3, 4, 6}`。典型用途：TOTP / HOTP 密钥。

```z42
public static class Base32 {
    static string Encode(byte[] bytes)
    static byte[] Decode(string s)
}
```

成员：[`Encode`](#Std.Encoding.Base32.Encode(byte[])) · [`Decode`](#Std.Encoding.Base32.Decode(string))

<a id="Std.Encoding.Base32.Encode(byte[])"></a>
### static string Encode(byte[] bytes)

[`src/libraries/z42.encoding/src/Base32.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Base32.z42#L13)

把字节编码为 Base32 字符串（字母表 `A-Z2-7`，带 `=` padding）。

<a id="Std.Encoding.Base32.Decode(string)"></a>
### static byte[] Decode(string s)

[`src/libraries/z42.encoding/src/Base32.z42:78`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Base32.z42#L78)

把 Base32 字符串解码为字节；只接受大写，严格校验。

## Base32Crockford

[`src/libraries/z42.encoding/src/Base32Crockford.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Base32Crockford.z42#L9)

Crockford Base32 编解码。

字母表 `0123456789ABCDEFGHJKMNPQRSTVWXYZ`（剔除 `I` `L` `O` `U`），与 RFC 4648 §6 非同一张表。
`Encode` 无 padding；`Decode` 大小写不敏感，`I`/`L`→`1`、`O`→`0`，`-` 分隔符丢弃。典型用途：ULID。

```z42
public static class Base32Crockford {
    static string Encode(byte[] bytes)
    static byte[] Decode(string s)
}
```

成员：[`Encode`](#Std.Encoding.Base32Crockford.Encode(byte[])) · [`Decode`](#Std.Encoding.Base32Crockford.Decode(string))

<a id="Std.Encoding.Base32Crockford.Encode(byte[])"></a>
### static string Encode(byte[] bytes)

[`src/libraries/z42.encoding/src/Base32Crockford.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Base32Crockford.z42#L13)

把字节编码为 Crockford Base32 字符串（无 padding）。

<a id="Std.Encoding.Base32Crockford.Decode(string)"></a>
### static byte[] Decode(string s)

[`src/libraries/z42.encoding/src/Base32Crockford.z42:72`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Base32Crockford.z42#L72)

解码 Crockford Base32 字符串；大小写不敏感，`-` 丢弃，`I`/`L`→`1`、`O`→`0`。

## Base32Hex

[`src/libraries/z42.encoding/src/Base32Hex.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Base32Hex.z42#L9)

Base32Hex 编解码（RFC 4648 §7）。

字母表 `0-9A-V`（A=10 … V=31），打包与 padding 规则同 `Base32`。字母表按序，
因此编码串字典序与原字节自然序一致——如 DNS NSEC3 场景适用。`Decode` 只接受大写。

```z42
public static class Base32Hex {
    static string Encode(byte[] bytes)
    static byte[] Decode(string s)
}
```

成员：[`Encode`](#Std.Encoding.Base32Hex.Encode(byte[])) · [`Decode`](#Std.Encoding.Base32Hex.Decode(string))

<a id="Std.Encoding.Base32Hex.Encode(byte[])"></a>
### static string Encode(byte[] bytes)

[`src/libraries/z42.encoding/src/Base32Hex.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Base32Hex.z42#L13)

把字节编码为 Base32Hex 字符串（字母表 `0-9A-V`，带 `=` padding）。

<a id="Std.Encoding.Base32Hex.Decode(string)"></a>
### static byte[] Decode(string s)

[`src/libraries/z42.encoding/src/Base32Hex.z42:70`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Base32Hex.z42#L70)

把 Base32Hex 字符串解码为字节；只接受大写，严格校验。

## Base64

[`src/libraries/z42.encoding/src/Base64.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Base64.z42#L8)

Base64 编解码（RFC 4648 §4 标准变体）。

字母表 `A-Z a-z 0-9 + /`，输出带 `=` padding（长度始终是 4 的倍数）。
严格校验：非法字符 / 长度非 4 倍数 / padding 超过 2 个抛 `Std.FormatException`。

```z42
public static class Base64 {
    static string Encode(byte[] bytes)
    static byte[] Decode(string s)
}
```

成员：[`Encode`](#Std.Encoding.Base64.Encode(byte[])) · [`Decode`](#Std.Encoding.Base64.Decode(string))

<a id="Std.Encoding.Base64.Encode(byte[])"></a>
### static string Encode(byte[] bytes)

[`src/libraries/z42.encoding/src/Base64.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Base64.z42#L13)

把字节编码为标准 Base64 字符串（带 `=` padding）。

<a id="Std.Encoding.Base64.Decode(string)"></a>
### static byte[] Decode(string s)

[`src/libraries/z42.encoding/src/Base64.z42:53`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Base64.z42#L53)

把标准 Base64 字符串解码为字节，严格校验。

## Base64Url

[`src/libraries/z42.encoding/src/Base64Url.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Base64Url.z42#L9)

URL 与文件名安全 Base64（RFC 4648 §5）。

字母表 `A-Z a-z 0-9 - _`（`+/` → `-_`），编码输出不带 `=` padding（JWT / RFC 7515 Appendix C）。
`Decode` 宽松接受带或不带 padding，但遇到标准变体的 `+` / `/` 立即报错并提示改用 `Base64`。

```z42
public static class Base64Url {
    static string Encode(byte[] bytes)
    static byte[] Decode(string s)
}
```

成员：[`Encode`](#Std.Encoding.Base64Url.Encode(byte[])) · [`Decode`](#Std.Encoding.Base64Url.Decode(string))

<a id="Std.Encoding.Base64Url.Encode(byte[])"></a>
### static string Encode(byte[] bytes)

[`src/libraries/z42.encoding/src/Base64Url.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Base64Url.z42#L12)

用 RFC 4648 §5 字母表（`+/` → `-_`）编码字节，输出不带 `=` padding。

<a id="Std.Encoding.Base64Url.Decode(string)"></a>
### static byte[] Decode(string s)

[`src/libraries/z42.encoding/src/Base64Url.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Base64Url.z42#L28)

解码 Base64URL 字符串；接受带或不带 padding，遇到 `+` / `/` 报错并提示改用 `Base64`。

## Encoding

[`src/libraries/z42.encoding/src/Encoding.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Encoding.z42#L24)

实例编码门面；当前只有 UTF-8 一种实现。

```z42
public class Encoding {
    Encoding(int kind)
    static Encoding GetUtf8()
    byte[] GetBytes(string s)
    string GetString(byte[] bytes)
    string GetString(byte[] bytes, int offset, int count)
}
```

成员：[`Encoding`](#Std.Encoding.Encoding.ctor(int)) · [`GetUtf8`](#Std.Encoding.Encoding.GetUtf8()) · [`GetBytes`](#Std.Encoding.Encoding.GetBytes(string)) · [`GetString`](#Std.Encoding.Encoding.GetString(byte[])) · [`GetString`](#Std.Encoding.Encoding.GetString(byte[],int,int))

<a id="Std.Encoding.Encoding.ctor(int)"></a>
### Encoding(int kind)

[`src/libraries/z42.encoding/src/Encoding.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Encoding.z42#L30)

构造编码实例；`kind` 参数当前不影响行为（始终 UTF-8），请用 `Encoding.GetUtf8()` 而非手动 new。

<a id="Std.Encoding.Encoding.GetUtf8()"></a>
### static Encoding GetUtf8()

[`src/libraries/z42.encoding/src/Encoding.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Encoding.z42#L37)

返回规范的 UTF-8 编码实例；同一 VM 生命周期内多次调用返回同一实例。

<a id="Std.Encoding.Encoding.GetBytes(string)"></a>
### byte[] GetBytes(string s)

[`src/libraries/z42.encoding/src/Encoding.z42:44`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Encoding.z42#L44)

按当前编码把 `s` 编码为字节；等价于 `Utf8.GetBytes(s)`。

<a id="Std.Encoding.Encoding.GetString(byte[])"></a>
### string GetString(byte[] bytes)

[`src/libraries/z42.encoding/src/Encoding.z42:49`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Encoding.z42#L49)

按当前编码把 `bytes` 解码为字符串，严格校验；等价于 `Utf8.GetString(bytes)`。

<a id="Std.Encoding.Encoding.GetString(byte[],int,int)"></a>
### string GetString(byte[] bytes, int offset, int count)

[`src/libraries/z42.encoding/src/Encoding.z42:54`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Encoding.z42#L54)

解码切片 `bytes[offset..offset+count]`；`count == 0` 返回 `""`，越界抛 `ArgumentException`。

## EncodingSingletons

[`src/libraries/z42.encoding/src/Encoding.z42:73`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Encoding.z42#L73)

通过静态缓存实现懒初始化的 UTF-8 单例持有类。

```z42
public static class EncodingSingletons {
    static Encoding Utf8()
}
```

成员：[`Utf8`](#Std.Encoding.EncodingSingletons.Utf8())

<a id="Std.Encoding.EncodingSingletons.Utf8()"></a>
### static Encoding Utf8()

[`src/libraries/z42.encoding/src/Encoding.z42:77`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Encoding.z42#L77)

返回缓存的 UTF-8 编码实例（懒初始化）。

## Hex

[`src/libraries/z42.encoding/src/Hex.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Hex.z42#L8)

Hex 编解码。

`Encode` 输出小写，`EncodeUpper` 输出大写；`Decode` 接受混合大小写。
奇数长度抛 `FormatException("odd-length hex string")`，非 `[0-9a-fA-F]` 字符抛 `invalid hex character '<c>'`。

```z42
public static class Hex {
    static string Encode(byte[] bytes)
    static string EncodeUpper(byte[] bytes)
    static byte[] Decode(string hex)
}
```

成员：[`Encode`](#Std.Encoding.Hex.Encode(byte[])) · [`EncodeUpper`](#Std.Encoding.Hex.EncodeUpper(byte[])) · [`Decode`](#Std.Encoding.Hex.Decode(string))

<a id="Std.Encoding.Hex.Encode(byte[])"></a>
### static string Encode(byte[] bytes)

[`src/libraries/z42.encoding/src/Hex.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Hex.z42#L13)

把字节编码为小写十六进制字符串。

<a id="Std.Encoding.Hex.EncodeUpper(byte[])"></a>
### static string EncodeUpper(byte[] bytes)

[`src/libraries/z42.encoding/src/Hex.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Hex.z42#L18)

把字节编码为大写十六进制字符串。

<a id="Std.Encoding.Hex.Decode(string)"></a>
### static byte[] Decode(string hex)

[`src/libraries/z42.encoding/src/Hex.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Hex.z42#L36)

把十六进制字符串解码为字节；接受混合大小写，严格校验。

## Utf16

[`src/libraries/z42.encoding/src/Utf16.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Utf16.z42#L10)

UTF-16 编解码（RFC 2781）。

`LE` = 小端（低字节在前），`BE` = 大端（高字节在前）；字节序由方法名选定，不读写 BOM。

BMP 码点 2 字节，增补平面走代理对 4 字节。解码严格校验代理对配对，拒绝孤立高/低代理
与截断输入；字节长度不是 2 的倍数即报错。

```z42
public static class Utf16 {
    static byte[] GetBytesLE(string s)
    static byte[] GetBytesBE(string s)
    static string GetStringLE(byte[] bytes)
    static string GetStringBE(byte[] bytes)
}
```

成员：[`GetBytesLE`](#Std.Encoding.Utf16.GetBytesLE(string)) · [`GetBytesBE`](#Std.Encoding.Utf16.GetBytesBE(string)) · [`GetStringLE`](#Std.Encoding.Utf16.GetStringLE(byte[])) · [`GetStringBE`](#Std.Encoding.Utf16.GetStringBE(byte[]))

<a id="Std.Encoding.Utf16.GetBytesLE(string)"></a>
### static byte[] GetBytesLE(string s)

[`src/libraries/z42.encoding/src/Utf16.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Utf16.z42#L14)

以小端字节序把 `s` 编码为 UTF-16 字节。

<a id="Std.Encoding.Utf16.GetBytesBE(string)"></a>
### static byte[] GetBytesBE(string s)

[`src/libraries/z42.encoding/src/Utf16.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Utf16.z42#L16)

以大端字节序把 `s` 编码为 UTF-16 字节。

<a id="Std.Encoding.Utf16.GetStringLE(byte[])"></a>
### static string GetStringLE(byte[] bytes)

[`src/libraries/z42.encoding/src/Utf16.z42:71`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Utf16.z42#L71)

以小端字节序把 UTF-16 字节解码为字符串。

<a id="Std.Encoding.Utf16.GetStringBE(byte[])"></a>
### static string GetStringBE(byte[] bytes)

[`src/libraries/z42.encoding/src/Utf16.z42:73`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Utf16.z42#L73)

以大端字节序把 UTF-16 字节解码为字符串。

## Utf32

[`src/libraries/z42.encoding/src/Utf32.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Utf32.z42#L9)

UTF-32 编解码（ISO/IEC 10646）。

定长 4 字节一个码点；`LE` / `BE` 选择字节序，不读写 BOM。

编解码两侧都拒绝孤立 surrogate (U+D800–U+DFFF) 与 >U+10FFFF；字节长度不是 4 的倍数即报错。

```z42
public static class Utf32 {
    static byte[] GetBytesLE(string s)
    static byte[] GetBytesBE(string s)
    static string GetStringLE(byte[] bytes)
    static string GetStringBE(byte[] bytes)
}
```

成员：[`GetBytesLE`](#Std.Encoding.Utf32.GetBytesLE(string)) · [`GetBytesBE`](#Std.Encoding.Utf32.GetBytesBE(string)) · [`GetStringLE`](#Std.Encoding.Utf32.GetStringLE(byte[])) · [`GetStringBE`](#Std.Encoding.Utf32.GetStringBE(byte[]))

<a id="Std.Encoding.Utf32.GetBytesLE(string)"></a>
### static byte[] GetBytesLE(string s)

[`src/libraries/z42.encoding/src/Utf32.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Utf32.z42#L13)

以小端字节序把 `s` 编码为 UTF-32 字节。

<a id="Std.Encoding.Utf32.GetBytesBE(string)"></a>
### static byte[] GetBytesBE(string s)

[`src/libraries/z42.encoding/src/Utf32.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Utf32.z42#L15)

以大端字节序把 `s` 编码为 UTF-32 字节。

<a id="Std.Encoding.Utf32.GetStringLE(byte[])"></a>
### static string GetStringLE(byte[] bytes)

[`src/libraries/z42.encoding/src/Utf32.z42:55`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Utf32.z42#L55)

以小端字节序把 UTF-32 字节解码为字符串。

<a id="Std.Encoding.Utf32.GetStringBE(byte[])"></a>
### static string GetStringBE(byte[] bytes)

[`src/libraries/z42.encoding/src/Utf32.z42:57`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Utf32.z42#L57)

以大端字节序把 UTF-32 字节解码为字符串。

## Utf8

[`src/libraries/z42.encoding/src/Utf8.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Utf8.z42#L8)

UTF-8 编解码：`string` ↔ `byte[]` 双向显式转换。

`GetString` 严格校验并拒绝：overlong / surrogate (U+D800–U+DFFF) /
超界 (>U+10FFFF) / 截断 / 非法首字节 → `Std.FormatException`。

```z42
public static class Utf8 {
    static byte[] GetBytes(string s)
    static string GetString(byte[] bytes)
}
```

成员：[`GetBytes`](#Std.Encoding.Utf8.GetBytes(string)) · [`GetString`](#Std.Encoding.Utf8.GetString(byte[]))

<a id="Std.Encoding.Utf8.GetBytes(string)"></a>
### static byte[] GetBytes(string s)

[`src/libraries/z42.encoding/src/Utf8.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Utf8.z42#L10)

把字符串编码为 UTF-8 字节。

<a id="Std.Encoding.Utf8.GetString(byte[])"></a>
### static string GetString(byte[] bytes)

[`src/libraries/z42.encoding/src/Utf8.z42:52`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.encoding/src/Utf8.z42#L52)

把 UTF-8 字节解码为字符串，严格校验。

