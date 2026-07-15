# z42.uri

## 职责
URI / URL 解析、构造、percent-encoding。RFC 3986 子集，覆盖最常用的 HTTP / file /
opaque URI。

不是 WHATWG URL Standard（更宽松、状态机更大）；安全 / 浏览器场景请等 `z42.net`。

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/Uri.z42`          | `Std.Uri.Uri` 不可变值对象 + 静态 `Uri.Resolve(base, ref)` RFC 3986 §5.3 |
| `src/UriParser.z42`    | `Std.Uri.UriParser` RFC 3986 子集解析器（从 Uri.z42 拆出） |
| `src/UriCodec.z42`     | `Std.Uri.UriCodec` percent-encode/decode 辅助（从 Uri.z42 拆出） |
| `src/UriException.z42` | `Std.UriException`（malformed input / bad percent escape） |

## 入口点
- `Uri.Parse(string text)` — 字符串 → `Uri` 实例；失败抛 `UriException`
- `Uri.EncodeComponent(string s)` — `A-Za-z0-9_.~-` 保留，其它 → `%XX`（UTF-8 多字节）
- `Uri.DecodeComponent(string s)` — `%XX` → bytes → UTF-8 string
- Accessors: `GetScheme() / GetUserInfo() / GetHost() / GetPort() / GetPath() / GetQuery() / GetFragment()`
- Has-checks: `HasAuthority() / HasUserInfo() / HasHost() / HasPort() / HasQuery() / HasFragment()`
- `override string ToString()` — canonical 重组（round-trip 保证）

## 用法

```z42
using Std.Uri;
using Std.IO;

var u = Uri.Parse("https://user:pass@api.example.com:8443/v1/users?id=42&active=true#sec");
Console.WriteLine(u.GetScheme());    // "https"
Console.WriteLine(u.GetUserInfo());  // "user:pass"
Console.WriteLine(u.GetHost());      // "api.example.com"
Console.WriteLine(u.GetPort());      // 8443
Console.WriteLine(u.GetPath());      // "/v1/users"
Console.WriteLine(u.GetQuery());     // "id=42&active=true"
Console.WriteLine(u.GetFragment());  // "sec"

// file:/// 允许空 host（RFC 3986 §3.2.2）
var f = Uri.Parse("file:///tmp/data.bin");
Console.WriteLine(f.HasAuthority()); // true
Console.WriteLine(f.GetHost());      // ""

// Percent encoding
Console.WriteLine(Uri.EncodeComponent("hello world / 中文"));
// "hello%20world%20%2F%20%E4%B8%AD%E6%96%87"
Console.WriteLine(Uri.DecodeComponent("hello%20world"));  // "hello world"
```

## 依赖关系
依赖 `z42.core`（基础类型 + Exception）+ `z42.text`（StringBuilder 用于 ToString 重组）。

## 设计要点

- **不可变**：所有字段在构造时设定，无 setter；状态变更走 "Parse → 新实例" 流程
- **`_hasAuthority` 显式存**：`file:///` 和 `mailto:foo@bar` 都有空 host 但语义不同；
  `//` 是否存在不能仅由 host 是否空推断，必须独立记录以保证 round-trip
- **`_port = -1` 表示未指定**：同 C# `System.Uri.Port == -1`，不做 default-port 推断
- **UTF-8 percent encoding**：codepoint 逐个 encode；surrogate pair 合并成单个 codepoint 再 UTF-8 编码
- **opaque URI（无 `//`）**：scheme 之后直接是 path，如 `mailto:user@host`、`urn:isbn:...`

## 不在本期 Scope（见 `docs/design/stdlib/uri.md` Deferred）

- 相对 URI 解析（`Uri.Resolve(base, ref)`）
- IPv6 字面量解构（`[::1]` 当字符串原样保留）
- IRI（RFC 3987）/ punycode IDN 转换
- Builder / mutator 风格 API
- Default port 推断（`https → 443`）
