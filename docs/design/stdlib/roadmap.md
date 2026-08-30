# z42 标准库扩展路线图

> **目的**：列出 z42 当前缺失但中长期必备的标准库包，按重要程度分级，
> 参考 C# BCL 与 Rust std/生态的做法，给后续 spec proposal 提供优先级依据。
>
> **受众**：stdlib 设计者、新包提案者、roadmap reviewer。
>
> **配套文档**：
> - 架构契约：[stdlib.md](overview.md)（三层模型 + `[Native]` 规则）
> - 划分规则：[stdlib-organization.md](organization.md)（包边界 + 层级硬约束）
> - 当前实现状态：[../../src/libraries/README.md](../../src/libraries/README.md)（Extern 现状审计）
>
> **更新策略**：每完成一个新包后，把对应行从本文件移除（迁移到 stdlib-organization.md 现状表）；
> 优先级随语言能力（lambda / async / `Arc<Mutex>` 改造等）就绪而调整。

---

## 现状回顾（2026-04-30）

已实现包：`z42.cli` / `z42.core` / `z42.collections` / `z42.diagnostics` / `z42.encoding` / `z42.io`（含 `Std.IO.Binary`，原 `z42.io.binary` 已并入） / `z42.json` / `z42.math` / `z42.random` / `z42.regex` / `z42.test` / `z42.text` / `z42.time` / `z42.toml` / `z42.uri`。

覆盖能力：基础类型、协议接口、基础集合、Math、StringBuilder、Console + File / Directory / Path / Environment / Process、测试框架、编码（Hex/Base64/UTF-8）、UTC 时刻 + 时间段 + 单调计时器、TOML 1.0 子集 reader/writer、JSON RFC 8259 reader/writer。

**主要空缺**：并发、网络、加密、随机数。

---

## 分级原则

| 级别 | 含义 | 准入条件 |
|------|------|---------|
| **P0** | 必备 — 缺则无法用于任何实际项目 | 几乎所有应用都需要；不依赖未实现语言特性 |
| **P1** | 高价值 — 中型项目几乎一定会用 | 阻塞主流场景（并发 / 网络 / 加密）；可能依赖语言能力 |
| **P2** | 中等优先 — 特定场景必需 | 协议解析、压缩、配置、日志；按需排 |
| **P3** | 低优 — 生态成熟后再补 | 反射、国际化、SIMD、Pipelines |

每个包准入还需符合 [stdlib-organization.md](organization.md) §"开新包必备" 三条：
明确层级、依赖闭包、是否可纯脚本化。

---

## P0 — 必备

| 包 | C# 对标 | Rust 对标 | 层级 | extern | 备注 |
|----|---------|----------|------|--------|------|
| **z42.io.fs**（扩展现 `z42.io`）| `System.IO.File` / `Directory` / `Path` | `std::fs` / `std::path` | L2 | 走 Platform HAL | 当前 `z42.io` 仅 stdin/stdout/基础 File；缺 Directory 枚举、Path 完整 API、文件元数据 |
| **z42.os** / **z42.env** | `System.Environment` / `System.Diagnostics.Process` | `std::env` / `std::process` | L2 | 走 Platform HAL | 环境变量、命令行参数、退出码、子进程 spawn |

**起步排期**：L2 内可立刻起步（不依赖未实现语言特性），按 `z42.io.fs 扩展 → z42.os` 顺序。

**已落地（不在 P0 表）**：
- `z42.encoding`（2026-05-13 add-z42-encoding）— Hex / Base64 RFC 4648 §4 / UTF-8。详 [encoding.md](encoding.md)
- `z42.time`（2026-05-14 add-z42-time）— UTC 时刻（DateTime）/ 时间段（TimeSpan）/ 单调计时器（Stopwatch）。详 [time.md](time.md)
- `z42.toml`（2026-05-14 add-z42-toml）— TOML 1.0 subset reader/writer，覆盖 manifest 解析。详 [toml.md](toml.md)
- `z42.json`（2026-05-15 add-z42-json）— JSON RFC 8259 完整 reader/writer。详 [json.md](json.md)
- `z42.random`（2026-05-15 add-z42-random）— PCG-XSH-RR deterministic PRNG。详 [random.md](random.md)
- `z42.uri`（2026-05-15 add-z42-uri）— RFC 3986 子集 URI parser + percent codec。详 [uri.md](uri.md)
- `Std.IO.Binary`（2026-05-15 add-z42-io-binary；2026-08-31 由原独立包 `z42.io.binary` 并入 `z42.io`）— `BinaryReader/Writer` over byte[]，LE+BE int16/32/64 + UTF-8 string。详 [io-binary.md](io-binary.md)
- `z42.diagnostics`（2026-05-15 add-z42-diagnostics）— `Log` static facade + 5 level (TRACE/DEBUG/INFO/WARN/ERROR)，stderr 输出。详 [diagnostics.md](diagnostics.md)
- `z42.regex`（2026-05-16 add-z42-regex）— RFC 子集 regex parser + backtracking 匹配引擎；Compile/IsMatch/Find/FindAll/Replace/Split。详 [regex.md](regex.md)
- `z42.cli`（2026-05-16 add-z42-cli）— ArgParser + ParseResult + auto -h/--help；Phase 0 of shell-script → z42 self-hosting。详 [cli.md](cli.md)
- `z42.threading`（2026-05-20 add-threading-stdlib）— `Std.Threading.Thread` / `Std.ThreadException`：OS 线程级 `Start(Action)` / `Join()`，跨线程 exception 经 `ThreadException.Message` 透传。底层 `__thread_spawn` / `__thread_join` builtin 接 `VmCore.threads` slot table。
- `z42.threading` 扩展（2026-05-20 add-sync-primitives）— `Std.Threading.Mutex<T>` / `Channel<T>` + `Std.ChannelDisconnectedException`：RAII callback Mutex（`Lock(Func<T,T>)`）+ unbounded MPSC channel（`Send` / `Recv` / `TryRecv` / `Close`）。底层 `__mutex_*` × 4 + `__channel_*` × 5 builtin 接 `VmCore.mutexes` / `VmCore.channels` slot table；附带修复 `GcRef::borrow` 跨线程 panic（`try_lock` → blocking `lock`）。
- `z42.compression`（2026-05-24 add-z42-compression）— `Std.Compression.{Gzip, Zlib, Deflate, Zstd}` + `Std.Archive.{Tar, Zip}` + streaming `CompressionStream`。**首个 stdlib native 走出 z42vm**：独立 cdylib `libz42_compression.{so,dylib,dll}` 经 `native::ext` loader（搜索路径 `Z42_NATIVE_PATH` + `<exe>/../native/`）按需 dlopen 加载；wasm / 移动平台经 `bundled-compression` Cargo feature 静态 link 兜底。建立了所有未来重 native stdlib（z42.net / z42.numerics）共用的"out-of-VM" 模板。详 [compression.md](compression.md) + [native-ext-loader.md](../runtime/native-ext-loader.md)。
- `z42.yaml`（2026-05-24 add-z42-yaml）— YAML 1.2 subset reader/writer。纯脚本，镜像 z42.toml / z42.json 的 `XxxValue` discriminated-union 形态。覆盖 block mapping / sequence、flow `{}` `[]`、plain / single / double 引号、`# ...` 注释；不含 anchors / tags / multi-line `|` `>` / multi-doc / 复杂键（详 [yaml.md](yaml.md) Deferred 段）。
- `Std.IO.Stream` + `MemoryStream`（2026-05-24 add-z42-io-stream）— 统一流式 I/O base class（capability `CanRead`/`CanWrite`/`CanSeek` + core `Read(buf,off,n)` / `Write(buf,off,n)` + 可选 `Length`/`Position`/`Seek` + convenience `ReadAllBytes`/`WriteAllBytes`/`ReadExactly`）+ `byte[]`-backed `MemoryStream`。z42 无 abstract 关键字 — 用 concrete base + throw stubs 模式。为后续 `FileStream` / `NetworkStream` / `TextReader` / `BufferedStream` + `CompressionStream` / `BinaryReader` refactor 铺路。详 [io-stream.md](io-stream.md)。
- `z42.net` K1（2026-05-24 add-z42-net）— `Std.Net.Sockets.{TcpClient, TcpListener, NetworkStream}` 同步阻塞 TCP。VM-side `corelib/network.rs` 内嵌 `std::net::*`，不走 cdylib；slot table 镜像 `ProcessHandle` pattern。wasm32 throw `NetUnsupportedException`。详 [net.md](net.md)。
- `z42.net` K2（2026-05-25 add-z42-net-udp）— `Std.Net.Sockets.{UdpClient, UdpReceiveResult}` 同步阻塞 UDP。Auto-bind on first Send；ephemeral port discovery via `LocalPort()`. 复用 K1 的 kind-tuple shape + slot-table pattern。
- `z42.net` K3（2026-05-25 add-z42-net-http）— `Std.Net.Http.{HttpClient, HttpRequest, HttpResponse, HttpHeaders, HttpMethod, HttpStatusCode, HttpUrl}` HTTP/1.1 plaintext client. 纯脚本 over K1 TcpClient，无新 VM builtin. Content-Length + Transfer-Encoding: chunked incoming. http:// only — https:// throws `NotSupportedException` pending `add-z42-net-tls`.
- `z42.net` K4（2026-05-25 add-z42-net-websocket + add-z42-net-websocket-accept-validate）— `Std.Net.WebSockets.{WebSocketClient, WebSocketMessage, WebSocketMessageType, WebSocketState, ...}` RFC 6455 client over K1 TCP. 纯脚本，无新 VM builtin. ws:// only — wss:// 等 `add-z42-net-tls`. Handshake + single-frame send + multi-frame receive assembly + auto-pong + close handshake. **Sec-WebSocket-Accept validated via SHA-1 + base64** (RFC 6455 §4.2.2; landed same day as `add-sha1-to-crypto`)。HTTP/2 / async 仍是 follow-up.
- `z42.numerics` v0（2026-05-24 add-z42-numerics-bigint）— `Std.Numerics.BigInt` 任意精度整数。纯脚本，31-bit limb (int[]) + sign 表示；Add / Sub / Mul (schoolbook) / Div / Mod / Pow / Parse(decimal+hex) / ToString / ToHex / CompareTo / GetHashCode。Out of scope: 位运算 / ModPow / Gcd / Karatsuba / Vector / Complex / Decimal — 走 follow-up spec。详 [numerics.md](numerics.md)。

---

## P1 — 高价值

| 包 | C# 对标 | Rust 对标 | 层级 | extern | 阻塞依赖 |
|----|---------|----------|------|--------|---------|
| ~~z42.threading~~ | ~~`System.Threading.Thread`~~ | ~~`std::thread`~~ | ~~L2~~ | — | **✅ 已落地 2026-05-20**（add-threading-stdlib + add-sync-primitives 双 spec） |
| **z42.async** | `Task` + `async/await` + `CancellationToken` | `tokio` / `async-std` | L3 | 部分 native | **L3 async/await 语法**（roadmap L3）；标准库需先有 z42.threading 同步原语 |
| ~~z42.net K1~~ | ~~`System.Net.Sockets`~~ | ~~`std::net::Tcp*`~~ | ~~L2~~ | — | **✅ K1 已落地 2026-05-24**（add-z42-net）—— TCP-only。UDP / IPAddress / DNS / Timeout / TLS / HTTP / async 走 follow-up specs |
| **z42.crypto** | `System.Security.Cryptography` | `ring` / `sha2` / `aes` | L2 | FFI | ✅ SHA-1 / SHA-256 / SHA-384 / SHA-512 / SHA3-224/256/384/512 + SHAKE128/256 + Keccak-256/512 (legacy) + BLAKE2b + BLAKE2s + BLAKE3 + HMAC for SHA-1/256/384/512 + CSPRNG `SecureRandom` + PBKDF2 + HKDF-SHA-1/256/512 + scrypt + AES-128/192/256 (ECB + CTR + CBC-PKCS7 + GCM + CCM) + ChaCha20 + Poly1305 + ChaCha20-Poly1305 AEAD + X25519 ECDH + Ed25519 signing + RSA (PKCS#1 v1.5 + PSS sign/verify + OAEP encrypt/decrypt + raw RSAEP/RSADP) + ECDSA P-256 + ECDSA secp256k1 (both RFC 6979 deterministic) 已落地 (2026-05-24 → 2026-05-28)；hw-accel / Argon2 留 follow-up |

**起步排期**：
- L2 末 / L3 初：`z42.regex` ✅ → `z42.threading` ✅ → `z42.crypto` (SHA-256 ✅ + HMAC ✅) → `z42.net` K1 ✅
- L3 中（async 语法就绪后）：`z42.async`

---

## P2 — 中等优先

| 包 | C# 对标 | Rust 对标 | 层级 | 备注 |
|----|---------|----------|------|------|
| ~~z42.compression~~ | ~~`System.IO.Compression`~~ | ~~`flate2` / `zstd`~~ | ~~L2~~ | **✅ 已落地 2026-05-24** add-z42-compression — Gzip / Zlib / Deflate / Zstd + Tar / Zip Read。首个 stdlib native 走出 z42vm（cdylib + dlopen via `native::ext` loader），详 [compression.md](compression.md) + [native-ext-loader.md](../runtime/native-ext-loader.md) |
| ~~z42.yaml~~ | ~~(社区)~~ | ~~`serde_yaml`~~ | ~~L1~~ | **✅ 已落地 2026-05-24** add-z42-yaml — YAML 1.2 subset reader/writer，纯脚本镜像 z42.toml / z42.json |
| **z42.linq** | `System.Linq` | `Iterator` trait 链式方法 | L1 | **依赖 lambda + iterator trait**（L3） |

---

## P3 — 低优

| 包 | C# 对标 | Rust 对标 | 阻塞依赖 |
|----|---------|----------|---------|
| **z42.xml** | `System.Xml` | `quick-xml` | — |
| **z42.globalization** | `System.Globalization` | `icu` crate | — |
| **z42.reflection** | `System.Reflection` | (无原生) | **L3-R 反射轨道** |
| ~~z42.numerics~~ | ~~`System.Numerics.BigInteger`~~ | ~~`num`~~ | — | **✅ v0 BigInt 已落地 2026-05-24**（add-z42-numerics-bigint）。Vector / Complex / Decimal 留 follow-up specs |
| **z42.io.pipelines** | `System.IO.Pipelines` | `bytes` / `tokio` | 高吞吐 IO；依赖 z42.async |

---

## 决策记录

### 为什么不直接抄 C# BCL 全集

- **Script-First 约束**：`docs/design/philosophy.md` §8 要求纯脚本优先，
  能用 z42 写就不进 VM。这与 .NET BCL 大量依赖运行时 native 实现的思路不同
- **包边界对齐 Rust**：用细粒度独立 zpkg（z42.json / z42.regex 单包），
  而非 C# 的"一个 BCL 大盒子"，符合 [stdlib-organization.md](organization.md) §"Extension over Expansion"
- **不在 z42.core 扩展**：core 已 lock-in（见 stdlib-organization.md 规则 #6），
  P0/P1 大多数包都是新独立 zpkg，**不回溯改 core**

### 为什么 FFI 包（crypto / compression / regex）排在中等优先

- Tier 1/2 C ABI 已就绪（C8–C11，2026-04-29 完成）
- 包系统库（libsodium / zlib / PCRE2）比纯脚本实现快 10×+，且代码量少
- 但需先 P0 解决"基础"（时间、文件、编码），再用 FFI 包系统库锦上添花

### 为什么 z42.threading 原本排 P1（已于 2026-05-20 提前落地）

- 单线程脚本 + GC 已能跑大多数应用（Python / Node.js 单线程也能撑住主流场景）
- 多线程依赖 Value 类型 `Rc<RefCell>` → `Arc<Mutex>` 改造（roadmap A6），
  这是底层架构变更，不能轻量提前
- 延后 P0 等基础齐了一并设计，避免 API 反复

**实际落地路径**（2026-05-20）：A6 改造拆成三个独立 spec 顺次完成 —
`add-multithreading-foundation`（Value/GC backing → `Arc<Mutex>`）→
`add-vmcontext-registry`（GC 扫描跨 VmContext）→
`add-threading-stdlib`（`Std.Threading.Thread`），让 P1 第一品提前到位。
P1 剩余条目（`z42.threading.sync` / `z42.crypto` / `z42.net` / `z42.async`）
照原计划推进。

---

## 与 roadmap.md 的关系

`docs/roadmap.md` "标准库（基础）"段保持精简（只列已完成包），具体扩展计划全部
留在本文件。每完成一个新包后：

1. 从本文件对应级别移除该行
2. 添加到 [stdlib-organization.md](organization.md) "现状" 表
3. 更新 [stdlib.md](overview.md) "Module Catalog" 段（如有新对外 API）
4. `docs/roadmap.md` 标准库段添加一行简述

---

## 不在本文件范围

以下不属于"包级扩展"，不在本路线图：

- VM 内部 intrinsic 调整（属 stdlib.md L1 层）
- 现有包的 bug fix / 小特性补丁（直接走 docs/spec/changes/）
- 语言特性扩展（lambda / async / 反射）— 见 [language-overview.md](../language/language-overview.md) + roadmap L3 段

---

## Deferred / Future Work

### z42.time 延后项索引（详见 [time.md](time.md) Deferred 段）

| ID | 标题 | 前置依赖 |
|----|------|---------|
| ~~`time-future-calendar`~~ | **✅ 已落地 2026-05-28** (`add-datetime-calendar`) — UTC `Year` / `Month` / `Day` / `Hour` / `Minute` / `Second` / `Millisecond` / `DayOfWeek` / `DayOfYear` 访问器 + 静态 `Utc(y,m,d,h,m,s,ms)` / `UtcDate(y,m,d)` 构造器 + `IsLeapYear` / `DaysInMonth` 谓词 + `AddDays`/`AddHours`/`AddMinutes`/`AddSeconds`/`AddMilliseconds`/`AddMonths`/`AddYears` 算术（month-end 截断匹配 .NET / Python dateutil）；时区数据库仍延后 | — |
| `time-future-format-parse` (ISO 8601 format + parse 均已落地 2026-05-27; strftime/format-string 仍延后) | `ToIso8601` / `ToIso8601With(tz)` + `ParseIso8601(string)` 全 round-trip；`strftime` 风格式说明符待 L3 IFormattable | — |
| ~~`time-future-sleep-timer`~~ | **✅ 已落地 2026-05-30** (`add-z42-threading-timer`) — `Std.Threading.Timer.{StartPeriodic, StartOnce, Stop, StopAndJoin, IsRunning}` pure-script over Thread + Sleep; 100ms 颗粒度 chunked sleep 让 Stop 响应 ≤100ms 无论 intervalMs；callback 抛 swallow + Log.Error；不重叠调用。Thread.Sleep 已 5-27 落地（add-thread-sleep）| — |
| ~~`time-future-datetime-offset`~~ | **✅ 已落地 2026-06-03 (`add-datetime-offset`)** — `Std.Time.DateTimeOffset` `(DateTime utc, TimeZone tz)` 对，BCL parity API；workaround 安置在 `DateTime.z42` 同文件以绕开 z42.time 包专属 TypeChecker E0402 bug | — |
| ~~`time-rename-bench-now-ns`~~ | **✅ 已落地 2026-06-03 (`rename-bench-now-ns-to-time-mono`)** — VM builtin + 全部 z42 caller + 设计文档同步重命名 | — |

### z42.compression 延后项索引（详见 [compression.md](compression.md) Deferred 段）

| ID | 标题 | 前置依赖 |
|----|------|---------|
| ~~`compression-future-zip-write`~~ | **✅ 已落地 2026-05-27** (`add-zip-write`) — `Zip.Write(ZipEntry[]) → byte[]`; STORE + DEFLATE; CRC-32 (poly 0xEDB88320); single-buffer 2-pass via `_GrowBuf` (sidesteps absent `byte[][]`); GPBF UTF-8 marker; ZIP64 / encryption 仍延后 | — |
| ~~`compression-future-streaming-decode`~~ | **✅ 已落地 2026-05-27** (`add-compression-streaming-decode`) — cdylib 用 `flate2::write::*Decoder` + `zstd::stream::write::Decoder` 替代 v0 的 `Vec<u8>` accumulator；`compressor_feed` 现在 per-chunk 返回解码输出 | — |
| ~~`compression-future-brotli`~~ | **✅ 已落地 2026-05-27** (`add-z42-compression-brotli`) — pure-Rust `brotli` crate (no C deps, works on wasm); `Std.Compression.Brotli.{Compress(data[,level])/Decompress}`; level 0..=11 (default 4 — quality-11 is O(seconds) on small inputs even in optimised builds); unlocks `net-future-http-compression` brotli portion | — |
| `compression-future-xz-lz4` | xz / LZ4 算法 | 具体用例驱动 |
| `compression-future-libdeflate-batch` | libdeflate 批量快通道 | bench 证据显示 batch 操作是瓶颈 |
| `compression-future-zstd-dictionary` | zstd preset dictionary | 小 payload 高 ratio 用例 |
| `compression-future-zip-encryption` | Zip 加密 read/write | 具体用例驱动 |
| `compression-future-wasm-zstd` | wasm 上 zstd 支持（需 WASI SDK 或纯 Rust zstd 实现） | WASI SDK 进入 z42 dev env 或 ruzstd 成熟 |

### z42.net 延后项索引（详见 [net.md](net.md) Deferred 段）

| ID | 标题 | 前置依赖 |
|----|------|---------|
| ~~`net-future-udp`~~ | ~~UDP sockets (`UdpClient`)~~ | **✅ 已落地 2026-05-25 (add-z42-net-udp K2)** |
| ~~`net-future-udp-connected`~~ | **✅ 已落地 2026-05-27** (`add-z42-net-udp-connected`) — `Connect(host, port)` + `SendConnected(data[, length])` arity overload + `Disconnect()` / `IsConnected()`; stored default address, no kernel-level `connect(2)` (matches BCL informational-default semantics) | — |
| ~~`net-future-udp-multicast`~~ | **✅ 已落地 2026-05-27** (`add-z42-net-udp-multicast`) — `JoinMulticastGroup(group[, iface])` / `LeaveMulticastGroup(...)` / `SetMulticastLoop(bool)` for IPv4 (224.0.0.0/4); pre-checks `is_multicast()`; multiple group memberships on same socket supported; IPv6 multicast deferred | — |
| ~~`net-future-udp-recv-into`~~ | **✅ 已落地 2026-05-27** (`add-z42-net-udp-recv-into`) — `ReceiveInto(buf, offset, count)` + `ReceiveIntoFull(buf)` convenience; sender address via `LastReceiveHost`/`LastReceivePort` (avoids byte[]+returning-tuple overload mess); silent truncation when datagram > buffer | — |
| ~~`net-future-ipaddress`~~ (IPAddress 已落地 2026-05-27, IPEndPoint 已落地 2026-06-02) | `Std.Net.Sockets.IPAddress.Parse(string)` — IPv4 dotted-quad + IPv6 with `::` shorthand; `IPEndPoint(IPAddress, port)` + `addr:port` / `[addr]:port` URL-authority parse/format round-trip. IPv4-in-IPv6 dotted form (`net-future-ipaddress-v4mapped`) + zone-id suffix (`net-future-ipaddress-zoneid`) 仍延后 | — |
| ~~`net-future-dns`~~ (基本 `GetHostAddresses` 已落地 2026-05-27; SRV/MX/PTR 仍延后) | `Std.Net.Sockets.Dns.GetHostAddresses(host) → IPAddress[]` synchronous resolver via libc `getaddrinfo`; mixed v4 + v6 results, dedup'd; numeric inputs round-trip without network hit | — |
| ~~`net-future-timeout`~~ | **✅ 已落地 2026-05-30** (`add-net-socket-options-extended`) — `TcpClient.SetConnectTimeout(ms)` (routes to `TcpStream::connect_timeout`); `UdpClient.SetReadTimeout(ms)` / `SetWriteTimeout(ms)`. TCP read/write 已 5-27 落地。`millis <= 0` 清 timeout（同 BCL 语义） | — |
| `net-future-tls` | TLS / HTTPS (rustls / OpenSSL cdylib) | HTTP client / 安全 RPC |
| ~~`net-future-http`~~ | ~~HTTP/1.1 client (`HttpClient.Get/Post`)~~ | **✅ 已落地 2026-05-25 (add-z42-net-http K3)** — http:// only; https:// 等 `net-future-tls` |
| ~~`net-future-http-keepalive`~~ | **✅ 已落地 2026-05-28** (`add-z42-net-http-keepalive`) — `HttpClient.SetKeepAlive(true)` opt-in; parallel-array per-host pool (cap 8, oldest-evict); 1-shot silent retry on stale-connection; `Dispose()` flushes pool | — |
| ~~`net-future-http-stream-body`~~ | **✅ 已落地 2026-05-28** (`add-z42-net-http-stream-body`) — `HttpClient.SendStreaming(request)` returns `HttpResponse` with `BodyStream: Stream` populated and `Body: byte[0]`; new `_HttpBodyStream : Stream` handles Content-Length / chunked / read-until-EOF framing transparently via Stream.Read; caller MUST close the stream to release the TCP socket; auto-decompress / auto-redirect bypassed (compose with Gzip.WrapRead) | — |
| ~~`net-future-http-redirects`~~ | **✅ 已落地 2026-05-26** (`add-z42-net-http-redirects`) — `SetMaxRedirects(n)`; auto-follows 301/302/303 (switch to GET) + 307/308 (preserve method); cookie jar replays on hop | — |
| ~~`net-future-http-cookies`~~ | ~~Set-Cookie 解析 + cookie jar~~ | **✅ 已落地 2026-05-26 (add-z42-net-http-cookies)** — Cookie + CookieJar 类；manual ingest + lookup (auto-jar on HttpClient is `add-z42-net-http-cookies-auto` follow-up) |
| ~~`net-future-http-cookies-auto`~~ | **✅ 已落地 2026-05-26** (`add-z42-net-http-cookies-auto`) — `HttpClient.SetCookieJar(jar)`; auto-ingest Set-Cookie on response + auto-emit Cookie header on request | — |
| ~~`net-future-http-cookies-persist`~~ | **✅ 已落地 2026-05-27** (`add-cookie-jar-persist`) — `CookieJar.SaveToFile/LoadFromFile` in Netscape "cookies.txt" format (curl/wget interop); `#HttpOnly_` prefix preserved | — |
| ~~`net-future-http-auth`~~ | **✅ 全部已落地** — Basic + Bearer 5-27；**Digest 5-31** (`add-z42-net-http-digest-auth`) `HttpRequest.WithDigestAuth(user, pass)` + `HttpClient` 401 challenge-response auto-retry；支持 algorithm=MD5 + SHA-256 + qop=auth。MD5 哈希 5-31 同日 ship (`add-md5-to-crypto`)。auth-int / -sess / SHA-512-256 / userhash / stale / proxy Digest 留 `net-future-http-digest-extras` follow-up | — |
| ~~`net-future-http-compression`~~ | **✅ 已落地 2026-05-30** (`add-z42-net-http-brotli`) — `HttpClient.SetAutoDecompress(true)` 现广播 `Accept-Encoding: gzip, br` 并透明解 gzip 或 brotli 响应；gzip 5-27 落地，brotli 30 日补齐。同次修复 `Content-Encoding` absent 时 `.ToLower()` 解引用 null 的 latent bug | — |
| ~~`net-future-http-server`~~ | ~~`HttpListener` / `HttpServer`~~ | **✅ 已落地 2026-05-25 (add-z42-net-http-server)** — single-threaded sequential; 12 tests; multi-threaded variant is `add-z42-net-http-server-threaded` follow-up |
| ~~`net-future-http-server-threaded`~~ | ~~each accept dispatched to `Std.Threading.Thread` worker~~ | **✅ 已落地 2026-05-26 (add-z42-net-http-server-threaded)** — `ServeThreaded(handler)`; fire-and-forget worker threads |
| ~~`net-future-http-server-pool`~~ | **✅ 已落地 2026-05-28** (`add-z42-net-http-server-pool`) — `HttpServer.ServeWithPool(handler, workerCount, queueSize)` pre-spawns N worker threads consuming from a bounded `Channel<TcpClient>`; accept loop uses `TrySend` so spikes drop excess connections instead of OOMing the process (DoS-resistance vs ServeThreaded's unbounded thread spawn) | — |
| `net-future-http2` | HTTP/2 binary framing + HPACK | gRPC 等高吞吐用例 |
| ~~`net-future-websocket`~~ | ~~WebSocket protocol (RFC 6455) over K1 TCP~~ | **✅ 已落地 2026-05-25 (add-z42-net-websocket K4 + accept-validate)** — ws:// only; wss:// 等 `net-future-tls` |
| ~~`net-future-websocket-accept-validate`~~ | ~~Sec-WebSocket-Accept SHA-1 verification client-side~~ | **✅ 已落地 2026-05-25** — depends on `add-sha1-to-crypto` which shipped same day |
| ~~`net-future-websocket-server`~~ | **✅ 已落地 2026-05-27** (`add-z42-net-websocket-server`) — `Std.Net.WebSockets.WebSocketServer.{Bind/Accept/LocalPort/Stop}` + `WebSocketConnection.{SendText/SendBinary/SendPing/Receive/Close/Dispose}`; reuses `_FrameCodec.ReadFrame` + new `EncodeServerFrame` (no MASK per RFC §5.1); 自动 Pong；ws:// only | — |
| ~~`net-future-websocket-deflate`~~ | **✅ 已落地 2026-05-28** (`add-z42-net-websocket-deflate`) — `WebSocketConnection.SetPermessageDeflate(bool)` opt-in; `_FrameCodec` thread RSV1 through encode + decode; deflate-strip-tail (`00 00 FF FF`) on send + append on receive per RFC 7692 §7.2.1; `*_no_context_takeover` variant (each message independent); inbound RSV1 with deflate-off throws WebSocketProtocolException | — |
| `net-future-async` | async/await sockets | L3 async/await 语法 |
| ~~`net-future-socket-options`~~ + ~~`net-future-keepalive-tuning`~~ | **✅ 全员落地** — Nagle / IP_TTL 2026-05-27；SO_REUSEADDR + SO_KEEPALIVE 2026-05-30 (`add-net-socket-options-extended`)；SO_KEEPALIVE 4-arg tuning 2026-06-03 (`add-net-keepalive-tuning`) per-OS idle/interval/probes via socket2 `TcpKeepalive`. SO_REUSEADDR 仅 pre-Bind 合法（post-Bind 抛 InvalidOperationException, 对齐 BCL ExclusiveAddressUse 语义） | — |
| `net-future-wasm-wasi-sockets` | wasm32 真实 socket via WASI-sockets | WASI-sockets 标准 stable + runtime 支持 |

### z42.crypto 延后项索引（详见 [crypto.md](crypto.md) Deferred 段）

| ID | 标题 | 前置依赖 |
|----|------|---------|
| ~~`aes-future-cbc`~~ | **✅ 已落地 2026-05-27** (`add-aes-cbc-pkcs7`) — `Aes.EncryptCbcPkcs7(key, iv, data)` / `Aes.DecryptCbcPkcs7(...)`; PKCS#7 padding (RFC 5652 §6.3); padding-validation rejects tampered ciphertext; verified against NIST SP 800-38A §F.2.1 vector | — |
| ~~`sha3-future-keccak-legacy`~~ | **✅ 已落地 2026-05-27** (`add-sha3-keccak-legacy`) — `Sha3.KeccakLegacy256` / `KeccakLegacy512` and their String/Hex/StringHex siblings; parameterised `_sponge` shares the FIPS 202 plumbing, only domain byte (0x01 vs 0x06) differs; verified against Ethereum `keccak256("")` + `keccak256("abc")` reference values | — |
| ~~`aes-future-gcm`~~ | **✅ 已落地 2026-05-27** (`add-aes-gcm`) — pure-script GHASH (bit-by-bit GF(2^128)) + GCTR + auth-tag; 96-bit IV path only (non-96-bit deferred to `aes-future-gcm-non96iv`); 16-byte tag fixed; constant-time tag compare; verified against gcm-spec §B.1-4 (AES-128) + §B.15 (AES-256) reference vectors | — |
| ~~`aes-future-gcm-non96iv`~~ | **✅ 已落地 2026-05-28** (`add-aes-gcm-non96iv`) — `_gcmDeriveJ0` dispatches 96-bit fast path (J0 = IV \|\| 0x00000001) vs GHASH-derived J0 = GHASH_H(IV \|\| pad \|\| len(IV)_64bit) per NIST SP 800-38D §7.1; verified against gcm-spec §B Test Case 5 (64-bit IV) + arbitrary 7-byte IV round trip; empty-IV rejected | — |
| `aes-future-hw-accel` | AES-NI / ARMv8 Crypto Extensions | crypto cdylib backend; 高吞吐用例 |
| `crypto-future-csprng-wasm32` | wasm32 `SecureRandom` 桥接到 `crypto.getRandomValues` / WASI `random_get` | wasm32 target |

### z42.numerics 延后项索引（详见 [numerics.md](numerics.md) Deferred 段）

| ID | 标题 | 前置依赖 |
|----|------|---------|
| ~~`bigint-future-bitops`~~ + ~~`bigint-future-bitops-twoscomp`~~ | **✅ AND/OR/XOR/Shift/TestBit/BitLength 已落地 2026-05-25** + **two's-complement negative-operand semantics + BitNot 已落地 2026-05-27** (`add-bigint-bitops-twoscomp`) — Python-style `-1 & 7 == 7`, `(-2).TestBit(0) == false / TestBit(1) == true`; 36 tests GREEN | — |
| ~~`bigint-future-modpow`~~ + ~~`bigint-future-modpow-negexp`~~ + ~~`bigint-future-modinverse`~~ + ~~`bigint-future-prime`~~ + ~~`bigint-future-prime-sieve`~~ + ~~`bigint-future-modpow-montgomery`~~ + ~~`bigint-future-prime-deterministic`~~ + ~~`bigint-future-bpsw`~~ | **✅ ModPow / ModInverse / IsProbablyPrime / NextPrime / Miller-Rabin + small-prime sieve + Montgomery REDC 全部已落地 2026-05-25 → 2026-05-28；deterministic `IsPrime()` 2026-06-02；`IsBpswPrime()` 2026-06-03** — 三 primality method 全员到齐 (probabilistic / deterministic-bounded / Baillie-PSW)；RSA / ECDH / Ed25519 / ECDSA primitives 自动 Montgomery (odd modulus) | — |
| ~~`bigint-future-gcd`~~ + ~~`bigint-future-gcd-binary`~~ | **✅ Gcd / Lcm 已落地 2026-05-25** + **Stein binary GCD 已落地 2026-05-27** (`add-bigint-gcd-binary`) — replaces Euclidean divmod with shift+subtract; ~3-5× faster on crypto-size operands; 18/18 existing GCD tests pass unchanged | — |
| ~~`bigint-future-karatsuba`~~ + `bigint-future-fft` | **✅ Karatsuba 已落地 2026-05-25** (threshold 32 limbs, ~10× speedup at 1024-bit); FFT (Schönhage–Strassen) 仍延后, requires bench evidence at 10k-bit+ workload | — |
| `bigint-future-operator-overloads` | `a + b` / `a * b` 运算符语法 | L3 generic op overload |
| `numerics-future-vector` | `Std.Numerics.Vector<T>` SIMD | 图形 / ML 用例 |
| ~~`numerics-future-complex`~~ | **✅ 已落地 2026-05-27** (`add-numerics-complex`) — immutable `(real, imag)` over `double`, BCL API parity (Magnitude / Phase / FromPolar / Add / Subtract / Multiply{[Real]} / Divide [Smith] / Negate / Conjugate / Reciprocal / Exp / Log / Sqrt / Sin / Cos / Pow); 24 tests cover the 3-4-5 magnitude, i² == -1, Euler's identity, sqrt(-1) == i, sqrt round-trip | — |
| ~~`numerics-future-decimal`~~ | **✅ 已落地 2026-05-27** (`add-numerics-decimal`) — arbitrary-precision base-10 fixed-point over `(BigInt mantissa, int scale)`; `Parse / FromInt/FromLong`, `Add / Subtract / Multiply / DivideBy(other, scale)` truncated-toward-zero, `Negate / Abs / CompareTo / Equals`; `ToString` preserves stored scale (1.00 stays "1.00"); 26 tests cover currency addition + repeating-decimal truncation | — |

### z42.yaml 延后项索引（详见 [yaml.md](yaml.md) Deferred 段）

| ID | 标题 | 前置依赖 |
|----|------|---------|
| ~~`yaml-future-anchors`~~ | **✅ 已落地 2026-05-25** (`add-yaml-anchors-aliases`) — `&name` anchor + `*name` alias; per-doc scope; alias resolves via DeepClone so mutating one alias never leaks; 17 tests covering kubectl/helm `defaults: &defaults` pattern + sequence/mapping anchors + multi-alias-to-same-anchor + per-doc reset | — |
| ~~`yaml-future-tags`~~ | **✅ 已落地 2026-05-25** (`add-yaml-tags`) — YAML 1.2 universal tags `!!str` / `!!int` / `!!float` / `!!bool` / `!!null` for explicit coercion; canonical use case `!!str 42` → `"42"` (Kubernetes ConfigMap); 20 tests cover round-trip + malformed-value rejection | — |
| ~~`yaml-future-multiline-strings`~~ | **✅ 已落地 2026-05-25** (`add-yaml-multiline-strings`) — `\|` literal + `>` folded block scalars with full chomping (`-` strip / `+` keep / default clip) + optional indent indicator; 18 tests cover blank-line preservation, folded space-folding, deeper-indented content | — |
| ~~`yaml-future-multi-doc`~~ | **✅ 已落地 2026-05-25** (`add-yaml-multi-doc`) — `YamlValue.ParseAll(string)` / `ParseAllStream(Stream) -> YamlValue[]` for kubectl-style stacked manifests (`---` separator); single-doc `Parse()` rejects multi-doc with hint; 17 tests cover separator variations + null docs + comments-between-docs | — |
| `yaml-future-complex-keys` | sequence / mapping 作为 key（`? key` 语法） | 用例驱动 |
| ~~`yaml-future-timestamps`~~ | **✅ 已落地 2026-05-27** (`add-yaml-timestamps`) — YamlValue kind 7 + `OfTimestamp(DateTime)` / `OfTimestampString(iso)` factories + raw-lexeme preservation for round-trip; parser auto-detects `YYYY-MM-DD[T...]` shapes; depends on `DateTime.ParseIso8601` (shipped same day) | — |
| ~~`yaml-future-numeric-bases`~~ | **✅ 已落地 2026-05-25** (`add-yaml-numeric-bases`) — `0xFF` / `0o755` int literals recognised before decimal inference in `_ParseScalarLiteral` | — |

### Std.IO.Stream 延后项索引（详见 [io-stream.md](io-stream.md) Deferred 段）

| ID | 标题 | 前置依赖 |
|----|------|---------|
| ~~`io-stream-future-filestream`~~ | **✅ 已落地 2026-05-24** — `Std.IO.FileStream : Stream` + `FileMode.Read/Write/Append`，8 个 corelib builtin (`__file_*`) 走 `VmCore.file_handles` slot table | — |
| ~~`io-stream-future-textreader`~~ | **✅ 已落地 2026-05-28** (`add-textreader-base` / `add-textwriter-base`) — `Std.IO.TextReader` + `TextWriter` abstract bases (concrete + throw-stubs, mirroring `Stream` pattern). Single-arity-per-name virtual core (`Peek`/`Read`/`Close` + `Write(string)`/`Flush`/`Close`) plus non-virtual default-impl helpers (`ReadLineBase`/`ReadToEndBase`/`ReadBufferBase` + `WriteCharsBase`/`WriteCharBase`/`WriteLineBase`) that compose on the virtual core; CRLF + lone-CR line splitting handled via Peek lookahead. Existing StringReader/StreamReader/StringWriter/StreamWriter keep their standalone shape (z42's name-only override resolver makes inheriting overloaded bulk-Read/Write counterproductive — see `compiler-future-typed-overload-resolution` Deferred) — TextReader/TextWriter are available for new custom subclasses | — |
| ~~`io-stream-future-bufferedstream`~~ | **✅ 已落地 2026-05-24**（add-buffered-stream）— single-buffer Read/Write batching wrapper，4 KB default，大 IO bypass | — |
| `io-stream-future-async` | `ReadAsync` / `WriteAsync` | L3 async/await |
| ~~`io-stream-future-copy-to`~~ | **✅ 已落地 2026-05-24**（add-stream-copy-to）— `Stream.CopyTo(Stream)` + `CopyTo(Stream, int)`，4 KB default buffer | — |
| ~~`refactor-compression-stream-on-iostream`~~ | **✅ 已落地 2026-05-24** — CompressionStream 删除，改 generic `_CompressionEncoderStream / _CompressionDecoderStream extends Stream` + `Gzip/Zlib/Deflate/Zstd.WrapWrite(Stream) / WrapRead(Stream)` | — |
| ~~`refactor-binary-reader-stream`~~ | **✅ 已落地 2026-05-24** — BinaryReader/Writer 后端 byte[] → Stream，新增 `(Stream)` 构造器；byte[] 构造保留作 sugar | — |
| ~~`process-stream-stdio`~~ | **✅ 已落地 2026-05-24** — `ProcessStdinStream` + `ProcessOutputStream(fd)`，`ProcessHandle.GetStdinStream/GetStdoutStream/GetStderrStream`；2 个 `__process_handle_read_*` builtin | — |
| ~~`add-z42-io-string-reader-writer`~~ | **✅ 已落地 2026-05-24** — char-oriented `StringReader` (`Peek` / `Read` / `ReadLine` / `ReadToEnd`) + `StringWriter` (`Write` / `WriteLine` / `ToString` / `Clear`)，纯脚本，无 VM 改动 | — |
| ~~`io-stream-future-streamreader-writer`~~ | **✅ 已落地 2026-05-24**（add-encoding-and-stream-text）— `Std.Encoding.Encoding` + `Std.IO.StreamReader(Stream[, Encoding])` + `StreamWriter(Stream[, Encoding])`，v0 drain-and-decode | — |
| `io-stream-future-streamreader-chunked` | True chunked-decode `Decoder` for unbounded / 10 GB streams | stateful `Encoding.GetDecoder()` API |
| `io-stream-future-objectdisposed` | `ObjectDisposedException` + `_closed` 追踪 | z42 IDisposable / using 语法 |
| ~~`io-stream-future-end-of-stream-exception`~~ | **✅ 已落地 2026-05-27** (`add-end-of-stream-exception`) — `Std.EndOfStreamException` extends Exception, ReadExactly switched | — |

### z42 build-driver prerequisites（2026-05-13）

**触发来源**：仓库 build / test 脚本目前全是 bash（`scripts/*.sh` + `src/toolchain/workload/*/platform/build.sh`），不能在 Windows 上跑。要让 Tier 1 Windows CI 工作，有 4 条路（xtask / Python driver / PowerShell 双维护 / Git Bash）—— 2026-05-13 决策**全部放弃**，长期目标是**用 z42 自身重写所有 build / test 脚本**：编译为 `.zbc` 单文件，由 `z42vm` 跨平台执行，与 z42 自举路线一致（参 [`roadmap.md`](../../roadmap.md) L4 自举段）。

**问题**：直接做不可行，z42 stdlib 缺一组 build driver 必需的原语。即便先用 native interop（`extern "C"`）shell out 到 libc，也只解决 POSIX，不解决 Windows（Win32 API 还得另封一层）—— 必须等下面这些**跨平台抽象**层在 stdlib 中先到位。

**阻塞清单**（每项对照本路线图分级；✅ = 已落地）：

| 原语 | 本路线图分级 | 用途 | 状态 |
|------|------------|------|------|
| `Std.IO.Process.{Run, Spawn}` 带 stdout capture / exit code / timeout / stdio 四态 | **P0 z42.io.process** | spawn `cargo build` / `dotnet test` / `gradle` / `xcodebuild` / `wasm-pack` | ✅ 2026-05-14 add-std-process（在 z42.io 而非独立 z42.os 包；与 File / Directory / Path / Environment 一起共享 host FFI 边界）|
| `Std.IO.File.{ReadAllText, WriteAllText, Exists, Copy, Move, Delete}` | **P0 z42.io.fs** | 读 versions.toml / 写 manifest.toml / 拷 zpkg | ✅ 2026-05-12 add-std-io-polish |
| `Std.IO.Directory.{Create, Enumerate, Exists}` | **P0 z42.io.fs** | mkdir -p / 扫 src/tests/<cat>/<name>/ | ✅ 2026-05-13 add-std-io-directory |
| `Std.IO.Path.{Combine, GetDirectoryName, GetExtension, IsRooted, ...}` | **P0 z42.io.fs** | 跨平台 path 拼接（避免 `/` vs `\` 错误） | ✅ 2026-05-12 add-std-io-polish |
| `Std.Environment.{GetVar, SetVar}` + `Std.Environment.GetCommandLineArgs()` | **P0 z42.os** + 已有 | 读 `$ANDROID_NDK_HOME` / argv 解析 | ✅ 2026-05-12 add-std-io-polish |
| `Std.Toml.TomlValue.Parse(text)` / `Stringify(root)` | **P2 z42.toml** | 解析 versions.toml；shell 端目前用 python3 + tomllib | ✅ 2026-05-14 add-z42-toml |
| `Std.Net.Http.Get(url) -> stream` 或 shell out 到 curl | **P1 z42.net**（先 curl 顶着）| NDK / SDK 压缩包下载 | — |
| `Std.Crypto.SHA256` 或 shell out 到 shasum/openssl | **P1 z42.crypto**（先 shasum 顶着）| 下载校验 | — |
| `Std.IO.Compression.Zip.Extract` 或 shell out 到 unzip | **P2 z42.compression**（先 unzip 顶着）| NDK zip 解压 | — |

**触发条件 / 可恢复推进点**：

1. **P0 三件套先到位**（z42.os Process / z42.os Environment / z42.io.fs File+Directory+Path）—— 估计与 L2 M7-M8 标准库主体推进同节奏。这三项落地后，**大多数 bash 脚本**（`test-all.sh` / `test-vm.sh` / `test-cross-zpkg.sh` / `test-stdlib.sh` / `regen-golden-tests.sh`）已经可以 1:1 迁成 `.z42`（shell out 到 curl / unzip / shasum 暂时顶住下载类）
2. **P1 z42.crypto + P1 z42.net 中后期** —— `setup-tools.sh` 的 NDK 下载 + sha256 校验从 shell-out 升级为纯 z42 端 API
3. **P2 z42.toml + z42.compression** —— `versions.toml` 解析改 z42 原生；NDK zip 解压改 z42 原生
4. **打通后**：删除 `scripts/_lib/versions.sh`（python3 依赖也跟着下）+ 所有 `.sh` → `.z42`，CI 第一步 `cargo build z42vm; ./z42vm scripts/test-all.zbc`，Tier 1 Windows 通

**当前 workaround**：

- 仓内继续 bash 驱动（macOS / Linux dev workflow 不受影响）
- Windows dev：用 WSL / Git Bash（接受 path 翻译等 quirks）；Tier 1 Windows CI 推迟到 z42 build-driver 落地
- 不投资中间态：**不做 xtask（Rust）**、**不做 PowerShell 双维护**、**不做 Python driver** —— 中间态的代码在 z42 driver 上线后必删，做了就是技术债

**与 stdlib P0/P1/P2 顺序的关系**：这条 backlog **不**强行抢占现有 P0 排期（`z42.io.fs 扩展 → z42.os → z42.json`）。z42.os / z42.io.fs 推进时附带验证它们能驱动 build script 即可。
