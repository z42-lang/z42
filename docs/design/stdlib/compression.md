# z42.compression

Gzip / zlib / DEFLATE / Zstandard primitives plus Tar / Zip archive
parsing. The first stdlib package whose native code lives **outside**
`z42vm` — built as a separate `cdylib` (`libz42_compression.{so,dylib,
dll}`) loaded on demand via the native ext loader
([`docs/internals/src/runtime/native-ext-loader.md`](../../internals/src/runtime/native-ext-loader.md)).

> Spec: [`docs/spec/archive/2026-05-24-add-z42-compression/`](../../spec/archive/2026-05-24-add-z42-compression/)
> shipped 2026-05-24.

## v0 API

### `namespace Std.Compression`

```z42
public static class Gzip {                                    // RFC 1952
    // One-shot
    public static byte[] Compress(byte[] data);
    public static byte[] Compress(byte[] data, int level);    // 1..=9
    public static byte[] Decompress(byte[] data);

    // Streaming (refactor-compression-stream-on-iostream, 2026-05-24):
    // returns a Std.IO.Stream that wraps dest / src — composes with
    // MemoryStream (today) and future FileStream / NetworkStream.
    public static Std.IO.Stream WrapWrite(Std.IO.Stream dest);
    public static Std.IO.Stream WrapWrite(Std.IO.Stream dest, int level);
    public static Std.IO.Stream WrapRead(Std.IO.Stream src);
}

public static class Zlib    { /* same shape, RFC 1950 framing */ }
public static class Deflate { /* same shape, raw RFC 1951      */ }

public static class Zstd {
    public static byte[]        Compress(byte[] data);
    public static byte[]        Compress(byte[] data, int level);  // 1..=22
    public static byte[]        Decompress(byte[] data);
    public static Std.IO.Stream WrapWrite(Std.IO.Stream dest);
    public static Std.IO.Stream WrapWrite(Std.IO.Stream dest, int level);
    public static Std.IO.Stream WrapRead(Std.IO.Stream src);
}

public static class Compression {
    public static int Fastest;     // 1
    public static int Default;     // 6
    public static int Best;        // 9
    public static int ZstdFastest; // 1
    public static int ZstdDefault; // 3
    public static int ZstdBest;    // 22
}
```

### Pipeline composition (`WrapWrite` / `WrapRead`)

```z42
using Std.IO;
using Std.Compression;

// Compress straight into an in-memory destination:
MemoryStream dest = new MemoryStream();
Stream enc = Gzip.WrapWrite(dest);
enc.Write(plaintext, 0, plaintext.Length);
enc.Close();                                  // emits gzip footer
byte[] compressed = dest.ToArray();

// Decompress straight from an in-memory source:
MemoryStream src = new MemoryStream(compressedBytes);
Stream dec = Gzip.WrapRead(src);
byte[] plain = dec.ReadAllBytes();
```

Once `FileStream` / `NetworkStream` land (separate follow-up specs)
they slot into the same factories with no API change:

```z42
FileStream f = new FileStream("response.gz");
Stream gunzipped = Gzip.WrapRead(f);
TextReader text = new TextReader(gunzipped, Utf8);
string line = text.ReadLine();
```

**Caveats** (per Deferred section):

- `WrapRead` v0 buffers the entire source upfront and bulk-decompresses
  on first `Read` — full decompressed payload sits in memory. Sufficient
  for HTTP / log / metadata workloads (≤ ~100 MB); multi-GB streams
  need the `compression-future-streaming-decode` upgrade.
- `WrapWrite` is true streaming — `Write` calls feed the encoder
  chunk-by-chunk, output forwards to `dest` immediately. Caller MUST
  call `Close()` to flush the codec's footer / final state; without
  it the output is truncated.
- Neither wrapper closes the wrapped `dest` / `src` — caller owns
  their lifecycle (matches .NET `leaveOpen=true` behaviour, safer
  default for pipeline composition).

### `namespace Std.Archive`

```z42
public class TarEntry {
    public string Name;
    public byte[] Content;
    public int    Mode;
}

public static class Tar {
    public static TarEntry[] Read(byte[] tarBytes);
    public static byte[]     Write(TarEntry[] entries);
}

public class ZipEntry {
    public string Name;
    public byte[] Content;
    public int    CompressionMethod;  // 0 = STORE, 8 = DEFLATE
}

public static class Zip {
    public static ZipEntry[] Read(byte[] zipBytes);
    public static byte[]     ExtractFile(byte[] zipBytes, string name);
    // Write deferred — see "Deferred / Future Work" below.
}
```

### `namespace Std`

```z42
public class CompressionException : Exception { /* */ }
public class ArchiveException : Exception { /* */ }
```

## Algorithm backends (build-time)

The `z42-compression` cdylib uses the standard Rust ecosystem crates,
with target-specific feature selection:

| Algorithm family | Crate | Non-wasm target | wasm32 target |
|------------------|-------|-----------------|---------------|
| DEFLATE / zlib / gzip | `flate2` | feature `zlib-ng` (vendored C, SIMD-optimised, 200–280 MB/s) | default `rust_backend` = `miniz_oxide` (pure Rust, 50–80 MB/s; no SIMD in wasm so zlib-ng has no advantage + 200 KB larger bundle) |
| Zstandard | `zstd` | libzstd vendored via `zstd-sys` | **not built** — `zstd-sys`'s C source needs WASI SDK / emscripten clang that's not in the standard `wasm32-unknown-unknown` toolchain. Wasm callers reaching `Std.Compression.Zstd.*` get `CompressionException("zstd not supported on wasm32")`. See Deferred → `compression-future-wasm-zstd`. |

Cross-target build commands:

```
cargo build --release -p z42-compression                       # native (zlib-ng)
cargo build --release -p z42-compression --target wasm32-unknown-unknown
                                                                # auto-fallback to miniz_oxide
```

## Streaming semantics (v0)

| Side | Behaviour |
|------|-----------|
| **Encoder** (Gzip / Zlib / Deflate / Zstd) | True streaming. `Feed(chunk)` writes through to the underlying encoder and returns whatever bytes the compressor has currently produced. Output may be empty for several Feed calls in a row — the encoder is buffering. `Finish` flushes pending state and emits the footer. |
| **Decoder** (Gzip / Zlib / Deflate / Zstd) | v0 simplification: accumulates fed chunks in an internal buffer and bulk-decompresses at `Finish`. Sufficient for HTTP / log payloads (typically < 100 MB). Unbounded streaming decode (multi-GB tarballs, tail-following gzip logs) requires the v1 upgrade described below. |

## Performance

Cdylib boundary cost: per call, ~1 µs for argument marshalling
(`Value::Array<I64>` ↔ `Vec<u8>`) + the dlopen'd fn-ptr indirect call.
For payloads larger than ~1 KB this is dominated by algorithm work
(compression itself is the bottleneck, not the FFI).

Local measurements on macos-arm64 release build:

| Operation | Throughput |
|-----------|------------|
| Gzip compress, 1 MB random text, level=6 | ~240 MB/s |
| Gzip decompress, same payload | ~420 MB/s |
| Zstd compress, 1 MB, level=3 | ~600 MB/s |
| Zstd decompress, same payload | ~1.2 GB/s |

These match the underlying C library throughput within noise — the
wrapper overhead is < 0.5%.

## Deferred / Future Work

### ~~`compression-future-zip-write`~~ — **✅ 已落地 2026-05-27 (add-zip-write)**

Shipped: `Zip.Write(ZipEntry[]) → byte[]` builds `.zip` archives in
memory. Per-entry method 0 (STORE) or 8 (DEFLATE, via
`Std.Compression.Deflate.Compress`). CRC-32 (poly 0xEDB88320 IEEE
802.3) computed inline via 256-entry lookup table. UTF-8 filename
encoder inlined to keep Zip self-contained.

Implementation chose the "flat-buffer single-pass + parallel
offset/CRC arrays" path rather than the original design doc's "needs
byte[][]" hypothetical — a single `_GrowBuf` byte accumulator with
append + offset tracking sidesteps the absence of nested-array
syntax in z42 at no cost.

GPBF bit 11 always set (UTF-8 names). DOS mtime hard-coded to
1980-01-01 (Zip's epoch); per-entry mtime preservation is a separate
follow-up. ZIP64 / encryption / extra fields / comments still
deferred.

Tests: 10/10 GREEN (STORE / DEFLATE round-trip, mixed methods,
empty archive (22-byte EOCD only), magic-byte verification, method
validation, ExtractAllTo pipeline, empty content, UTF-8 filename
byte-fidelity, DEFLATE beats STORE on repetitive input).

### `compression-future-zip-create-from-directory`

`Zip.CreateFromDirectory(dir) → byte[]`（对称于已落地的 `Zip.ExtractAllTo`）：
递归枚举 `dir` 下文件 → 每个相对路径作为 entry name → `Zip.Write` 组装。

- **来源**：stdlib-p1-p2-backlog（已归档/删除）的 `Zip.ExtractAllTo` 行 follow-up 注记。
- **前置依赖**：`Zip.Write`（✅ 已落地 2026-05-27），现已解锁。
- **触发原因**：当年 Zip.Write 未就绪而推迟；纯 z42 atop `Zip.Write` + `Directory.Enumerate` 递归即可，小工作量。
- **当前 workaround**：调用方自己枚举目录 + 构 `ZipEntry[]` + `Zip.Write`。

### ~~`compression-future-streaming-decode`~~ — **✅ 已落地 2026-05-27 (add-compression-streaming-decode)**

Shipped: cdylib's slot-table decoders refactored from
`*Dec(Vec<u8>)` accumulators to push-mode `flate2::write::*Decoder`
(deflate / zlib / gzip) and `zstd::stream::write::Decoder` (zstd).
`compressor_feed` now writes the chunk into the decoder and returns
the bytes that forwarded into the inner `Vec` *that* call — so
multi-MB inputs surface decoded output progressively instead of
waiting for `compressor_finish`. End-to-end correctness verified by
the existing stream_pipeline + gzip_round_trip + zstd_round_trip
tests (all GREEN unchanged).

> **2026-06-09 更正（`compression-decoder-pull-mode`）**：上面这条原称"消费端
> `Stream.WrapRead` 已是 chunk-by-chunk"——**与当时实际不符**。cdylib 虽 2026-05-27
> 就流式，但 z42 消费端 `CompressionDecoderStream` 当时仍 v0 lazy-bulk（首次 `Read`
> 即 `ReadAllBytes` + 一次性 feed + finish，全量驻留内存）。`compression-decoder-pull-mode`
> (2026-06-09) 把消费端迁移到真 per-chunk pull（slot 跨 Read 持有，逐 chunk 从 `_src`
> pull + `_CompressorFeed` 增量输出，src EOF 才 `_CompressorFinish`），流式解压**至此
> 真正端到端**——多-GB/网络流不再在首个 `Read` 返回前 materialise 全量。`WrapWrite`
> 一侧本就是真流式（每 `Write` 即时 feed/forward），不受影响。

**Follow-up gaps** (separate specs):
- `Tar.ExtractStream(Stream src, string dir)` — Tar reader currently
  takes `byte[]`; pairs naturally with streaming gzip for `.tar.gz`
  extraction (avoid ~250 MB peak on 30 MB tarballs)
- Wasm zstd decode still returns the unsupported error (gated on
  `compression-future-wasm-zstd`)

### ~~`compression-future-brotli`~~ — ✅ 已落地 2026-05-27 (`add-z42-compression-brotli`)

`Std.Compression.Brotli.{Compress(data[, level]) / Decompress}` via
pure-Rust `brotli` crate (no C deps, wasm-compatible). Levels 0..=11
(default 4 — quality 11 is O(seconds) on small inputs even in
optimised builds). Unlocked `net-future-http-compression` brotli
portion (shipped 2026-05-30 by `add-z42-net-http-brotli`).

### ~~`compression-future-lz4`~~ — **✅ 已落地 2026-05-27 (add-z42-compression-lz4)**

Shipped: `Std.Compression.Lz4.{Compress(data[,level])/Decompress}`
backed by pure-Rust `lz4_flex` crate. LZ4 frame format (the standard
`.lz4` file wrapper — magic + descriptor + blocks + EndMark) so output
interops with the standard `lz4` CLI. Pure Rust → works on every
target including wasm32. `level` arg accepted for API symmetry but
ignored (base LZ4 has no level dial; LZ4-HC variant deferred).

Tests: 6/6 GREEN (short-string round-trip, empty input, repeated
pattern compresses >5×, frame magic 0x184D2204, level-independence,
full 256-byte-value binary payload).

### `compression-future-xz`

- **来源**：add-z42-compression v0 algorithm scope
- **触发原因**：xz use case is narrower (Linux kernel /
  Debian packages). Zstd covers most ratio cases adequately.
- **触发条件**：concrete user demand. Available via `lzma-rs` (pure Rust,
  decode-only) or `xz2` (C dep) when needed.

### `compression-future-libdeflate-batch`

- **来源**：add-z42-compression v0 perf scope
- **触发原因**：`libdeflate` is 1.5× faster than zlib-ng for one-shot
  batch operations but has no streaming API. Worth adding as a fast
  path for `Std.Compression.Deflate.CompressBatch(byte[]) -> byte[]`
  (etc.) if benchmarks show real workloads bottlenecked here.
- **触发条件**：benchmark evidence that batch operations (e.g. zpkg
  internal compression, bulk log compression) dominate workload time.

### `compression-future-zstd-dictionary`

- **来源**：libzstd supports preset dictionaries for small-payload
  compression but the v0 API doesn't expose them.
- **触发条件**：use case requiring high compression ratio on small
  payloads (RPC frames, log lines).

### `compression-future-zip-encryption`

- **触发条件**：use case requiring read or write of encrypted Zip
  archives (rare in z42's target domains).

### `compression-future-wasm-zstd`

- **来源**：add-z42-compression v0 wasm target build
- **触发原因**：`zstd-sys` crate vendors libzstd C source. cc-rs invokes
  `clang --target=wasm32-unknown-unknown` to build it, but a plain
  Apple/Linux clang doesn't have wasm target support — it's available
  through the WASI SDK or emscripten clang only. Standing up that
  toolchain in CI / dev environments is non-trivial.
- **触发条件**：either (a) WASI SDK becomes a documented part of the
  z42 dev environment, OR (b) a pure-Rust zstd implementation reaches
  production quality (e.g. `ruzstd` for decompress-only — currently
  experimental).
- **当前 workaround**：on wasm, `Std.Compression.Zstd.*` and
  `Std.Compression.CompressionStream` with algo=Zstd return
  `CompressionException("zstd not supported on wasm32")`. Apps that
  only need gzip / zlib / deflate are unaffected — those use
  `miniz_oxide` pure Rust which compiles cleanly for wasm.

## Architecture notes

Why a separate cdylib instead of `BUILTINS[]` in-VM? See
[`docs/internals/src/runtime/native-ext-loader.md`](../../internals/src/runtime/native-ext-loader.md)
for the full rationale; brief version:

- **Binary modularity**: z42vm core stays small (~4 MB); compression's
  ~600 KB of vendored C only loads when actually needed
- **Independent rebuild**: bumping flate2 / zstd version doesn't
  invalidate z42vm caches; build pipelines can update the cdylib
  out-of-band
- **Future migration template**: z42.net / z42.numerics will follow
  the same pattern (separate cdylib + dlopen at startup)

Why long-form `[Native(lib="z42_compression", entry="__deflate_compress")]`
in the z42 facade instead of the short `[Native("__deflate_compress")]`?

The `lib=` annotation is the **dependency hint** for the build pipeline
— a future SDK packager can scan a zpkg's facade attributes and emit a
manifest of "this zpkg needs the following native ext libs at runtime",
which lets dynamic SDK assembly (e.g. minimal SDK without
libz42_compression for apps that don't import `Std.Compression`) work
correctly. Short form `[Native("__name")]` carries no such hint.
