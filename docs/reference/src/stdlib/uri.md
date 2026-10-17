<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.uri/ -->

# z42.uri —— URI 解析与 percent 编解码。

> 包路径 `src/libraries/z42.uri/`；命名空间 `Std.Uri`

把 URI / URL 字符串拆成结构化组件（scheme / userInfo / host / port / path / query /
fragment），并提供 percent-encoding 编解码与 RFC 3986 §5 相对引用解析。解析遵循
RFC 3986 的一个子集：必须有 scheme，authority 内的 IPv6 字面量按 `[::1]` 括号形式
识别，其余按分隔符切分，不做 WHATWG URL 的宽松归一化。

## 不支持

- 相对引用不能单独 `Parse`：`Uri.Parse` 要求输入自带 scheme，`../w` / `/y` / `//host/x`
  都会抛 `UriException`。相对形态只能作为 `Resolve` 的第二个参数。
- 无 builder / mutator：没有 `WithHost` / `WithPort` 之类。改一个组件的办法是拼串重新
  `Parse`，或用八参数构造器逐字段重建。
- 不做规范化：`ToString` 原样重组——不折叠 percent 编码的大小写、不小写化 scheme 与
  host、不剔除与默认端口相同的显式端口、也不在 `Parse` 阶段做 dot-segment 归约
  （只有 `Resolve` 会归约路径）。
- IRI / IDN 不支持：非 ASCII 域名不做 punycode（`xn--`）转换，也不做 Unicode NFC 归一化。
- opaque URI 不细分：`mailto:` / `urn:` 这类无 `//` 的 URI 只填 scheme + path，
  path 就是 `:` 之后的整段（`mailto:foo@bar.com` 的 path 是 `foo@bar.com`）。
- IPv6 不拆分量：host 原样保留括号形态，不拆成 16-bit 组件；需要数值形态请把
  `GetHostName()` 交给 `Std.Net.IPAddress.Parse`。
- `Resolve` 的 `..` 越界被静默夹紧：`/x/../../y` 归约成 `/y`，不报错。
- `Decode` 不区分 query 与 path 语境：任何位置的裸 `+` 都变成空格。

## Uri

`src/libraries/z42.uri/src/Uri.z42:27`


```z42
public class Uri {
    Uri(string scheme, bool hasAuthority, string userInfo, string host, int port, string path, string query, string fragment)
    string GetScheme()
    string GetUserInfo()
    string GetHost()
    int GetPort()
    string GetPath()
    string GetQuery()
    string GetFragment()
    bool HasUserInfo()
    bool HasHost()
    bool HasAuthority()
    bool HasPort()
    bool HasQuery()
    bool HasFragment()
    bool IsIPv6Literal()
    string GetHostName()
    int EffectivePort()
    static int DefaultPortFor(string scheme)
    static Uri Parse(string text)
    static string EncodeComponent(string s)
    static string DecodeComponent(string s)
    static Uri Resolve(Uri base_, string reference)
}
```

### Uri(string scheme, bool hasAuthority, string userInfo, string host, int port, string path, string query, string fragment)

`src/libraries/z42.uri/src/Uri.z42:38`

八参数构造器：逐字段直接构造（不做校验）。

### string GetScheme()

`src/libraries/z42.uri/src/Uri.z42:53`

返回 scheme 组件。

### string GetUserInfo()

`src/libraries/z42.uri/src/Uri.z42:55`

返回 userInfo 组件；缺省时返回空串。

### string GetHost()

`src/libraries/z42.uri/src/Uri.z42:57`

返回 host 组件；IPv6 字面量保留方括号（`[::1]`）；缺省返回空串。

### int GetPort()

`src/libraries/z42.uri/src/Uri.z42:59`

返回显式端口；未显式写端口时返回 -1。

### string GetPath()

`src/libraries/z42.uri/src/Uri.z42:61`

返回 path 组件。

### string GetQuery()

`src/libraries/z42.uri/src/Uri.z42:63`

返回 query 组件，不含前导 `?`；缺省返回空串。

### string GetFragment()

`src/libraries/z42.uri/src/Uri.z42:65`

返回 fragment 组件，不含前导 `#`；缺省返回空串。

### bool HasUserInfo()

`src/libraries/z42.uri/src/Uri.z42:68`

userInfo 非空串时为 true。

### bool HasHost()

`src/libraries/z42.uri/src/Uri.z42:70`

host 非空串时为 true。

### bool HasAuthority()

`src/libraries/z42.uri/src/Uri.z42:72`

URI 在 scheme 后带 `//` 时为 true（与 host 是否为空无关）。

### bool HasPort()

`src/libraries/z42.uri/src/Uri.z42:74`

即 `GetPort() >= 0`。

### bool HasQuery()

`src/libraries/z42.uri/src/Uri.z42:76`

query 非空串时为 true。

### bool HasFragment()

`src/libraries/z42.uri/src/Uri.z42:78`

fragment 非空串时为 true。

### bool IsIPv6Literal()

`src/libraries/z42.uri/src/Uri.z42:85`

host 是否以 `[` 开头、`]` 结尾且长度 ≥ 2（即括号形式的 IPv6 字面量）。

### string GetHostName()

`src/libraries/z42.uri/src/Uri.z42:92`

剥掉 IPv6 方括号后的 host（`[::1]` → `::1`）；普通域名 / IPv4 与 `GetHost()` 相同。

### int EffectivePort()

`src/libraries/z42.uri/src/Uri.z42:100`

返回生效端口：有显式端口就返回它，否则返回 scheme 的 IANA 默认端口；两者都没有返回 -1。

### static int DefaultPortFor(string scheme)

`src/libraries/z42.uri/src/Uri.z42:106`

返回 scheme 对应的 IANA 默认端口（scheme 大小写不敏感）；未收录的 scheme 返回 -1。

### static Uri Parse(string text)

`src/libraries/z42.uri/src/Uri.z42:161`

解析绝对 URI。

| 参数 | 说明 |
|---|---|
| `text` | 待解析的 URI 字符串，必须自带 scheme |

抛 `UriException`：语法不合法时

### static string EncodeComponent(string s)

`src/libraries/z42.uri/src/Uri.z42:167`

percent 编码组件：只保留 RFC 3986 §2.3 的 unreserved 集合，其余一律 `%XX`（大写十六进制）。

### static string DecodeComponent(string s)

`src/libraries/z42.uri/src/Uri.z42:174`

percent 解码组件：`%XX` → 字节 → UTF-8 字符串；裸 `+` 解码成空格。

抛 `UriException`：`%` 后不足两位或非十六进制数字等非法输入时

### static Uri Resolve(Uri base_, string reference)

`src/libraries/z42.uri/src/Uri.z42:183`

按 RFC 3986 §5.3 把相对引用 reference 相对 base_ 解析成绝对 URI。

| 参数 | 说明 |
|---|---|
| `base_` | 基 URI |
| `reference` | 相对或绝对引用；自带 scheme 时直接当绝对 URI（仍做 dot-segment 归约）；空串返回 base 并清空 fragment |

抛 `UriException`：当 reference 解析为绝对 URI 且语法不合法时

## UriCodec

`src/libraries/z42.uri/src/UriCodec.z42:5`

percent 编解码辅助：`Uri.EncodeComponent` / `Uri.DecodeComponent` 就是它的转发入口，行为完全一致。

```z42
public static class UriCodec {
    static string Encode(string s)
    static string Decode(string s)
}
```

### static string Encode(string s)

`src/libraries/z42.uri/src/UriCodec.z42:9`

只保留 RFC 3986 §2.3 的 unreserved 集合，其余一律 `%XX`（大写十六进制）；非 ASCII 先 UTF-8 再逐字节编码。

### static string Decode(string s)

`src/libraries/z42.uri/src/UriCodec.z42:52`

`%XX` → 字节 → UTF-8 字符串；裸 `+` 解码成空格。

抛 `UriException`：`%` 后不足两位、非十六进制数字、裸非 ASCII 字符或非法 UTF-8 时

## UriException

`src/libraries/z42.uri/src/UriException.z42:4`

URI 解析 / 编解码错误；`Parse` / `Resolve` / `DecodeComponent` 的所有失败都抛这一种异常。

```z42
public class UriException : Exception {
    UriException(string message)
}
```

### UriException(string message)

`src/libraries/z42.uri/src/UriException.z42:6`

用给定消息构造异常。

## UriParser

`src/libraries/z42.uri/src/UriParser.z42:4`

一次性 URI 解析器：`ParseUri()` 只能对一个实例调用一次（内部游标不重置）。

```z42
public sealed class UriParser {
    UriParser(string src)
    Uri ParseUri()
}
```

### UriParser(string src)

`src/libraries/z42.uri/src/UriParser.z42:9`

构造解析器，绑定待解析的源字符串。

### Uri ParseUri()

`src/libraries/z42.uri/src/UriParser.z42:15`

解析为 Uri（只能对一个实例调用一次）。

