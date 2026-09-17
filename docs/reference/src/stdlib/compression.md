# z42.compression —— 压缩算法与归档格式

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.compression/`；命名空间 `Std.Compression`（算法）、
> `Std.Archive`（归档）、`Std`（异常类型）

六种压缩算法（gzip / zlib / raw deflate / Zstandard / Brotli / LZ4）加两种归档格式
（tar / zip）。压缩算法都提供 `byte[] → byte[]` 的一次性 API；其中 gzip / zlib /
deflate / zstd 另外提供把任意 [`Std.IO.Stream`](io-stream.md) 包起来的流式 API。
归档侧 tar / zip 只有一次性 `byte[]` API，另加针对目录的解包与（tar 的）流式读写。

「压缩算法」和「归档格式」是两件事：tar 只打包不压缩，要 `.tar.gz` 就把
`Tar.WriteStream` 的目标换成 `Gzip.WrapWrite(...)`；zip 则自带每条目的 deflate。

## 算法一览

| 类（`Std.Compression`） | 格式 | 一次性 | 流式 `WrapWrite` / `WrapRead` | level 取值 | 默认 |
|---|---|---|---|---|---|
| `Gzip` | RFC 1952 gzip | ✅ | ✅ | 1..9 | 6 |
| `Zlib` | RFC 1950 zlib | ✅ | ✅ | 1..9 | 6 |
| `Deflate` | RFC 1951 裸 deflate | ✅ | ✅ | 1..9 | 6 |
| `Zstd` | Zstandard | ✅ | ✅ | 1..22 | 3 |
| `Brotli` | RFC 7932 | ✅ | ❌ | 0..11 | 4 |
| `Lz4` | LZ4 frame（`.lz4`） | ✅ | ❌ | 无（形参被忽略） | — |

`Gzip` / `Zlib` / `Deflate` / `Zstd` 四个类的成员完全同形：

```z42
public static class Gzip {
    public static byte[] Compress(byte[] data);
    public static byte[] Compress(byte[] data, int level);
    public static byte[] Decompress(byte[] data);

    public static Stream WrapWrite(Stream dest);
    public static Stream WrapWrite(Stream dest, int level);
    public static Stream WrapRead(Stream src);
}
```

`Brotli` / `Lz4` 只有一次性三件套：

```z42
public static class Brotli {
    public static byte[] Compress(byte[] data);
    public static byte[] Compress(byte[] data, int level);   // 0..11
    public static byte[] Decompress(byte[] data);
}

public static class Lz4 {
    public static byte[] Compress(byte[] data);
    public static byte[] Compress(byte[] data, int level);   // level 被忽略
    public static byte[] Decompress(byte[] data);
}
```

`Lz4` 产出的是标准 `.lz4` frame（magic `0x184D2204`），与 `lz4` 命令行工具互通。

### 等级常量

```z42
public static class Compression {
    public static int Fastest       = 1;   // deflate / zlib / gzip
    public static int Default       = 6;
    public static int Best          = 9;

    public static int ZstdFastest   = 1;
    public static int ZstdDefault   = 3;
    public static int ZstdBest      = 22;

    public static int BrotliFastest = 0;
    public static int BrotliDefault = 4;
    public static int BrotliBest    = 11;
}
```

Brotli 的 `BrotliBest`（11）在小输入上也可能耗时以秒计，HTTP 之类响应时间敏感的场景
用默认的 4。

### 算法 ID

```z42
public static class AlgoId {
    public static int DeflateRaw = 0;
    public static int Zlib       = 1;
    public static int Gzip       = 2;
    public static int Zstd       = 10;
    public static int Brotli     = 11;
    public static int Lz4        = 12;
}
```

这些 ID 只在直接构造下面两个流类时才需要；走 `Gzip.WrapWrite(...)` 这样的门面时用不到。

## 流式压缩 / 解压

`WrapWrite` / `WrapRead` 返回的就是这两个 `Std.IO.Stream` 子类，也可以直接构造：

```z42
public sealed class CompressionEncoderStream : Stream {
    public CompressionEncoderStream(Stream dest, int algo, int level);
}

public sealed class CompressionDecoderStream : Stream {
    public CompressionDecoderStream(Stream src, int algo);
}
```

| | `CompressionEncoderStream`（`WrapWrite`） | `CompressionDecoderStream`（`WrapRead`） |
|---|---|---|
| `CanWrite` / `CanRead` | `true`（`Close()` 后 false）/ `false` | `false` / `true` |
| `CanSeek` | `false` | `false` |
| 数据流向 | 写明文进来 → 压缩后即时转发给 `dest` | 从 `src` 拉压缩字节 → 读出明文 |
| 内存占用 | 每次 `Write` 即时下推 | 一次一块（64 KB 压缩输入）+ 该块产出的明文 |

- **`WrapWrite` 必须 `Close()`**：收尾字节（gzip CRC32 尾、zstd 校验等）在 `Close()`
  时才写出，不关就是一个被截断、解不开的输出。`Close()` 幂等。
- **`Flush()` 对编码流是空操作**：待压缩状态只能由 `Close()` 释放。
- **两个包装流都不关闭被包装的流**：`dest` / `src` 的生命周期归调用方。
- 构造时能力不符抛 `ArgumentException`：`WrapWrite` 的 `dest` 不可写、`WrapRead` 的
  `src` 不可读。
- 解码流 `Close()` 之后再 `Read` 抛 `InvalidOperationException`。

## Std.Archive —— tar

```z42
public class TarEntry {
    public string Name;
    public byte[] Content;
    public int    Mode;                                        // POSIX 权限位
    public TarEntry(string name, byte[] content, int mode);
}

public static class Tar {
    public static TarEntry[] Read(byte[] tarBytes);
    public static byte[]     Write(TarEntry[] entries);
    public static void       WriteStream(TarEntry[] entries, Stream dest);
    public static int        ExtractTo(byte[] tarBytes, string destDir);
    public static int        ExtractStream(Stream src, string destDir);
}
```

格式是 ustar（POSIX 1003.1-1988）：512 字节块、八进制字段、两个全零块表示结束。

| 成员 | 说明 |
|---|---|
| `Read` | 空数组返回 0 条；长度不是 512 倍数抛 `ArchiveException`；遇到**非普通文件**的 typeflag（目录、符号链接、设备……）抛 `ArchiveException` |
| `Write` | 返回完整归档字节。名字写进 100 字节的 name 字段，**不使用 ustar 的 prefix 扩展** |
| `WriteStream` | 同 `Write` 但直接写进 `dest`，不在内存里攒完整归档。**不关闭 `dest`** |
| `ExtractTo` | 解包到 `destDir`（自动建目录），返回写出的文件数；`Mode` 带任一执行位时给目标文件加可执行位 |
| `ExtractStream` | 边读边解，适合大归档；除普通文件外**还接受目录条目**（typeflag `5`）；缺结尾全零块时按正常结束处理 |

两个 `Extract*` 都做 Zip-Slip 防御：条目名为空、以 `/` 开头、或含 `..` 路径段时抛
`ArchiveException`，不写任何文件。

## Std.Archive —— zip

```z42
public class ZipEntry {
    public string Name;
    public byte[] Content;             // 始终是解压后的明文
    public int    CompressionMethod;   // 0 = STORE，8 = DEFLATE
    public ZipEntry(string name, byte[] content, int compressionMethod);
}

public static class Zip {
    public static ZipEntry[] Read(byte[] zipBytes);
    public static byte[]     Write(ZipEntry[] entries);
    public static byte[]     ExtractFile(byte[] zipBytes, string name);
    public static int        ExtractAllTo(byte[] zipBytes, string destDir);
}
```

| 成员 | 说明 |
|---|---|
| `Read` | 逐条返回**已解压**的 `Content`；输入短于 22 字节（EOCD 最小长度）抛 `ArchiveException` |
| `Write` | 逐条按 `CompressionMethod` 打包；method 不是 0 / 8 抛 `ArchiveException`。空条目表产出 22 字节（只有 EOCD）的合法空归档 |
| `ExtractFile` | 按名字精确匹配取出内容；找不到抛 `ArchiveException` |
| `ExtractAllTo` | 解包到 `destDir`，返回写出的**文件**数；名字以 `/` 结尾的条目当目录处理（建目录、不计数）。同样有 Zip-Slip 防御 |

`Write` 写出的条目时间戳固定为 1980-01-01（zip 纪元），不保留原始 mtime。

## 异常

| 类型（`Std`） | 说明 |
|---|---|
| `ArchiveException` | tar / zip 的格式错误、不支持的条目类型、不支持的压缩方法、条目找不到、Zip-Slip 拦截 |
| `ArgumentException` | 包装流构造时目标 / 源的能力不符 |
| `InvalidOperationException` | 对已 `Close()` 的包装流继续读写 |

```z42
public class CompressionException : Exception { public CompressionException(string message); }
public class ArchiveException     : Exception { public ArchiveException(string message);     }
```

`CompressionException` 这个类型存在，但标准库自身不抛它——见下面的「错误处理」。

### 错误处理

压缩 / 解压的**失败路径目前不可捕获**：给出损坏的输入（拿非 gzip 字节调
`Gzip.Decompress`）或越界的 level（`Gzip.Compress(data, 0)`），调用不会抛异常，
而是不返回。所以调用方要自己保证：

- level 落在上表给出的区间内；
- 解压前已经确认过数据来源（例如核对过 magic 字节）。

## 用法

```z42
using Std;
using Std.IO;
using Std.Compression;
using Std.Archive;
using Std.Encoding;

void Main() {
    byte[] plain = Utf8.GetBytes("payload payload payload payload");

    // 一次性
    byte[] gz = Gzip.Compress(plain, Compression.Best);
    Console.WriteLine($"{plain.Length} -> {gz.Length} -> {Gzip.Decompress(gz).Length}");

    // 流式压缩：写进内存流
    MemoryStream dest = new MemoryStream();
    Stream enc = Gzip.WrapWrite(dest);
    enc.Write(plain, 0, plain.Length);
    enc.Close();                                   // 不 Close 就是截断输出
    Console.WriteLine($"streamed={dest.Length()}");

    // 流式解压
    Stream dec = Gzip.WrapRead(new MemoryStream(dest.ToArray()));
    Console.WriteLine($"back={dec.ReadAllBytes().Length}");
    dec.Close();

    // .tar.gz：tar 写进 gzip 编码流
    TarEntry[] entries = new TarEntry[2];
    entries[0] = new TarEntry("a.txt", Utf8.GetBytes("AAA"), 0x1A4);   // 0o644
    entries[1] = new TarEntry("sub/b.txt", Utf8.GetBytes("BBB"), 0x1ED); // 0o755
    MemoryStream tgz = new MemoryStream();
    Stream gzout = Gzip.WrapWrite(tgz);
    try { Tar.WriteStream(entries, gzout); } finally { gzout.Close(); }

    // 反向：gzip 解码流直接喂给 tar 流式解包
    int n = Tar.ExtractStream(Gzip.WrapRead(new MemoryStream(tgz.ToArray())), "out");
    Console.WriteLine($"extracted={n}");

    // zip：混合 STORE / DEFLATE
    ZipEntry[] zs = new ZipEntry[2];
    zs[0] = new ZipEntry("raw.bin", plain, 0);
    zs[1] = new ZipEntry("packed.txt", plain, 8);
    byte[] zip = Zip.Write(zs);
    Console.WriteLine($"zip={zip.Length} entries={Zip.Read(zip).Length}");
    Console.WriteLine($"one=[{Utf8.GetString(Zip.ExtractFile(zip, "raw.bin"))}]");
}
```

## 不支持

- **Brotli / LZ4 没有流式 API**：只有一次性 `Compress` / `Decompress`。
- **LZ4 没有等级**：`Compress(data, level)` 的 `level` 被忽略（无 LZ4-HC）。
- **编码流没有中途 flush**：`Flush()` 是空操作，只有 `Close()` 能把待压数据推出去。
- **包装流不可定位**：`CanSeek()` 恒 `false`。
- **wasm32 上没有 Zstd**：其余五种算法可用。
- **没有 xz / LZMA**，没有 Zstd 预置字典。
- **tar 只支持普通文件**：`Tar.Read` 碰到目录 / 符号链接 / 硬链接 / 设备条目即抛
  `ArchiveException`（`Tar.ExtractStream` 额外接受目录条目）；pax 扩展头、稀疏文件不支持。
- **tar 条目名限 100 字节以内的 ASCII**：`Tar.Write` 对更长的名字**静默截断**，
  非 ASCII 字符也会被静默破坏（写入路径不使用 ustar 的 155 字节 prefix 字段）。
- **zip 条目名同样限 ASCII**：`Zip.Write` 按 UTF-8 写出，但 `Zip.Read` 按单字节解码，
  非 ASCII 名字读回来是乱码。
- **zip 只支持 method 0 / 8**：没有 ZIP64（归档需 < 4 GB、条目数 < 65535）、
  没有加密、没有归档注释、不保留 mtime。
- **zip 没有 `CreateFromDirectory`**：自己遍历目录构 `ZipEntry[]` 再 `Zip.Write`。
- **zip / tar 的一次性 API 全量驻留内存**：`Zip.Read` 会把每条目的明文都解出来；
  只有 tar 有 `WriteStream` / `ExtractStream` 这对流式出入口。
