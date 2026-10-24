<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.compression/ -->

# z42.compression —— 压缩算法与归档格式。

> 包路径 `src/libraries/z42.compression/`；命名空间 `Std.Compression`

六种压缩算法（gzip / zlib / raw deflate / Zstandard / Brotli / LZ4）加两种归档格式
（tar / zip）。压缩算法都提供 `byte[] → byte[]` 的一次性 API；其中 gzip / zlib /
deflate / zstd 另外提供把任意 `Std.IO.Stream` 包起来的流式 API。
归档侧 tar / zip 只有一次性 `byte[]` API，另加针对目录的解包与（tar 的）流式读写。

「压缩算法」和「归档格式」是两件事：tar 只打包不压缩，要 `.tar.gz` 就把
`Tar.WriteStream` 的目标换成 `Gzip.WrapWrite(...)`；zip 则自带每条目的 deflate。

## 不支持

- **Brotli / LZ4 没有流式 API**：只有一次性 `Compress` / `Decompress`。
- **LZ4 没有等级**：`Compress(data, level)` 的 `level` 被忽略（无 LZ4-HC）。
- **编码流没有中途 flush**：`Flush()` 是空操作，只有 `Close()` 能把待压数据推出去。
- **包装流不可定位**：`CanSeek()` 恒 `false`。
- **wasm32 上没有 Zstd**：其余五种算法可用。
- **没有 xz / LZMA**，没有 Zstd 预置字典。
- **tar 只支持普通文件**：`Tar.Read` 碰到目录 / 符号链接 / 硬链接 / 设备条目即抛
  `ArchiveException`（`Tar.ExtractStream` 额外接受目录条目）；稀疏文件不支持。pax / GNU
  长名头只在**读取**时认，`Tar.Write` 不写它们。
- **tar 条目名限 100 字节以内的 ASCII**：`Tar.Write` 对更长的名字**静默截断**，
  非 ASCII 字符也会被静默破坏（写入路径不使用 ustar 的 155 字节 prefix 字段）。
- **zip 条目名同样限 ASCII**：`Zip.Write` 按 UTF-8 写出，但 `Zip.Read` 按单字节解码，
  非 ASCII 名字读回来是乱码。
- **zip 只支持 method 0 / 8**：没有 ZIP64（归档需 < 4 GB、条目数 < 65535）、
  没有加密、没有归档注释、不保留 mtime。
- **zip 没有 `CreateFromDirectory`**：自己遍历目录构 `ZipEntry[]` 再 `Zip.Write`。
- **zip / tar 的一次性 API 全量驻留内存**：`Zip.Read` 会把每条目的明文都解出来；
  只有 tar 有 `WriteStream` / `ExtractStream` 这对流式出入口。

## Brotli

[`src/libraries/z42.compression/src/Brotli.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Brotli.z42#L4)

RFC 7932 Brotli 压缩/解压（HTTP `Content-Encoding: br`）。纯 Rust 实现，仅一次性 API。

```z42
public static class Brotli {
    static byte[] Compress(byte[] data)
    static byte[] Compress(byte[] data, int level)
    static byte[] Decompress(byte[] data)
}
```

成员：[`Compress`](#Std.Compression.Brotli.Compress(byte[])) · [`Compress`](#Std.Compression.Brotli.Compress(byte[],int)) · [`Decompress`](#Std.Compression.Brotli.Decompress(byte[]))

<a id="Std.Compression.Brotli.Compress(byte[])"></a>
### static byte[] Compress(byte[] data)

[`src/libraries/z42.compression/src/Brotli.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Brotli.z42#L12)

用默认等级（4）把 `data` 压缩成 Brotli 字节。

<a id="Std.Compression.Brotli.Compress(byte[],int)"></a>
### static byte[] Compress(byte[] data, int level)

[`src/libraries/z42.compression/src/Brotli.z42:17`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Brotli.z42#L17)

用给定等级（0 最快 .. 11 最优）把 `data` 压缩成 Brotli 字节。

<a id="Std.Compression.Brotli.Decompress(byte[])"></a>
### static byte[] Decompress(byte[] data)

[`src/libraries/z42.compression/src/Brotli.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Brotli.z42#L22)

解压 Brotli 字节为明文。

## Compression

[`src/libraries/z42.compression/src/Compression.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Compression.z42#L34)

各压缩算法的等级（level）常量。

```z42
public static class Compression {
    static int Fastest
    static int Default
    static int Best
    static int ZstdFastest
    static int ZstdDefault
    static int ZstdBest
    static int BrotliFastest
    static int BrotliDefault
    static int BrotliBest
}
```

成员：[`Fastest`](#Std.Compression.Compression.Fastest) · [`Default`](#Std.Compression.Compression.Default) · [`Best`](#Std.Compression.Compression.Best) · [`ZstdFastest`](#Std.Compression.Compression.ZstdFastest) · [`ZstdDefault`](#Std.Compression.Compression.ZstdDefault) · [`ZstdBest`](#Std.Compression.Compression.ZstdBest) · [`BrotliFastest`](#Std.Compression.Compression.BrotliFastest) · [`BrotliDefault`](#Std.Compression.Compression.BrotliDefault) · [`BrotliBest`](#Std.Compression.Compression.BrotliBest)

<a id="Std.Compression.Compression.Fastest"></a>
### static int Fastest

[`src/libraries/z42.compression/src/Compression.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Compression.z42#L36)

deflate / zlib / gzip 的最快等级（1）。

<a id="Std.Compression.Compression.Default"></a>
### static int Default

[`src/libraries/z42.compression/src/Compression.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Compression.z42#L38)

deflate / zlib / gzip 的默认等级（6）。

<a id="Std.Compression.Compression.Best"></a>
### static int Best

[`src/libraries/z42.compression/src/Compression.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Compression.z42#L40)

deflate / zlib / gzip 的最优压缩等级（9）。

<a id="Std.Compression.Compression.ZstdFastest"></a>
### static int ZstdFastest

[`src/libraries/z42.compression/src/Compression.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Compression.z42#L43)

Zstd 的最快等级（1）。

<a id="Std.Compression.Compression.ZstdDefault"></a>
### static int ZstdDefault

[`src/libraries/z42.compression/src/Compression.z42:45`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Compression.z42#L45)

Zstd 的默认等级（3），与 libzstd 默认一致。

<a id="Std.Compression.Compression.ZstdBest"></a>
### static int ZstdBest

[`src/libraries/z42.compression/src/Compression.z42:47`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Compression.z42#L47)

Zstd 的最优压缩等级（22）。

<a id="Std.Compression.Compression.BrotliFastest"></a>
### static int BrotliFastest

[`src/libraries/z42.compression/src/Compression.z42:50`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Compression.z42#L50)

Brotli 的最快等级（0）。

<a id="Std.Compression.Compression.BrotliDefault"></a>
### static int BrotliDefault

[`src/libraries/z42.compression/src/Compression.z42:52`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Compression.z42#L52)

Brotli 的默认等级（4）。

<a id="Std.Compression.Compression.BrotliBest"></a>
### static int BrotliBest

[`src/libraries/z42.compression/src/Compression.z42:54`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Compression.z42#L54)

Brotli 的最优压缩等级（11）；小输入上也可能耗时以秒计。

## AlgoId

[`src/libraries/z42.compression/src/Compression.z42:59`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Compression.z42#L59)

算法 ID 常量；只在直接构造流类时需要，走 `Gzip.WrapWrite(...)` 等门面时用不到。

```z42
public static class AlgoId {
    static int DeflateRaw
    static int Zlib
    static int Gzip
    static int Zstd
    static int Brotli
    static int Lz4
}
```

成员：[`DeflateRaw`](#Std.Compression.AlgoId.DeflateRaw) · [`Zlib`](#Std.Compression.AlgoId.Zlib) · [`Gzip`](#Std.Compression.AlgoId.Gzip) · [`Zstd`](#Std.Compression.AlgoId.Zstd) · [`Brotli`](#Std.Compression.AlgoId.Brotli) · [`Lz4`](#Std.Compression.AlgoId.Lz4)

<a id="Std.Compression.AlgoId.DeflateRaw"></a>
### static int DeflateRaw

[`src/libraries/z42.compression/src/Compression.z42:61`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Compression.z42#L61)

裸 deflate（RFC 1951）。

<a id="Std.Compression.AlgoId.Zlib"></a>
### static int Zlib

[`src/libraries/z42.compression/src/Compression.z42:63`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Compression.z42#L63)

zlib（RFC 1950）。

<a id="Std.Compression.AlgoId.Gzip"></a>
### static int Gzip

[`src/libraries/z42.compression/src/Compression.z42:65`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Compression.z42#L65)

gzip（RFC 1952）。

<a id="Std.Compression.AlgoId.Zstd"></a>
### static int Zstd

[`src/libraries/z42.compression/src/Compression.z42:67`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Compression.z42#L67)

Zstandard。

<a id="Std.Compression.AlgoId.Brotli"></a>
### static int Brotli

[`src/libraries/z42.compression/src/Compression.z42:69`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Compression.z42#L69)

Brotli（RFC 7932）。

<a id="Std.Compression.AlgoId.Lz4"></a>
### static int Lz4

[`src/libraries/z42.compression/src/Compression.z42:71`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Compression.z42#L71)

LZ4 frame。

## CompressionDecoderStream

[`src/libraries/z42.compression/src/CompressionDecoderStream.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/CompressionDecoderStream.z42#L10)

拉模式压缩解码流：调用方从返回流读明文，按需从 `src` 拉取压缩字节。

对应 `Gzip.WrapRead(...)` 等门面的返回类型，也可直接构造。
内存占用为一块压缩输入（64 KB）+ 该块产出的明文。不关闭 `src`——其生命周期归调用方。
关闭后再 `Read` 抛 `InvalidOperationException`。

```z42
public sealed class CompressionDecoderStream : Stream {
    CompressionDecoderStream(Stream src, int algo)
}
```

成员：[`CompressionDecoderStream`](#Std.Compression.CompressionDecoderStream.ctor(Stream,int))

<a id="Std.Compression.CompressionDecoderStream.ctor(Stream,int)"></a>
### CompressionDecoderStream(Stream src, int algo)

[`src/libraries/z42.compression/src/CompressionDecoderStream.z42:21`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/CompressionDecoderStream.z42#L21)

用给定算法 `algo` 把 `src` 包成解码流；`src` 不可读时抛 `ArgumentException`。

## CompressionEncoderStream

[`src/libraries/z42.compression/src/CompressionEncoderStream.z42:11`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/CompressionEncoderStream.z42#L11)

写通式压缩编码流：调用方写入明文，流即时压缩并转发给被包装的 `dest`。对应 `Gzip.WrapWrite(...)` 等门面的返回类型，也可直接构造。

收尾字节（gzip CRC32 尾、zstd 校验等）在 `Close()` 时才写出，不关闭就是一个被截断、
解不开的输出；`Close()` 幂等。`Flush()` 是空操作。不关闭被包装的 `dest`——其生命周期
归调用方。

```z42
public sealed class CompressionEncoderStream : Stream {
    CompressionEncoderStream(Stream dest, int algo, int level)
}
```

成员：[`CompressionEncoderStream`](#Std.Compression.CompressionEncoderStream.ctor(Stream,int,int))

<a id="Std.Compression.CompressionEncoderStream.ctor(Stream,int,int)"></a>
### CompressionEncoderStream(Stream dest, int algo, int level)

[`src/libraries/z42.compression/src/CompressionEncoderStream.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/CompressionEncoderStream.z42#L18)

用给定算法 `algo` 与等级 `level` 把 `dest` 包成编码流；`dest` 不可写时抛 `ArgumentException`。

## CompressionException

[`src/libraries/z42.compression/src/CompressionException.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/CompressionException.z42#L5)

压缩异常类型，继承 `Exception`。该类型存在，但标准库自身不抛它——压缩/解压的失败路径 当前不可捕获。

```z42
public class CompressionException : Exception {
    CompressionException(string message)
}
```

成员：[`CompressionException`](#Std.Compression.CompressionException.ctor(string))

<a id="Std.Compression.CompressionException.ctor(string)"></a>
### CompressionException(string message)

[`src/libraries/z42.compression/src/CompressionException.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/CompressionException.z42#L7)

用 `message` 构造压缩异常。

## Deflate

[`src/libraries/z42.compression/src/Deflate.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Deflate.z42#L6)

裸 DEFLATE（RFC 1951）：无帧头、无校验和；一次性 `byte[]` 压缩/解压及流式封装。

```z42
public static class Deflate {
    static byte[] Compress(byte[] data)
    static byte[] Compress(byte[] data, int level)
    static byte[] Decompress(byte[] data)
    static Stream WrapWrite(Stream dest)
    static Stream WrapWrite(Stream dest, int level)
    static Stream WrapRead(Stream src)
}
```

成员：[`Compress`](#Std.Compression.Deflate.Compress(byte[])) · [`Compress`](#Std.Compression.Deflate.Compress(byte[],int)) · [`Decompress`](#Std.Compression.Deflate.Decompress(byte[])) · [`WrapWrite`](#Std.Compression.Deflate.WrapWrite(Stream)) · [`WrapWrite`](#Std.Compression.Deflate.WrapWrite(Stream,int)) · [`WrapRead`](#Std.Compression.Deflate.WrapRead(Stream))

<a id="Std.Compression.Deflate.Compress(byte[])"></a>
### static byte[] Compress(byte[] data)

[`src/libraries/z42.compression/src/Deflate.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Deflate.z42#L14)

用默认等级（6）把 `data` 压缩成裸 deflate 字节。

<a id="Std.Compression.Deflate.Compress(byte[],int)"></a>
### static byte[] Compress(byte[] data, int level)

[`src/libraries/z42.compression/src/Deflate.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Deflate.z42#L19)

用给定等级（1..9）把 `data` 压缩成裸 deflate 字节。

<a id="Std.Compression.Deflate.Decompress(byte[])"></a>
### static byte[] Decompress(byte[] data)

[`src/libraries/z42.compression/src/Deflate.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Deflate.z42#L24)

解压裸 deflate 字节为明文。

<a id="Std.Compression.Deflate.WrapWrite(Stream)"></a>
### static Stream WrapWrite(Stream dest)

[`src/libraries/z42.compression/src/Deflate.z42:29`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Deflate.z42#L29)

把 `dest` 包装成裸 deflate 编码流（默认等级）。

<a id="Std.Compression.Deflate.WrapWrite(Stream,int)"></a>
### static Stream WrapWrite(Stream dest, int level)

[`src/libraries/z42.compression/src/Deflate.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Deflate.z42#L34)

把 `dest` 包装成裸 deflate 编码流（给定等级 1..9）。

<a id="Std.Compression.Deflate.WrapRead(Stream)"></a>
### static Stream WrapRead(Stream src)

[`src/libraries/z42.compression/src/Deflate.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Deflate.z42#L39)

把 `src` 包装成裸 deflate 解码流。

## Gzip

[`src/libraries/z42.compression/src/Gzip.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Gzip.z42#L6)

RFC 1952 gzip 容器：一次性 `byte[]` 压缩/解压，以及 `Std.IO.Stream` 流式封装。

```z42
public static class Gzip {
    static byte[] Compress(byte[] data)
    static byte[] Compress(byte[] data, int level)
    static byte[] Decompress(byte[] data)
    static Stream WrapWrite(Stream dest)
    static Stream WrapWrite(Stream dest, int level)
    static Stream WrapRead(Stream src)
}
```

成员：[`Compress`](#Std.Compression.Gzip.Compress(byte[])) · [`Compress`](#Std.Compression.Gzip.Compress(byte[],int)) · [`Decompress`](#Std.Compression.Gzip.Decompress(byte[])) · [`WrapWrite`](#Std.Compression.Gzip.WrapWrite(Stream)) · [`WrapWrite`](#Std.Compression.Gzip.WrapWrite(Stream,int)) · [`WrapRead`](#Std.Compression.Gzip.WrapRead(Stream))

<a id="Std.Compression.Gzip.Compress(byte[])"></a>
### static byte[] Compress(byte[] data)

[`src/libraries/z42.compression/src/Gzip.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Gzip.z42#L18)

用默认等级（6）把 `data` 压缩成 gzip 字节。

<a id="Std.Compression.Gzip.Compress(byte[],int)"></a>
### static byte[] Compress(byte[] data, int level)

[`src/libraries/z42.compression/src/Gzip.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Gzip.z42#L23)

用给定等级（1..9）把 `data` 压缩成 gzip 字节。

<a id="Std.Compression.Gzip.Decompress(byte[])"></a>
### static byte[] Decompress(byte[] data)

[`src/libraries/z42.compression/src/Gzip.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Gzip.z42#L28)

解压 gzip 字节为明文。

<a id="Std.Compression.Gzip.WrapWrite(Stream)"></a>
### static Stream WrapWrite(Stream dest)

[`src/libraries/z42.compression/src/Gzip.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Gzip.z42#L38)

把 `dest` 包装成 gzip 编码流（默认等级）。

写入返回流的字节会被压缩并转发给 `dest`。调用方必须对返回流调用 `Close()`
以写出 gzip 尾；否则输出被截断。`dest` 不会被编码流关闭——其生命周期归调用方。

<a id="Std.Compression.Gzip.WrapWrite(Stream,int)"></a>
### static Stream WrapWrite(Stream dest, int level)

[`src/libraries/z42.compression/src/Gzip.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Gzip.z42#L43)

把 `dest` 包装成 gzip 编码流，使用给定等级（1..9）。

<a id="Std.Compression.Gzip.WrapRead(Stream)"></a>
### static Stream WrapRead(Stream src)

[`src/libraries/z42.compression/src/Gzip.z42:51`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Gzip.z42#L51)

把 `src` 包装成 gzip 解码流，读出的是解压后的明文。

明文按需从 `src` 拉取的 gzip 字节解压而来。`src` 不会被解码流关闭——其
生命周期归调用方。

## Lz4

[`src/libraries/z42.compression/src/Lz4.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Lz4.z42#L4)

LZ4 frame（`.lz4`）压缩/解压：速度优先，仅一次性 API。

```z42
public static class Lz4 {
    static byte[] Compress(byte[] data)
    static byte[] Compress(byte[] data, int level)
    static byte[] Decompress(byte[] data)
}
```

成员：[`Compress`](#Std.Compression.Lz4.Compress(byte[])) · [`Compress`](#Std.Compression.Lz4.Compress(byte[],int)) · [`Decompress`](#Std.Compression.Lz4.Decompress(byte[]))

<a id="Std.Compression.Lz4.Compress(byte[])"></a>
### static byte[] Compress(byte[] data)

[`src/libraries/z42.compression/src/Lz4.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Lz4.z42#L12)

把 `data` 压缩成 `.lz4` frame 字节。

<a id="Std.Compression.Lz4.Compress(byte[],int)"></a>
### static byte[] Compress(byte[] data, int level)

[`src/libraries/z42.compression/src/Lz4.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Lz4.z42#L18)

`level` 为与其他算法保持形参对称而接受，但被忽略——LZ4 基础格式没有等级旋钮； 需要压缩比调节时用 Brotli / Zstd。

<a id="Std.Compression.Lz4.Decompress(byte[])"></a>
### static byte[] Decompress(byte[] data)

[`src/libraries/z42.compression/src/Lz4.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Lz4.z42#L23)

解压 `.lz4` frame 字节为明文。

## Zlib

[`src/libraries/z42.compression/src/Zlib.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Zlib.z42#L6)

RFC 1950 zlib 容器：一次性 `byte[]` 压缩/解压，以及 `Std.IO.Stream` 流式封装。

```z42
public static class Zlib {
    static byte[] Compress(byte[] data)
    static byte[] Compress(byte[] data, int level)
    static byte[] Decompress(byte[] data)
    static Stream WrapWrite(Stream dest)
    static Stream WrapWrite(Stream dest, int level)
    static Stream WrapRead(Stream src)
}
```

成员：[`Compress`](#Std.Compression.Zlib.Compress(byte[])) · [`Compress`](#Std.Compression.Zlib.Compress(byte[],int)) · [`Decompress`](#Std.Compression.Zlib.Decompress(byte[])) · [`WrapWrite`](#Std.Compression.Zlib.WrapWrite(Stream)) · [`WrapWrite`](#Std.Compression.Zlib.WrapWrite(Stream,int)) · [`WrapRead`](#Std.Compression.Zlib.WrapRead(Stream))

<a id="Std.Compression.Zlib.Compress(byte[])"></a>
### static byte[] Compress(byte[] data)

[`src/libraries/z42.compression/src/Zlib.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Zlib.z42#L14)

用默认等级（6）把 `data` 压缩成 zlib 字节。

<a id="Std.Compression.Zlib.Compress(byte[],int)"></a>
### static byte[] Compress(byte[] data, int level)

[`src/libraries/z42.compression/src/Zlib.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Zlib.z42#L19)

用给定等级（1..9）把 `data` 压缩成 zlib 字节。

<a id="Std.Compression.Zlib.Decompress(byte[])"></a>
### static byte[] Decompress(byte[] data)

[`src/libraries/z42.compression/src/Zlib.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Zlib.z42#L24)

解压 zlib 字节为明文。

<a id="Std.Compression.Zlib.WrapWrite(Stream)"></a>
### static Stream WrapWrite(Stream dest)

[`src/libraries/z42.compression/src/Zlib.z42:29`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Zlib.z42#L29)

把 `dest` 包装成 zlib 编码流（默认等级）。

<a id="Std.Compression.Zlib.WrapWrite(Stream,int)"></a>
### static Stream WrapWrite(Stream dest, int level)

[`src/libraries/z42.compression/src/Zlib.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Zlib.z42#L34)

把 `dest` 包装成 zlib 编码流（给定等级 1..9）。

<a id="Std.Compression.Zlib.WrapRead(Stream)"></a>
### static Stream WrapRead(Stream src)

[`src/libraries/z42.compression/src/Zlib.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Zlib.z42#L39)

把 `src` 包装成 zlib 解码流。

## Zstd

[`src/libraries/z42.compression/src/Zstd.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Zstd.z42#L6)

Zstandard（RFC 8878）压缩/解压：一次性 `byte[]` API 及 `Std.IO.Stream` 流式封装。

```z42
public static class Zstd {
    static byte[] Compress(byte[] data)
    static byte[] Compress(byte[] data, int level)
    static byte[] Decompress(byte[] data)
    static Stream WrapWrite(Stream dest)
    static Stream WrapWrite(Stream dest, int level)
    static Stream WrapRead(Stream src)
}
```

成员：[`Compress`](#Std.Compression.Zstd.Compress(byte[])) · [`Compress`](#Std.Compression.Zstd.Compress(byte[],int)) · [`Decompress`](#Std.Compression.Zstd.Decompress(byte[])) · [`WrapWrite`](#Std.Compression.Zstd.WrapWrite(Stream)) · [`WrapWrite`](#Std.Compression.Zstd.WrapWrite(Stream,int)) · [`WrapRead`](#Std.Compression.Zstd.WrapRead(Stream))

<a id="Std.Compression.Zstd.Compress(byte[])"></a>
### static byte[] Compress(byte[] data)

[`src/libraries/z42.compression/src/Zstd.z42:14`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Zstd.z42#L14)

用默认等级（3）把 `data` 压缩成 Zstandard 字节。

<a id="Std.Compression.Zstd.Compress(byte[],int)"></a>
### static byte[] Compress(byte[] data, int level)

[`src/libraries/z42.compression/src/Zstd.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Zstd.z42#L19)

用给定等级（1..22）把 `data` 压缩成 Zstandard 字节。

<a id="Std.Compression.Zstd.Decompress(byte[])"></a>
### static byte[] Decompress(byte[] data)

[`src/libraries/z42.compression/src/Zstd.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Zstd.z42#L24)

解压 Zstandard 字节为明文。

<a id="Std.Compression.Zstd.WrapWrite(Stream)"></a>
### static Stream WrapWrite(Stream dest)

[`src/libraries/z42.compression/src/Zstd.z42:29`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Zstd.z42#L29)

把 `dest` 包装成 Zstandard 编码流（默认等级）。

<a id="Std.Compression.Zstd.WrapWrite(Stream,int)"></a>
### static Stream WrapWrite(Stream dest, int level)

[`src/libraries/z42.compression/src/Zstd.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Zstd.z42#L34)

把 `dest` 包装成 Zstandard 编码流（给定等级 1..22）。

<a id="Std.Compression.Zstd.WrapRead(Stream)"></a>
### static Stream WrapRead(Stream src)

[`src/libraries/z42.compression/src/Zstd.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.compression/src/Zstd.z42#L39)

把 `src` 包装成 Zstandard 解码流。

