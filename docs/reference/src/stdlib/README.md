# 标准库

> 对齐：2026-09-17（change `restructure-docs-three-books`）

z42 标准库各包的公开 API。每页覆盖一个包：类型、方法签名、用法、**不支持的用法**。

包名（`z42.<domain>`）是**分发单元**，命名空间（`Std.<X>`）是**引用名**，两者不一一对应——
`z42.core` 一个包就横跨 `Std` / `Std.Collections` / `Std.IO` / `Std.Time` 等多个命名空间。
每页页头都写明了该页对应的包路径与命名空间。

`z42.core` 是**隐式 prelude**：所有程序自动加载，不需要也不能在 `z42.toml` 里声明依赖。
其余包按需在清单里声明，字段写法见[工程清单 z42.toml](../toolchain/z42-toml.md)。

## 按包索引

### 核心（`z42.core`，隐式加载）

| 页 | 覆盖 |
|---|---|
| [集合](collections-core.md) | `List<T>` `Dictionary<TKey,TValue>` `HashSet<T>` `KeyValuePair<K,V>` `ReadOnlyCollection<T>` |
| [字符串方法](string.md) | `string` 的实例方法与静态方法全表 |
| [日期与时间](time.md) | `DateTime` `TimeSpan` `DateTimeOffset` `Stopwatch` |
| [反射](reflection.md) | `Type` `typeof` 元数据查询、特性读取 |
| [运行时设置查询](runtime-config.md) | `Std.Runtime.RuntimeConfig`——只读查询生效的运行时旋钮 |
| [应用自定义配置](app-properties.md) | `Std.Runtime.AppProperties`——清单 `[properties]` 表的运行时读取 |
| [平台与宿主信息](platform.md) | `Std.Platform`（OS / 架构 / 能力查询）`Std.OperatingSystem` `OSKind` `ArchKind` |
| [GC 与句柄](gc.md) | `Std.GC` `HeapStats` `GCHandle` / `GCHandleType` `WeakHandle` `SoftHandle` |

### 数据与文本

| 页 | 包 | 覆盖 |
|---|---|---|
| [JSON](json.md) | `z42.json` | `JsonValue` DOM + `JsonSerializer` 对象序列化 |
| [TOML](toml.md) | `z42.toml` | `TomlValue` 读写 |
| [YAML](yaml.md) | `z42.yaml` | YAML 1.2 子集读写 |
| [正则](regex.md) | `z42.regex` | `Regex` `Match` |
| [文本处理](text.md) | `z42.text` | `StringBuilder` 等 |
| [编解码](encoding.md) | `z42.encoding` | Hex / Base64 / Base32 / UTF-8/16/32 |
| [URI](uri.md) | `z42.uri` | `Uri` 解析 + percent 编解码 |
| [次级集合](collections.md) | `z42.collections` | `Stack<T>` `Queue<T>` `LinkedList<T>` `PriorityQueue<T>` `SortedSet<T>` |

### 系统与 IO

| 页 | 包 | 覆盖 |
|---|---|---|
| [控制台与文件](io-file.md) | `z42.core` | `Console` `ConsoleError` `File` `Directory` `Path` `Environment` |
| [子进程与终端](process.md) | `z42.io` | `Process` `ProcessHandle` `ProcessResult` `Stdio` `Ansi` |
| [流](io-stream.md) | `z42.io` | `Stream` 家族、文件流、文本读写器 |
| [二进制读写](io-binary.md) | `z42.io` | `BinaryReader` / `BinaryWriter` |
| [网络](net.md) | `z42.net` | TCP / UDP / HTTP / TLS / WebSocket / `IPAddress` / DNS |
| [并发](threading.md) | `z42.threading` | `Thread` `Channel<T>` `Mutex<T>` `RwLock<T>` `Timer` |
| [压缩](compression.md) | `z42.compression` | Gzip / Zlib / Deflate / Zstd / Brotli / Tar / Zip |
| [日志](diagnostics.md) | `z42.diagnostics` | `Log` `LogLevel` |

### 应用支撑

| 页 | 包 | 覆盖 |
|---|---|---|
| [命令行参数](cli.md) | `z42.cli` | `ArgParser` flag / option / positional |
| [随机数](random.md) | `z42.random` | `Random`——确定性 PRNG（**不是** CSPRNG） |
| [密码学](crypto.md) | `z42.crypto` | 摘要 / HMAC / 对称加密 / 签名 / KDF / `SecureRandom` |
| [大数与数值类型](numerics.md) | `z42.numerics` | `BigInt` / `Decimal` / `Complex` |

## 不在本部分

- **测试框架**（`z42.test` 的 `[Test]` 写法、`z42 test` 用法）——归工具链部分
- **语法面**：集合字面量 `[1,2,3]` / `{}`、`foreach`、字符串字面量与插值等属语言规则，
  见[语言部分](../language/README.md)；本部分只写**方法面**
- **编译器与工具自身的库**（`Z42.*` 命名空间的 `z42.ir` / `z42.project` / `z42.build` /
  `z42c.core` / `z42c.syntax`）——它们不是用户 API
