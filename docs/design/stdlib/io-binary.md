# z42.io.binary — Low-level binary stream

> 落地版本：2026-05-15（add-z42-io-binary），2026-05-16 升级到 `Std.IO.Binary`（fix-deep-namespace-import-discovery），2026-05-24 后端 byte[] → `Std.IO.Stream`（refactor-binary-reader-stream），2026-08-31 由独立包 `z42.io.binary` **并入 `z42.io`**（library-review PR-C，碎包合并；命名空间不变）
> 包路径：`src/libraries/z42.io/src/Binary{Reader,Writer,Exception}.z42`（原独立包 `z42.io.binary` 已并入 `z42.io`）
> 命名空间：`Std.IO.Binary`（`BinaryException` 在 `Std`）

## 职责

`BinaryReader` / `BinaryWriter` 提供 byte / int16 / int32 / int64 显式字节序
（LE + BE）读写 + UTF-8 string + 原始 byte[] helper + cursor 管理。

**对标**：C# `System.IO.BinaryReader/Writer` + Rust `byteorder` crate。

**适用场景**：自定义二进制协议、二进制文件格式、`.zbc` introspection、跨进程
binary marshal、CTF / forensics 工具。

## API surface

```z42
class BinaryReader {
    BinaryReader(byte[] data)             // convenience — wraps in MemoryStream
    BinaryReader(Std.IO.Stream src)        // NEW (refactor 2026-05-24)
    int  GetPosition() / GetLength()       // throws BinaryException if !CanSeek
    bool EndOfStream()                     // throws BinaryException if !CanSeek
    void Seek(int pos) / Skip(int count)   // throws BinaryException if !CanSeek
    int  ReadByte()                        // 0..255
    int  ReadInt16LE() / ReadInt16BE()     // sign-extended
    int  ReadInt32LE() / ReadInt32BE()     // sign-extended
    long ReadInt64LE() / ReadInt64BE()
    byte[] ReadBytes(int count)
    string ReadString(int byteCount)       // UTF-8
}

class BinaryWriter {
    BinaryWriter()                         // default — internal MemoryStream
    BinaryWriter(int initialCapacity)      // hint only (MemoryStream grows on demand)
    BinaryWriter(Std.IO.Stream dest)        // NEW (refactor 2026-05-24)
    int  GetLength()                       // throws BinaryException if !CanSeek
    void Clear()                           // throws if !_ownsStream
    byte[] ToArray()                       // throws if !_ownsStream (caller-supplied dest)
    void WriteByte(int b)                  // low 8 bits
    void WriteBytes(byte[] data)
    void WriteInt16LE(int v) / WriteInt16BE(int v)
    void WriteInt32LE(int v) / WriteInt32BE(int v)
    void WriteInt64LE(long v) / WriteInt64BE(long v)
    int  WriteString(string s)             // returns bytes written
}

class BinaryException : Exception     // in Std namespace
```

## Pipeline composition (refactor 2026-05-24)

```z42
using Std.IO;
using Std.IO.Binary;
using Std.Compression;

// Read structured binary from a gzip-encoded source:
Stream gz = Gzip.WrapRead(new MemoryStream(compressedBytes));
BinaryReader r = new BinaryReader(gz);
int magic = r.ReadInt32LE();
long size = r.ReadInt64LE();

// Write structured binary into a destination stream (also works with
// future FileStream / NetworkStream — no API change):
MemoryStream dest = new MemoryStream();
BinaryWriter w = new BinaryWriter(dest);
w.WriteInt32LE(0xDEADBEEF);
w.WriteString("hello");
byte[] payload = dest.ToArray();
```

The byte[] constructor and `ToArray()` / `Clear()` continue to work
for the common "I just have a byte[]" case — internally they
delegate to a MemoryStream so all existing v0 callers see no change.

## 设计决策

| Decision | 选项 | 决定 | 理由 |
|----------|------|------|------|
| 1. 缓冲方式 | byte[] / Stream 抽象 | byte[] | z42 暂无 Stream trait；byte[] 简单且覆盖 protocol 场景 |
| 2. Reader 失败 | exception / -1 sentinel | exception | fail-fast；C# parity |
| 3. Writer grow | 2x doubling / 固定 | 2x doubling | 均摊 O(1)；初始 64 bytes |
| 4. Int16 返回 | int (sign-ext) / short | int | `short` 不在 HEAD-available 的 primitive 集 |
| 5. Float read/write | 提供 / 不提供 | 不提供 | 需 BitConverter intrinsic；defer |
| 6. UTF-8 string framing | length-prefixed / explicit byteCount | explicit | 不强制 framing 协议；caller 决定 |
| 7. 异常类 | 复用 Std.EndOfStreamException / 包内 BinaryException | 包内 | EndOfStream 不存在；新加到 z42.core 会扰动 incremental cache 计数 |
| 8. Namespace 层级 | `Std.IO.Binary` / `Std.Binary` | `Std.IO.Binary` | 与 package 名一致 `z42.io.binary`；VM lazy loader 的 prefix-emit 路径修好后（fix-deep-namespace-import-discovery, 2026-05-16）三段 namespace 工作 |
| 9. Byte param/return | `byte` 类型 / `int` | `int` | stdlib 无 `byte` 方法签名先例；统一 int + caller cast |
| 10. Int32 sign-extend | 不处理 / bit-31 显式 | 显式 | 不处理时负值变正大数 |

## 实现结构

```
src/BinaryReader.z42  (~140 行)
└── class BinaryReader  — cursor + read helpers

src/BinaryWriter.z42  (~155 行)
└── class BinaryWriter  — append-only buffer with 2x grow

src/BinaryException.z42  (~13 行)
└── class BinaryException : Exception  in Std namespace
```

## 字节序约定

- **LE (Little-Endian)** 后缀：低字节存低地址。x86/ARM 原生格式，protobuf、QUIC、
  大多数文件格式（PNG IDAT / ZIP / msgpack）。
- **BE (Big-Endian)** 后缀：高字节存低地址（"network byte order"）。TCP/UDP
  header、IP address、DNS、ssh wire format、Java class file。

没有 default — 显式后缀强制调用方阅读时知道字节序。

## Int32 sign-extension 注解

z42 `int` 物理上 i64-backed。直接 `b0 | (b1<<8) | (b2<<16) | (b3<<24)` 在
i64 算术下保留高位为 0，使 `-1` (0xFFFFFFFF bytes) 读回成 4294967295。

Solution：
```z42
long raw = (long)b0 | ((long)b1<<8) | ((long)b2<<16) | ((long)b3<<24);
if ((raw & 2147483648L) != 0L) { raw = raw - 4294967296L; }
return (int)raw;
```

Int64 read 不需要因为 `(long)int << 32` 路径里的 i64 overflow 自然给出正确
符号位（i64::MIN 来自 `1L << 63`）。

## 不支持（Deferred）

> ~~`io-binary-future-nested-ns`~~ ✅ 已解 by `fix-deep-namespace-import-discovery`
> (2026-05-16)：VM `extract_import_namespaces` 现 emit 每个 call target 的所有
> `.`-bounded prefix（不再只取前两段），3+ 段 stdlib namespace 现可被 lazy loader
> 命中。namespace 从 `Std.Binary` 还原为 `Std.IO.Binary`。

### ~~io-binary-future-float-double~~ — **✅ 已落地 2026-06-09 (`add-binary-float`)**

Shipped: `BinaryWriter.WriteSingle{LE,BE}(double)` / `WriteDouble{LE,BE}(double)`
+ `BinaryReader.ReadSingle{LE,BE}() → double` / `ReadDouble{LE,BE}() → double`。
取/返 `double`（宽浮点），Single 在线缆上是 4-byte IEEE-754 f32（写时 round，读时
widen），Double 是精确 8-byte f64 —— 对齐既有 `WriteInt16LE(int)` 的"宽入窄线"约定。
bit-level 转换由 4 个 corelib intrinsic 提供（`__single_to_bits` / `__single_from_bits`
/ `__double_to_bits` / `__double_from_bits`，`f32/f64::to_bits/from_bits`），不需要
独立的 `BitConverter` 类。8 tests：1.0f/1.0 已知字节模式（LE/BE）、f32-exact 与 f64
round-trip、0.1 narrowing 稳定性、与整数序列化器混合组合。

### ~~io-binary-future-stream~~ — ✅ 已落地 2026-05-24 (`refactor-binary-reader-stream`)

`BinaryReader(Stream src)` / `BinaryWriter(Stream dest)` constructors
sit `BinaryReader` / `BinaryWriter` directly on top of `Std.IO.Stream`.
File / Network / Memory sources all flow through the same `Stream`
abstraction without intermediate byte[]. Async streaming is still
gated on L3 async/await → `io-stream-future-async`.

### ~~io-binary-future-varint~~ — **✅ 已落地 2026-05-26 (add-io-binary-varint)**

Shipped: 4 methods covering both raw and ZigZag-encoded variable-length
i64 codec (protobuf naming convention `int64` and `sint64`).

| 方法 | 用途 |
|---|---|
| `WriteVarInt64(long v) → int` | 7-bit per byte; high bit = continuation; positive: ⌈bits/7⌉ bytes; negative: always 10 bytes |
| `ReadVarInt64() → long` | Inverse; throws on >10-byte input |
| `WriteVarSInt64(long v) → int` | ZigZag-encode then varint; small-magnitude negatives stay compact (`-1` → 1 byte, `-64` → 1 byte) |
| `ReadVarSInt64() → long` | Inverse of `WriteVarSInt64` |

21 tests cover: zero / one-byte boundary (127) / two-byte boundary
(128, 16383) / three-byte boundary / i64 max round-trip / arbitrary
sizes; encoded-length spec checks; protobuf canonical example (300 →
`[0xAC, 0x02]`); negative-1 uses 10 bytes (no zigzag); ZigZag
small-magnitude compaction (0, -1, -64 all 1 byte); ZigZag round-trip
positive/negative-small/large-negative/i64 min/i64 max; three varints
back-to-back in same stream; read varint from manually-constructed
bytes.

Implementation note: positive vs negative dispatch in the loop uses
explicit `if (u < 0)` to apply masked-shift-right (since z42 `>>` is
arithmetic). The mask constants `(1<<57) - 1` and `(1<<63) - 1`
simulate unsigned right-shift in the 7-bit and 1-bit variants
respectively.

### io-binary-future-byte-typed-api

- **来源**：理想 `WriteByte(byte b)` / `ReadByte() → byte` 类型签名
- **触发原因**：stdlib 当前无 `byte` 作 method param/return 先例；首次尝试
  导致 VM dispatch failure
- **前置依赖**：narrow-int primitives 完整落地（i8/u8 等 first-class）
- **当前 workaround**：用 `int` + caller `(byte)x` cast

### io-binary-future-parser-min-int

- **来源**：`-9223372036854775808L`（i64::MIN）字面量解析 overflow
- **触发原因**：parser 先 read `9223372036854775808` 为 i64 字面量
  （i64::MAX+1）→ Int64.Parse overflow；unary minus 是后置应用
- **当前 workaround**：用 i64::MIN+1（差 1，对边界测试不致命）或
  `-9223372036854775807L - 1L`

## 跨 stdlib 交互

- 依赖 `z42.core`（基础类型 + Exception）
- 依赖 `z42.encoding`（Utf8.GetBytes / GetString）
- 与 `z42.json` / `z42.toml` 互补：JSON/TOML 是文本格式，z42.io.binary 是
  二进制格式
- 未来 `z42.net` 接收 TCP/UDP buffer 后用 BinaryReader 解码 header

## 实施期发现

详 archived tasks.md "实施期发现" 段。关键四条：

1. 三段 namespace 在 VM lazy loader 里被 prefix heuristic 截断（`infer_namespace` 只取前两段）→ z42.io.binary.zpkg 不被加载 → `undefined function`。`fix-deep-namespace-import-discovery` (2026-05-16) 改为 emit 所有 `.`-bounded prefix。namespace 还原为 `Std.IO.Binary`
2. `byte` 不能作 method param/return（导致 `int` API）
3. Int32 read 必须显式 sign-extend（i64 backing 不自动 sign-extend）
4. `-i64::MIN` literal parser overflow
