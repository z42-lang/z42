# z42.uri

## 职责
URI / URL 解析、构造、percent-encoding。RFC 3986 子集，覆盖最常用的 HTTP / file /
opaque URI。

不是 WHATWG URL Standard（更宽松、状态机更大）；网络访问见 `z42.net`。

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/Uri.z42`          | `Std.Uri.Uri` 不可变值对象 + 静态 `Uri.Resolve(base, ref)` RFC 3986 §5.3 |
| `src/UriParser.z42`    | `Std.Uri.UriParser` RFC 3986 子集解析器 |
| `src/UriCodec.z42`     | `Std.Uri.UriCodec` percent-encode/decode 辅助 |
| `src/UriException.z42` | `Std.UriException`（malformed input / bad percent escape） |

## 入口点
公开 API 的完整签名见 [reference 包页](../../../docs/reference/src/stdlib/uri.md)（由源码 doc 注释生成）。

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
- **`_port = -1` 表示未指定**：同 C# `System.Uri.Port == -1`，`GetPort()` 不做 default-port 推断（需要时用 `EffectivePort()`）
- **UTF-8 percent encoding**：codepoint 逐个 encode；surrogate pair 合并成单个 codepoint 再 UTF-8 编码
- **opaque URI（无 `//`）**：scheme 之后直接是 path，如 `mailto:user@host`、`urn:isbn:...`

## 不支持（见 `docs/reference/src/stdlib/uri.md`「不支持」节）

- 相对引用单独 `Parse`（只能作为 `Resolve` 的第二个参数）
- IPv6 字面量拆分量（`[::1]` 原样保留）
- IRI（RFC 3987）/ punycode IDN 转换
- Builder / mutator 风格 API；规范化（大小写 / 默认端口剔除 / dot-segment 仅 `Resolve` 做）

## 如何测试验证

```bash
xtask test stdlib z42.uri    # 本库全部 [Test]
```

## 关联文档

- 用法与规则（参考手册）：[z42.uri](../../../docs/reference/src/stdlib/uri.md)
- 设计 / 机制（实现内幕）：[标准库三层架构](../../../docs/internals/src/stdlib/architecture.md) · [包划分与 interop 归属](../../../docs/internals/src/stdlib/organization.md)
