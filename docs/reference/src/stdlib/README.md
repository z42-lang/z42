# 标准库

这里收录 z42 标准库的公开 API，包括类型、方法签名和用法。`z42.core` 按类型分页面，其余包各有一页。

包名（`z42.<domain>`）是**分发单元**，命名空间（`Std.<X>`）是**引用名**，两者不一一对应——
`z42.core` 一个包就横跨 `Std` / `Std.Collections` / `Std.IO` / `Std.Time` 等多个命名空间。
每页页头都写明了该页对应的包路径与命名空间。

`z42.core` 是**隐式 prelude**：所有程序自动加载，不需要也不能在 `z42.toml` 里声明依赖。
其余包按需在清单里声明，字段写法见[工程清单 z42.toml](../toolchain/z42-toml.md)。

## 按包索引

<!-- z42-doc:stdlib-overview-begin -->
| 包 | 主命名空间 | 概述 |
|---|---|---|
| [z42.core](core.md) | `Std` | 核心类型与预导入工具。 |
| [z42.collections](collections.md) | `Std.Collections` | 进阶集合容器。 |
| [z42.encoding](encoding.md) | `Std.Encoding` | 字节与文本的编解码。 |
| [z42.text](text.md) | `Std.Text` | 字符串构建与文本工具。 |
| [z42.io](io.md) | `Std.IO` | 流式 I/O、二进制读写与子进程。 |
| [z42.toml](toml.md) | `Std.Toml` | 把 TOML 1.0 文本解析成值树（[`TomlValue`]），或把值树写回 canonical TOML 文本。 |
| [z42.diagnostics](diagnostics.md) | `Std.Diagnostics` | 日志与运行时自省。 |
| [z42.threading](threading.md) | `Std.Threading` | OS 线程与同步原语。 |
| [z42.uri](uri.md) | `Std.Uri` | URI 解析与 percent 编解码。 |
| [z42.regex](regex.md) | `Std.Regex` | 正则表达式编译与匹配。 |
| [z42.cli](cli.md) | `Std.Cli` | 命令行参数解析。 |
| [z42.json](json.md) | `Std.Json` | JSON 读写与对象序列化。 |
| [z42.random](random.md) | `Std.Random` | 确定性伪随机数：给同一个 seed 得到同一串数。 |
| [z42.numerics](numerics.md) | `Std.Numerics` | 任意精度整数 / 十进制定点 / 复数。 |
| [z42.crypto](crypto.md) | `Std.Crypto` | 加密原语。 |
| [z42.compression](compression.md) | `Std.Compression` | 压缩算法与归档格式。 |
| [z42.yaml](yaml.md) | `Std.Yaml` | YAML 读写。 |
| [z42.net](net.md) | `Std.Net.Sockets` | 同步阻塞的网络类型。 |
| [z42.harness](harness.md) | `Std.Harness` | 测试框架：声明（attribute）、发现（反射）、运行与报告。 |
<!-- z42-doc:stdlib-overview-end -->

## 不在本部分

- **测试框架**（`z42.harness` 的 `[Test]` 写法、`z42 test` 用法）——归工具链部分
- **语法面**：集合字面量 `[1,2,3]` / `{}`、`foreach`、字符串字面量与插值等属语言规则，
  见[语言部分](../language/index.md)；本部分只写**方法面**
- **编译器与工具自身的库**（`Z42.*` 命名空间的 `z42.package` / `z42.project` / `z42.build` /
  `z42c.core` / `z42c.syntax`）——它们不是用户 API
