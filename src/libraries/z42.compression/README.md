# z42.compression

Gzip / Zlib / Deflate / Zstandard / Brotli / LZ4 primitives + Tar / Zip archive
read / write. First stdlib package whose native code lives **outside**
z42vm — built as a separate `cdylib` (`libz42_compression.{so,dylib,
dll}`) loaded via the native ext loader.

## One-shot

```z42
using Std.Compression;

byte[] compressed   = Gzip.Compress(original);          // default level 6
byte[] decompressed = Gzip.Decompress(compressed);
byte[] best         = Gzip.Compress(original, 9);

byte[] zstdEnc = Zstd.Compress(original);                // default level 3
byte[] zstdDec = Zstd.Decompress(zstdEnc);
```

## Pipeline (Stream-based)

```z42
using Std.IO;
using Std.Compression;

// Compress straight into a destination Stream:
MemoryStream dest = new MemoryStream();
Stream enc = Gzip.WrapWrite(dest);          // default level
enc.Write(plaintext, 0, plaintext.Length);
enc.Close();                                 // emits gzip footer
byte[] compressed = dest.ToArray();

// Decompress straight from a source Stream:
MemoryStream src = new MemoryStream(compressedBytes);
Stream dec = Gzip.WrapRead(src);
byte[] plain = dec.ReadAllBytes();
```

Same `WrapWrite / WrapRead` shape on `Zlib`, `Deflate`, `Zstd`. Future
`FileStream` / `NetworkStream` slot in transparently — no API change.

## Archive

```z42
using Std.Archive;

// Read existing zip:
ZipEntry[] entries = Zip.Read(zipBytes);
byte[] hello = Zip.ExtractFile(zipBytes, "hello.txt");

// Extract all entries to a directory:
int n = Zip.ExtractAllTo(zipBytes, "destDir");
// 自动 mkdir-p 父目录 + 目录条目 + Zip-Slip 防御。

// Tar write（Zip 写入用 Zip.Write(ZipEntry[])）:
TarEntry[] entries = new TarEntry[] {
    new TarEntry("readme.txt", contentBytes, 0644),
};
byte[] tarBytes = Tar.Write(entries);

// Tar extract to filesystem:
//   tar -xzf foo.tar.gz -C dest 等价 z42 流程
byte[] tarBytes = Gzip.Decompress(File.ReadAllBytes("foo.tar.gz"));
int n = Tar.ExtractTo(tarBytes, "dest");
// 自动 mkdir-p 父目录、设置可执行位、防御 ../ 路径穿越（Zip Slip）。
// in-memory buffered；真流式 pipeline 见 compression.md「不支持」。
```

See [docs/reference/src/stdlib/compression.md](../../../docs/reference/src/stdlib/compression.md)
for the full API + unsupported items.

## Core files

| File | Type | Role |
|------|------|------|
| `Gzip.z42` / `Zlib.z42` / `Deflate.z42` / `Zstd.z42` / `Brotli.z42` / `Lz4.z42` | `static class` (`Std.Compression`) | one-shot `Compress` / `Decompress`；Gzip / Zlib / Deflate / Zstd 另有 `WrapWrite` / `WrapRead` |
| `CompressionEncoderStream.z42` / `CompressionDecoderStream.z42` | `Stream` subclasses | Stream pipeline shared by the formats |
| `Compression.z42` | `static class Compression` | level constants (`Fastest` / `Default` / `Best`) |
| `Tar.z42` / `Zip.z42` | `static class` (`Std.Archive`) | archive read / write / extract |
| `Exceptions.z42` | exceptions (namespace `Std`) | compression / archive errors |

## Testing

```bash
xtask test stdlib z42.compression
```
