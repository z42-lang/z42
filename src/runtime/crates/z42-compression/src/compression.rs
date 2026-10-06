//! Algorithm implementations. Pure Rust (no z42 type deps); each function
//! returns `Result<Vec<u8>, (error_code, message)>` so [`lib.rs`] can
//! translate failures into the C ABI return code + thread-local
//! last-error slot.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Mutex, OnceLock};
use std::sync::atomic::{AtomicU64, Ordering};

use flate2::Compression as FlateLevel;
use flate2::read::{DeflateDecoder, GzDecoder, ZlibDecoder};
use flate2::write::{DeflateEncoder, GzEncoder, ZlibEncoder};
// add-compression-streaming-decode (2026-05-27): push-mode decoders for
// true streaming decompression. Same crate path as the encoders, write
// chunks in and decoded output forwards to the inner Vec<u8>.
use flate2::write::{
    DeflateDecoder as DeflateDecoderWrite,
    GzDecoder as GzDecoderWrite,
    ZlibDecoder as ZlibDecoderWrite,
};

use crate::{
    Z42_COMPRESSION_ERR_COMPRESS, Z42_COMPRESSION_ERR_DECOMPRESS,
    Z42_COMPRESSION_ERR_INVALID_INPUT, Z42_COMPRESSION_ERR_INVALID_LEVEL,
    Z42_COMPRESSION_ERR_INVALID_MODE, Z42_COMPRESSION_ERR_UNKNOWN_SLOT,
};

// ── Mode IDs (must stay in sync with z42 Std.Compression.Algo) ──────────────
const ALGO_DEFLATE_RAW: i32 = 0;
const ALGO_ZLIB:        i32 = 1;
const ALGO_GZIP:        i32 = 2;
const ALGO_ZSTD:        i32 = 10;
#[allow(dead_code)]
const ALGO_BROTLI:      i32 = 11;

type AlgoResult = Result<Vec<u8>, (i32, String)>;

fn flate_level(level: i32) -> Result<FlateLevel, (i32, String)> {
    match level {
        1..=9 => Ok(FlateLevel::new(level as u32)),
        n => Err((Z42_COMPRESSION_ERR_INVALID_LEVEL,
                  format!("level must be 1..=9, got {n}"))),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn zstd_level(level: i32) -> Result<i32, (i32, String)> {
    match level {
        1..=22 => Ok(level),
        n => Err((Z42_COMPRESSION_ERR_INVALID_LEVEL,
                  format!("level must be 1..=22, got {n}"))),
    }
}

// ── One-shot ─────────────────────────────────────────────────────────────────

pub fn deflate_compress(input: &[u8], level: i32, mode: i32) -> AlgoResult {
    let level = flate_level(level)?;
    let mut out = Vec::with_capacity(input.len() / 2 + 32);
    match mode {
        ALGO_DEFLATE_RAW => {
            let mut enc = DeflateEncoder::new(&mut out, level);
            enc.write_all(input).map_err(|e| (Z42_COMPRESSION_ERR_COMPRESS, e.to_string()))?;
            enc.finish().map_err(|e| (Z42_COMPRESSION_ERR_COMPRESS, e.to_string()))?;
        }
        ALGO_ZLIB => {
            let mut enc = ZlibEncoder::new(&mut out, level);
            enc.write_all(input).map_err(|e| (Z42_COMPRESSION_ERR_COMPRESS, e.to_string()))?;
            enc.finish().map_err(|e| (Z42_COMPRESSION_ERR_COMPRESS, e.to_string()))?;
        }
        ALGO_GZIP => {
            let mut enc = GzEncoder::new(&mut out, level);
            enc.write_all(input).map_err(|e| (Z42_COMPRESSION_ERR_COMPRESS, e.to_string()))?;
            enc.finish().map_err(|e| (Z42_COMPRESSION_ERR_COMPRESS, e.to_string()))?;
        }
        other => return Err((Z42_COMPRESSION_ERR_INVALID_MODE,
                             format!("unsupported deflate mode {other} (0=raw 1=zlib 2=gzip)"))),
    }
    Ok(out)
}

pub fn deflate_decompress(input: &[u8], mode: i32) -> AlgoResult {
    let mut out = Vec::with_capacity(input.len() * 2);
    match mode {
        ALGO_DEFLATE_RAW => DeflateDecoder::new(input).read_to_end(&mut out)
            .map_err(|e| (Z42_COMPRESSION_ERR_DECOMPRESS, format!("invalid raw deflate data: {e}")))?,
        ALGO_ZLIB => ZlibDecoder::new(input).read_to_end(&mut out)
            .map_err(|e| (Z42_COMPRESSION_ERR_DECOMPRESS, format!("invalid zlib data: {e}")))?,
        ALGO_GZIP => GzDecoder::new(input).read_to_end(&mut out)
            .map_err(|e| (Z42_COMPRESSION_ERR_DECOMPRESS, format!("invalid gzip data: {e}")))?,
        other => return Err((Z42_COMPRESSION_ERR_INVALID_MODE,
                             format!("unsupported deflate mode {other} (0=raw 1=zlib 2=gzip)"))),
    };
    Ok(out)
}

// zstd one-shot — non-wasm only. Wasm callers handle the unsupported case
// at the C ABI shim layer (see lib.rs); these functions don't compile in
// at all on wasm so any caller path is a logic bug.

#[cfg(not(target_arch = "wasm32"))]
pub fn zstd_compress(input: &[u8], level: i32) -> AlgoResult {
    let level = zstd_level(level)?;
    zstd::stream::encode_all(input, level)
        .map_err(|e| (Z42_COMPRESSION_ERR_COMPRESS, e.to_string()))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn zstd_decompress(input: &[u8]) -> AlgoResult {
    zstd::stream::decode_all(input)
        .map_err(|e| (Z42_COMPRESSION_ERR_DECOMPRESS, format!("invalid zstd data: {e}")))
}

// ── LZ4 (add-z42-compression-lz4 2026-05-27) ─────────────────────────────
//
// LZ4 frame format (the standard `lz4` CLI / file format wrapper around
// the raw LZ4 block algorithm — magic number + descriptor + blocks +
// EndMark). Pure-Rust `lz4_flex` works on wasm.
//
// LZ4 has no compression-level dial (the HC variant lives in separate
// crates); `level` arg accepted for API symmetry but ignored.

pub fn lz4_compress(input: &[u8], _level: i32) -> AlgoResult {
    let mut enc = lz4_flex::frame::FrameEncoder::new(Vec::new());
    std::io::Write::write_all(&mut enc, input)
        .map_err(|e| (Z42_COMPRESSION_ERR_COMPRESS, e.to_string()))?;
    enc.finish()
        .map_err(|e| (Z42_COMPRESSION_ERR_COMPRESS, e.to_string()))
}

/// LZ4 frame magic number (little-endian `0x184D2204`).
const LZ4_FRAME_MAGIC: [u8; 4] = [0x04, 0x22, 0x4D, 0x18];
/// Smallest complete frame: magic (4) + FLG/BD (2) + header checksum (1) + EndMark (4).
const LZ4_MIN_FRAME: usize = 11;

/// lz4_flex reads a stream that ends inside a frame header as a clean end of
/// stream, so a short or foreign input "decompressed" to nothing. Reject what
/// cannot be a frame before decoding: a non-empty input must start with the
/// frame magic (or a skippable-frame magic, `0x184D2A50`–`0x184D2A5F`) and be
/// at least one minimal frame long.
fn lz4_check_frame_start(input: &[u8]) -> Result<(), (i32, String)> {
    if input.is_empty() {
        return Ok(());
    }
    let bad = |why: &str| Err((Z42_COMPRESSION_ERR_DECOMPRESS, format!("invalid lz4 data: {why}")));
    if input.len() < 4 {
        return bad("shorter than a frame header");
    }
    let skippable = input[0] & 0xF0 == 0x50 && input[1..4] == [0x2A, 0x4D, 0x18];
    if input[..4] != LZ4_FRAME_MAGIC && !skippable {
        return bad("not an LZ4 frame (wrong magic number)");
    }
    if !skippable && input.len() < LZ4_MIN_FRAME {
        return bad("truncated frame");
    }
    Ok(())
}

pub fn lz4_decompress(input: &[u8]) -> AlgoResult {
    lz4_check_frame_start(input)?;
    let mut out: Vec<u8> = Vec::with_capacity(input.len() * 4);
    let mut dec = lz4_flex::frame::FrameDecoder::new(input);
    std::io::Read::read_to_end(&mut dec, &mut out)
        .map_err(|e| (Z42_COMPRESSION_ERR_DECOMPRESS, format!("invalid lz4 data: {e}")))?;
    Ok(out)
}

// ── Brotli (add-z42-compression-brotli 2026-05-27) ───────────────────────
//
// Pure-Rust brotli — works on every target including wasm32 (no C deps,
// unlike zstd). RFC 7932. Used by HTTP `Content-Encoding: br` and other
// web-y workloads.

/// Brotli quality range: 0..=11 (11 = best ratio, slowest). Default 11
/// matches the brotli reference encoder.
fn brotli_quality(level: i32) -> Result<u32, (i32, String)> {
    match level {
        0..=11 => Ok(level as u32),
        n => Err((Z42_COMPRESSION_ERR_INVALID_LEVEL,
                  format!("brotli level must be 0..=11, got {n}"))),
    }
}

pub fn brotli_compress(input: &[u8], level: i32) -> AlgoResult {
    let quality = brotli_quality(level)?;
    // window-bits 22 = brotli default; larger = better ratio at memory cost.
    let lgwin: u32 = 22;
    let mut out: Vec<u8> = Vec::with_capacity(input.len() / 2 + 64);
    let params = brotli::enc::BrotliEncoderParams {
        quality: quality as i32,
        lgwin: lgwin as i32,
        ..Default::default()
    };
    let mut reader = input;
    brotli::BrotliCompress(&mut reader, &mut out, &params)
        .map_err(|e| (Z42_COMPRESSION_ERR_COMPRESS, format!("brotli: {e:?}")))?;
    Ok(out)
}

pub fn brotli_decompress(input: &[u8]) -> AlgoResult {
    let mut out: Vec<u8> = Vec::with_capacity(input.len() * 4);
    let mut reader = input;
    brotli::BrotliDecompress(&mut reader, &mut out)
        .map_err(|e| (Z42_COMPRESSION_ERR_DECOMPRESS, format!("invalid brotli data: {e:?}")))?;
    Ok(out)
}

// ── Streaming slot table ────────────────────────────────────────────────────
//
// Process-global (cdylib's static state). Thread-safe via Mutex. The
// z42-side wrapper marshals the u64 slot id back to z42 as i64 via the
// `__compressor_*` builtin set.

enum Handle {
    DeflateEnc(DeflateEncoder<Vec<u8>>),
    ZlibEnc(ZlibEncoder<Vec<u8>>),
    GzipEnc(GzEncoder<Vec<u8>>),
    #[cfg(not(target_arch = "wasm32"))]
    ZstdEnc(zstd::stream::Encoder<'static, Vec<u8>>),
    // add-compression-streaming-decode (2026-05-27): push-mode decoders
    // backed by flate2::write::*Decoder<Vec<u8>>. `compressor_feed` writes
    // the input chunk and `std::mem::take`s the decoded output that
    // forwarded into the inner Vec, so callers see decoded bytes per
    // feed call instead of having to wait for finish.
    DeflateDec(DeflateDecoderWrite<Vec<u8>>),
    ZlibDec(ZlibDecoderWrite<Vec<u8>>),
    GzipDec(GzDecoderWrite<Vec<u8>>),
    #[cfg(not(target_arch = "wasm32"))]
    ZstdDec(zstd::stream::write::Decoder<'static, Vec<u8>>),
    // wasm32 keeps a `Vec<u8>` placeholder so `compressor_begin` /
    // `compressor_feed` don't fail-fast; `compressor_finish` returns
    // the unsupported error (preserves the existing wasm contract).
    #[cfg(target_arch = "wasm32")]
    ZstdDec(Vec<u8>),
}

fn slots() -> &'static Mutex<HashMap<u64, Handle>> {
    static SLOTS: OnceLock<Mutex<HashMap<u64, Handle>>> = OnceLock::new();
    SLOTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_slot_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

pub fn compressor_begin(algo: i32, level: i32, is_decompress: bool) -> Result<u64, (i32, String)> {
    let handle = if is_decompress {
        match algo {
            ALGO_DEFLATE_RAW => Handle::DeflateDec(DeflateDecoderWrite::new(Vec::new())),
            ALGO_ZLIB        => Handle::ZlibDec(ZlibDecoderWrite::new(Vec::new())),
            ALGO_GZIP        => Handle::GzipDec(GzDecoderWrite::new(Vec::new())),
            #[cfg(not(target_arch = "wasm32"))]
            ALGO_ZSTD        => {
                let dec = zstd::stream::write::Decoder::new(Vec::new())
                    .map_err(|e| (Z42_COMPRESSION_ERR_DECOMPRESS, e.to_string()))?;
                Handle::ZstdDec(dec)
            }
            #[cfg(target_arch = "wasm32")]
            ALGO_ZSTD        => Handle::ZstdDec(Vec::new()),
            other => return Err((Z42_COMPRESSION_ERR_INVALID_MODE,
                                 format!("unknown algo {other} for decompress"))),
        }
    } else {
        match algo {
            ALGO_DEFLATE_RAW => Handle::DeflateEnc(DeflateEncoder::new(Vec::new(), flate_level(level)?)),
            ALGO_ZLIB        => Handle::ZlibEnc(ZlibEncoder::new(Vec::new(), flate_level(level)?)),
            ALGO_GZIP        => Handle::GzipEnc(GzEncoder::new(Vec::new(), flate_level(level)?)),
            #[cfg(not(target_arch = "wasm32"))]
            ALGO_ZSTD => {
                let enc = zstd::stream::Encoder::new(Vec::new(), zstd_level(level)?)
                    .map_err(|e| (Z42_COMPRESSION_ERR_COMPRESS, e.to_string()))?;
                Handle::ZstdEnc(enc)
            }
            #[cfg(target_arch = "wasm32")]
            ALGO_ZSTD => return Err((Z42_COMPRESSION_ERR_INVALID_MODE,
                "zstd not supported on wasm32 — see compression.md Deferred: compression-future-wasm-zstd".into())),
            other => return Err((Z42_COMPRESSION_ERR_INVALID_MODE,
                                 format!("unknown algo {other} for compress"))),
        }
    };
    let id = next_slot_id();
    slots().lock().unwrap().insert(id, handle);
    Ok(id)
}

pub fn compressor_feed(slot_id: u64, chunk: &[u8]) -> Result<Vec<u8>, (i32, String)> {
    let mut s = slots().lock().unwrap();
    let h = s.get_mut(&slot_id)
        .ok_or((Z42_COMPRESSION_ERR_UNKNOWN_SLOT, format!("slot {slot_id} not found")))?;
    let out = match h {
        Handle::DeflateEnc(e) => {
            e.write_all(chunk).map_err(|e| (Z42_COMPRESSION_ERR_COMPRESS, e.to_string()))?;
            std::mem::take(e.get_mut())
        }
        Handle::ZlibEnc(e) => {
            e.write_all(chunk).map_err(|e| (Z42_COMPRESSION_ERR_COMPRESS, e.to_string()))?;
            std::mem::take(e.get_mut())
        }
        Handle::GzipEnc(e) => {
            e.write_all(chunk).map_err(|e| (Z42_COMPRESSION_ERR_COMPRESS, e.to_string()))?;
            std::mem::take(e.get_mut())
        }
        #[cfg(not(target_arch = "wasm32"))]
        Handle::ZstdEnc(e) => {
            e.write_all(chunk).map_err(|e| (Z42_COMPRESSION_ERR_COMPRESS, e.to_string()))?;
            std::mem::take(e.get_mut())
        }
        Handle::DeflateDec(d) => {
            d.write_all(chunk).map_err(|e| (Z42_COMPRESSION_ERR_DECOMPRESS, e.to_string()))?;
            std::mem::take(d.get_mut())
        }
        Handle::ZlibDec(d) => {
            d.write_all(chunk).map_err(|e| (Z42_COMPRESSION_ERR_DECOMPRESS, e.to_string()))?;
            std::mem::take(d.get_mut())
        }
        Handle::GzipDec(d) => {
            d.write_all(chunk).map_err(|e| (Z42_COMPRESSION_ERR_DECOMPRESS, e.to_string()))?;
            std::mem::take(d.get_mut())
        }
        #[cfg(not(target_arch = "wasm32"))]
        Handle::ZstdDec(d) => {
            d.write_all(chunk).map_err(|e| (Z42_COMPRESSION_ERR_DECOMPRESS, e.to_string()))?;
            std::mem::take(d.get_mut())
        }
        #[cfg(target_arch = "wasm32")]
        Handle::ZstdDec(buf) => {
            buf.extend_from_slice(chunk);
            Vec::new()
        }
    };
    Ok(out)
}

pub fn compressor_finish(slot_id: u64) -> Result<Vec<u8>, (i32, String)> {
    let h = slots().lock().unwrap().remove(&slot_id)
        .ok_or((Z42_COMPRESSION_ERR_UNKNOWN_SLOT, format!("slot {slot_id} not found")))?;
    match h {
        Handle::DeflateEnc(e) => e.finish().map_err(|e| (Z42_COMPRESSION_ERR_COMPRESS, e.to_string())),
        Handle::ZlibEnc(e)    => e.finish().map_err(|e| (Z42_COMPRESSION_ERR_COMPRESS, e.to_string())),
        Handle::GzipEnc(e)    => e.finish().map_err(|e| (Z42_COMPRESSION_ERR_COMPRESS, e.to_string())),
        #[cfg(not(target_arch = "wasm32"))]
        Handle::ZstdEnc(e)    => e.finish().map_err(|e| (Z42_COMPRESSION_ERR_COMPRESS, e.to_string())),
        // add-compression-streaming-decode (2026-05-27): drain the
        // decoder's tail (flush any buffered output). `finish` returns
        // the inner Vec by consuming the wrapper. Anything written but
        // not yet picked up by the most-recent feed comes out here.
        Handle::DeflateDec(d) => d.finish().map_err(|e| (Z42_COMPRESSION_ERR_DECOMPRESS, e.to_string())),
        Handle::ZlibDec(d)    => d.finish().map_err(|e| (Z42_COMPRESSION_ERR_DECOMPRESS, e.to_string())),
        Handle::GzipDec(d)    => d.finish().map_err(|e| (Z42_COMPRESSION_ERR_DECOMPRESS, e.to_string())),
        #[cfg(not(target_arch = "wasm32"))]
        Handle::ZstdDec(mut d) => {
            d.flush().map_err(|e| (Z42_COMPRESSION_ERR_DECOMPRESS, e.to_string()))?;
            Ok(std::mem::take(d.get_mut()))
        }
        #[cfg(target_arch = "wasm32")]
        Handle::ZstdDec(_)    => Err((Z42_COMPRESSION_ERR_INVALID_MODE,
            "zstd not supported on wasm32 — see compression.md Deferred: compression-future-wasm-zstd".into())),
    }
}

pub fn compressor_dispose(slot_id: u64) {
    let _ = slots().lock().unwrap().remove(&slot_id);
}

// Suppress dead-code lint when no callers reference these helpers under
// certain feature combinations.
#[allow(dead_code)]
const _: i32 = Z42_COMPRESSION_ERR_INVALID_INPUT;
