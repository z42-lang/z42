# z42.uri —— URI 解析与 percent 编解码

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.uri/`；命名空间 `Std.Uri`（`UriException` 在 `Std`）

把 URI / URL 字符串拆成结构化组件（scheme / userInfo / host / port / path / query /
fragment），并提供 percent-encoding 编解码与 RFC 3986 §5 相对引用解析。解析遵循
RFC 3986 的一个子集：**必须有 scheme**，authority 内的 IPv6 字面量按 `[::1]` 括号形式
识别，其余按分隔符切分，不做 WHATWG URL 的宽松归一化。

## Uri

不可变值对象：`Parse` 之后没有任何 mutator，要改只能重新拼串再 `Parse`。

```z42
namespace Std.Uri;

public class Uri {
    // 八参数构造器：逐字段直接构造（不做校验）
    public Uri(string scheme, bool hasAuthority, string userInfo, string host, int port,
               string path, string query, string fragment)

    // 解析
    public static Uri Parse(string text)
    public static Uri Resolve(Uri base_, string reference)

    // 组件读取
    public string GetScheme()
    public string GetUserInfo()
    public string GetHost()
    public int    GetPort()
    public string GetPath()
    public string GetQuery()
    public string GetFragment()

    // 存在性
    public bool HasAuthority()
    public bool HasUserInfo()
    public bool HasHost()
    public bool HasPort()
    public bool HasQuery()
    public bool HasFragment()

    // host / port 辅助
    public bool   IsIPv6Literal()
    public string GetHostName()
    public int    EffectivePort()
    public static int DefaultPortFor(string scheme)

    // percent 编解码（转发到 UriCodec）
    public static string EncodeComponent(string s)
    public static string DecodeComponent(string s)

    // 重组
    override string ToString()
}
```

| 成员 | 说明 |
|---|---|
| `Parse` | 解析绝对 URI。语法不合法抛 `UriException` |
| `Resolve` | RFC 3986 §5.3 相对引用解析。`reference` 自带 scheme 时直接当绝对 URI（仍做 dot-segment 归约）；`""` 返回 base 并清空 fragment |
| `GetUserInfo` / `GetHost` / `GetQuery` / `GetFragment` | 缺省时返回 `""`，不是 `null`。`query` 不含前导 `?`，`fragment` 不含前导 `#` |
| `GetPort` | 未显式写端口时返回 `-1` |
| `GetHost` | IPv6 字面量**保留方括号**（`[::1]`），以保证 `ToString` 能原样重组 |
| `HasAuthority` | URI 在 scheme 后带 `//` 时为 `true`——与 host 是否为空无关（`file:///tmp/a` 是 `true` 且 host 为 `""`） |
| `HasHost` / `HasUserInfo` / `HasQuery` / `HasFragment` | 即「对应字段非空串」 |
| `HasPort` | 即 `GetPort() >= 0` |
| `IsIPv6Literal` | host 以 `[` 开头、`]` 结尾且长度 ≥ 2 |
| `GetHostName` | 剥掉 IPv6 方括号后的 host（`[::1]` → `::1`）；普通域名 / IPv4 与 `GetHost()` 相同。这是可以交给 `Std.Net.IPAddress.Parse` 或 DNS 的形态 |
| `EffectivePort` | 有显式端口就返回它，否则返回 scheme 的 IANA 默认端口；两者都没有返回 `-1` |
| `DefaultPortFor` | scheme → 默认端口，scheme 大小写不敏感；未收录的 scheme 返回 `-1`（空串同样 `-1`）。静态方法，不需要先构造 `Uri` |
| `ToString` | 按 `scheme:[//[userInfo@]host[:port]]path[?query][#fragment]` 重组，round-trip 安全 |

`DefaultPortFor` 收录 26 个 scheme：`http` 80、`https` 443、`ftp` 21、`ftps` 990、
`ssh` 22、`sftp` 22、`telnet` 23、`smtp` 25、`smtps` 465、`dns` 53、`tftp` 69、
`gopher` 70、`pop3` 110、`pop3s` 995、`ntp` 123、`imap` 143、`imaps` 993、`snmp` 161、
`ldap` 389、`ldaps` 636、`ws` 80、`wss` 443、`redis` 6379、`mongodb` 27017、
`postgresql` 5432、`mysql` 3306。

## UriCodec

percent 编解码。`Uri.EncodeComponent` / `Uri.DecodeComponent` 就是它的转发入口，
两套名字行为完全一致。

```z42
namespace Std.Uri;

public static class UriCodec {
    public static string Encode(string s)
    public static string Decode(string s)
}
```

| 成员 | 说明 |
|---|---|
| `Encode` | 只保留 RFC 3986 §2.3 的 unreserved 集合 `A-Za-z0-9 - _ . ~`，其余一律 `%XX`（大写十六进制）。非 ASCII 码点先 UTF-8 编码再逐字节 percent 化。**`/` 也会被编码**——路径分隔符要自己分段编码再拼 |
| `Decode` | `%XX` → 字节 → UTF-8 字符串。额外接受 form-urlencoded 习惯：**裸 `+` 解码成空格**。要保留字面 `+`，编码侧必须写成 `%2B` |

## UriParser

```z42
namespace Std.Uri;

public sealed class UriParser {
    public UriParser(string src)
    public Uri ParseUri()
}
```

一次性解析器，`ParseUri()` 只能对一个实例调用一次（内部游标不重置）。日常直接用
`Uri.Parse` 即可，它内部就是 `new UriParser(text).ParseUri()`。

## UriException

```z42
namespace Std;   // 注意：不在 Std.Uri

public class UriException : Exception {
    public UriException(string message)
    override string ToString()
}
```

`Parse` / `Resolve` / `DecodeComponent` 的所有失败都抛这一种异常。触发条件：

| 消息 | 触发 |
|---|---|
| `empty URI` | 输入为空串 |
| `scheme must start with a letter` | 首字符不是 ASCII 字母（`//host/x`、`/abs/path` 都落这里） |
| `expected ':' after scheme but got '<c>'` | scheme 后缺 `:` |
| `unterminated IPv6 literal '[' in authority` | authority 里的 `[` 没有配对 `]` |
| `expected port number after ':'` | host 后有 `:` 但没有数字 |
| `truncated %XX in percent-encoded input` | `%` 后不足两个字符 |
| `invalid hex digit in %XX` | `%` 后两位不是十六进制数字 |
| `non-ASCII char in percent-encoded input` | 待解码串里出现码点 > 127 的裸字符 |
| `invalid UTF-8 leading byte in decoded output` | 解出的字节序列不是合法 UTF-8 起始字节 |

## 用法

```z42
using Std;
using Std.IO;
using Std.Uri;

void Main() {
    var u = Uri.Parse("https://user:pw@example.com:8443/a/b?q=1#frag");
    Console.WriteLine(u.GetHost());          // example.com
    Console.WriteLine(u.EffectivePort());    // 8443
    Console.WriteLine(u.ToString());         // 原样重组

    var h = Uri.Parse("http://example.com/x");
    Console.WriteLine(h.GetPort());          // -1（未显式写）
    Console.WriteLine(h.EffectivePort());    // 80

    // IPv6 字面量
    var v6 = Uri.Parse("http://[::1]:8080/p");
    Console.WriteLine(v6.GetHost());         // [::1]
    Console.WriteLine(v6.GetHostName());     // ::1

    // 相对引用
    var b = Uri.Parse("http://a.com/x/y/z");
    Console.WriteLine(Uri.Resolve(b, "../w").ToString());       // http://a.com/x/w
    Console.WriteLine(Uri.Resolve(b, "//b.com/y").ToString());  // http://b.com/y
    Console.WriteLine(Uri.Resolve(b, "?q=1").ToString());       // http://a.com/x/y/z?q=1

    // 查询串拼装
    string q = Uri.EncodeComponent("a b/c") + "=" + Uri.EncodeComponent("中");
    Console.WriteLine(q);                                       // a%20b%2Fc=%E4%B8%AD
    Console.WriteLine(Uri.DecodeComponent("a%20b%2Fc"));        // a b/c

    try {
        Uri.Parse("//nohost/x");
    } catch (UriException e) {
        Console.WriteLine(e.Message);        // scheme must start with a letter
    }
}
```

## 不支持

- **相对引用不能单独 `Parse`**：`Uri.Parse` 要求输入自带 scheme，`../w` / `/y` /
  `//host/x` 都会抛 `UriException`。相对形态只能作为 `Resolve` 的第二个参数。
- **无 builder / mutator**：没有 `WithHost` / `WithPort` 之类。改一个组件的办法是拼串重新
  `Parse`，或用八参数构造器逐字段重建。
- **不做规范化**：`ToString` 原样重组——不折叠 percent 编码的大小写、不小写化 scheme 与
  host、不剔除与默认端口相同的显式端口、也不在 `Parse` 阶段做 dot-segment 归约
  （只有 `Resolve` 会归约路径）。
- **IRI / IDN 不支持**：非 ASCII 域名不做 punycode（`xn--`）转换，也不做 Unicode NFC 归一化。
- **opaque URI 不细分**：`mailto:` / `urn:` 这类无 `//` 的 URI 只填 scheme + path，
  path 就是 `:` 之后的整段（`mailto:foo@bar.com` 的 path 是 `foo@bar.com`）。
- **IPv6 不拆分量**：host 原样保留括号形态，不拆成 16-bit 组件；需要数值形态请把
  `GetHostName()` 交给 `Std.Net.IPAddress.Parse`。
- **`Resolve` 的 `..` 越界被静默夹紧**：`/x/../../y` 归约成 `/y`，不报错。
- **`Decode` 不区分 query 与 path 语境**：任何位置的裸 `+` 都变成空格。
