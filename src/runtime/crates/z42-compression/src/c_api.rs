//! Standalone byte-buffer C API. Script module registration lives in registration.rs.
//! Algorithms and stream handles live in compression.rs. Returned buffers must
//! be released through this image's own free function.

use std::cell::RefCell;
use std::os::raw::c_char;

#[path = "compression.rs"]
pub(crate) mod compression;

// ── Error codes ──────────────────────────────────────────────────────────────

/// Returned from every entry. Zero is success; non-zero values map to
/// specific error categories that the z42-side wrapper translates into
/// `Std.Compression.CompressionException`. The numeric values are part of the C ABI
/// and must stay stable (additions OK; reuses or removals are a major
/// version bump).
pub const Z42_COMPRESSION_OK: i32 = 0;
pub const Z42_COMPRESSION_ERR_INVALID_INPUT: i32 = 1;
pub const Z42_COMPRESSION_ERR_INVALID_LEVEL: i32 = 2;
pub const Z42_COMPRESSION_ERR_INVALID_MODE:  i32 = 3;
pub const Z42_COMPRESSION_ERR_DECOMPRESS:    i32 = 4;
pub const Z42_COMPRESSION_ERR_COMPRESS:      i32 = 5;
pub const Z42_COMPRESSION_ERR_UNKNOWN_SLOT:  i32 = 6;
pub const Z42_COMPRESSION_ERR_INTERNAL:      i32 = 99;

// ── ABI version handshake ────────────────────────────────────────────────────

/// Contract version for the standalone byte-buffer exports. Callers check it
/// before binding signatures; changes to parameters, calling conventions or error
/// code meanings require a version bump. The script module protocol has its own header.
pub const Z42_COMPRESSION_ABI_VERSION: u32 = 1;

/// Returns [`Z42_COMPRESSION_ABI_VERSION`]. The first symbol z42vm resolves when
/// dlopen'ing this library; a mismatch — or this symbol missing on a
/// pre-versioning build — makes the loader skip the library rather than risk
/// calling a signature-incompatible ABI.
#[unsafe(no_mangle)]
pub extern "C" fn z42_compression_abi_version() -> u32 {
    Z42_COMPRESSION_ABI_VERSION
}

thread_local! {
    static LAST_ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Set the thread-local last-error string. Called by every error path in
/// the compression entries; queried by z42vm via
/// [`z42_compression_last_error`] when an entry returns non-zero.
pub(crate) fn set_last_error(msg: impl Into<String>) {
    LAST_ERROR.with(|e| *e.borrow_mut() = msg.into());
}

/// Returns a pointer to the current thread's last error message as a
/// NUL-terminated UTF-8 string. Empty string when no error pending. The
/// pointer is valid until the next entry call on the same thread.
#[unsafe(no_mangle)]
pub extern "C" fn z42_compression_last_error() -> *const c_char {
    LAST_ERROR.with(|e| {
        let s = e.borrow();
        // The string in the cell is always kept NUL-terminated by setters
        // (set_last_error always appends \0). Borrow's lifetime is the
        // closure but we return a pointer; safe because the cell lives
        // for the thread's lifetime and the next setter overwrites in
        // place rather than reallocating.
        s.as_ptr() as *const c_char
    })
}

// ── One-shot entries ─────────────────────────────────────────────────────────

/// Compress `input` with DEFLATE / zlib / gzip depending on `mode`.
///
/// - `input_ptr` / `input_len`: input bytes
/// - `level`: 1..=9 (1 = fastest, 6 = default, 9 = best)
/// - `mode`: 0 = raw DEFLATE (RFC 1951), 1 = zlib (RFC 1950), 2 = gzip (RFC 1952)
/// - `out_ptr` / `out_len`: on success, written with a heap-allocated
///   buffer (caller takes ownership via [`z42_compression_free`])
///
/// Returns [`Z42_COMPRESSION_OK`] on success, otherwise an error code +
/// sets thread-local last-error message.
///
/// # Safety
///
/// All pointers must be valid; `input_ptr` for `input_len` bytes,
/// `out_ptr` / `out_len` for 1 element each. `out_ptr` is overwritten
/// regardless of return code (NULL on error).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_compression_deflate_compress(
    input_ptr: *const u8, input_len: usize,
    level: i32, mode: i32,
    out_ptr: *mut *mut u8, out_len: *mut usize,
) -> i32 {
    unsafe {
        *out_ptr = std::ptr::null_mut();
        *out_len = 0;
        let input = std::slice::from_raw_parts(input_ptr, input_len);
        match compression::deflate_compress(input, level, mode) {
            Ok(bytes) => write_owned_buffer(bytes, out_ptr, out_len),
            Err((code, msg)) => { set_last_error(format!("{msg}\0")); code }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_compression_deflate_decompress(
    input_ptr: *const u8, input_len: usize,
    mode: i32,
    out_ptr: *mut *mut u8, out_len: *mut usize,
) -> i32 {
    unsafe {
        *out_ptr = std::ptr::null_mut();
        *out_len = 0;
        let input = std::slice::from_raw_parts(input_ptr, input_len);
        match compression::deflate_decompress(input, mode) {
            Ok(bytes) => write_owned_buffer(bytes, out_ptr, out_len),
            Err((code, msg)) => { set_last_error(format!("{msg}\0")); code }
        }
    }
}

// zstd: not built for wasm32 (the libzstd C source needs WASI SDK /
// emscripten clang that's not in the standard wasm32-unknown-unknown
// toolchain). Wasm callers that reach these entries get
// Z42_COMPRESSION_ERR_INVALID_MODE with the "zstd not supported on
// wasm32" message via z42_compression_last_error. Tracked in
// compression.md Deferred → `compression-future-wasm-zstd`.

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_compression_zstd_compress(
    input_ptr: *const u8, input_len: usize,
    level: i32,
    out_ptr: *mut *mut u8, out_len: *mut usize,
) -> i32 {
    unsafe {
        *out_ptr = std::ptr::null_mut();
        *out_len = 0;
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (input_ptr, input_len, level);
            set_last_error("zstd not supported on wasm32 — see compression.md Deferred: compression-future-wasm-zstd\0");
            return Z42_COMPRESSION_ERR_INVALID_MODE;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let input = std::slice::from_raw_parts(input_ptr, input_len);
            match compression::zstd_compress(input, level) {
                Ok(bytes) => write_owned_buffer(bytes, out_ptr, out_len),
                Err((code, msg)) => { set_last_error(format!("{msg}\0")); code }
            }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_compression_zstd_decompress(
    input_ptr: *const u8, input_len: usize,
    out_ptr: *mut *mut u8, out_len: *mut usize,
) -> i32 {
    unsafe {
        *out_ptr = std::ptr::null_mut();
        *out_len = 0;
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (input_ptr, input_len);
            set_last_error("zstd not supported on wasm32 — see compression.md Deferred: compression-future-wasm-zstd\0");
            return Z42_COMPRESSION_ERR_INVALID_MODE;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let input = std::slice::from_raw_parts(input_ptr, input_len);
            match compression::zstd_decompress(input) {
                Ok(bytes) => write_owned_buffer(bytes, out_ptr, out_len),
                Err((code, msg)) => { set_last_error(format!("{msg}\0")); code }
            }
        }
    }
}

// ── Brotli one-shot (add-z42-compression-brotli) ─────────────────
//
// Pure-Rust brotli works on every target (including wasm32) so no cfg
// gating needed. RFC 7932.

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_compression_brotli_compress(
    input_ptr: *const u8, input_len: usize,
    level: i32,
    out_ptr: *mut *mut u8, out_len: *mut usize,
) -> i32 {
    unsafe {
        *out_ptr = std::ptr::null_mut();
        *out_len = 0;
        let input = std::slice::from_raw_parts(input_ptr, input_len);
        match compression::brotli_compress(input, level) {
            Ok(bytes) => write_owned_buffer(bytes, out_ptr, out_len),
            Err((code, msg)) => { set_last_error(format!("{msg}\0")); code }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_compression_brotli_decompress(
    input_ptr: *const u8, input_len: usize,
    out_ptr: *mut *mut u8, out_len: *mut usize,
) -> i32 {
    unsafe {
        *out_ptr = std::ptr::null_mut();
        *out_len = 0;
        let input = std::slice::from_raw_parts(input_ptr, input_len);
        match compression::brotli_decompress(input) {
            Ok(bytes) => write_owned_buffer(bytes, out_ptr, out_len),
            Err((code, msg)) => { set_last_error(format!("{msg}\0")); code }
        }
    }
}

// ── LZ4 one-shot (add-z42-compression-lz4) ───────────────────────
//
// Pure-Rust lz4_flex works on every target. LZ4 frame format wraps the
// raw block algorithm with magic + descriptor + EndMark so it interops
// with the standard `lz4` CLI / .lz4 files.

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_compression_lz4_compress(
    input_ptr: *const u8, input_len: usize,
    level: i32,
    out_ptr: *mut *mut u8, out_len: *mut usize,
) -> i32 {
    unsafe {
        *out_ptr = std::ptr::null_mut();
        *out_len = 0;
        let input = std::slice::from_raw_parts(input_ptr, input_len);
        match compression::lz4_compress(input, level) {
            Ok(bytes) => write_owned_buffer(bytes, out_ptr, out_len),
            Err((code, msg)) => { set_last_error(format!("{msg}\0")); code }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_compression_lz4_decompress(
    input_ptr: *const u8, input_len: usize,
    out_ptr: *mut *mut u8, out_len: *mut usize,
) -> i32 {
    unsafe {
        *out_ptr = std::ptr::null_mut();
        *out_len = 0;
        let input = std::slice::from_raw_parts(input_ptr, input_len);
        match compression::lz4_decompress(input) {
            Ok(bytes) => write_owned_buffer(bytes, out_ptr, out_len),
            Err((code, msg)) => { set_last_error(format!("{msg}\0")); code }
        }
    }
}

// ── Streaming entries (slot-based) ───────────────────────────────────────────
//
// Streaming compressors / decompressors live in a process-global slot
// table inside this crate (thread-safe). The slot id is an opaque u64
// that z42vm marshals as i64 across the FFI boundary.

/// Begin a streaming compressor (`is_decompress = 0`) or decompressor
/// (`is_decompress != 0`). Returns the slot id via `out_slot_id` or an
/// error code.
///
/// `algo`: 0 = raw deflate, 1 = zlib, 2 = gzip, 10 = zstd.
/// `level`: ignored for decompress; 1..=9 for deflate family, 1..=22 for zstd.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_compression_compressor_begin(
    algo: i32, level: i32, is_decompress: i32,
    out_slot_id: *mut u64,
) -> i32 {
    unsafe {
        *out_slot_id = 0;
        match compression::compressor_begin(algo, level, is_decompress != 0) {
            Ok(id) => { *out_slot_id = id; Z42_COMPRESSION_OK }
            Err((code, msg)) => { set_last_error(format!("{msg}\0")); code }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_compression_compressor_feed(
    slot_id: u64,
    chunk_ptr: *const u8, chunk_len: usize,
    out_ptr: *mut *mut u8, out_len: *mut usize,
) -> i32 {
    unsafe {
        *out_ptr = std::ptr::null_mut();
        *out_len = 0;
        let chunk = std::slice::from_raw_parts(chunk_ptr, chunk_len);
        match compression::compressor_feed(slot_id, chunk) {
            Ok(bytes) => write_owned_buffer(bytes, out_ptr, out_len),
            Err((code, msg)) => { set_last_error(format!("{msg}\0")); code }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_compression_compressor_finish(
    slot_id: u64,
    out_ptr: *mut *mut u8, out_len: *mut usize,
) -> i32 {
    unsafe {
        *out_ptr = std::ptr::null_mut();
        *out_len = 0;
        match compression::compressor_finish(slot_id) {
            Ok(bytes) => write_owned_buffer(bytes, out_ptr, out_len),
            Err((code, msg)) => { set_last_error(format!("{msg}\0")); code }
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_compression_compressor_dispose(slot_id: u64) -> i32 {
    compression::compressor_dispose(slot_id);
    Z42_COMPRESSION_OK
}

// ── Buffer free entry ────────────────────────────────────────────────────────

/// Free a buffer previously returned by any `*_compress` / `*_decompress`
/// entry. Pass the exact `(ptr, len)` pair received. Calling this with
/// mismatched length is **undefined behavior**.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_compression_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 { return; }
    unsafe { drop(Vec::from_raw_parts(ptr, len, len)); }
}

// ── Helper: hand a Vec<u8> off to the caller as a (ptr, len) pair ────────────

unsafe fn write_owned_buffer(buf: Vec<u8>, out_ptr: *mut *mut u8, out_len: *mut usize) -> i32 {
    let len = buf.len();
    let mut boxed = buf.into_boxed_slice();
    let ptr = boxed.as_mut_ptr();
    std::mem::forget(boxed);
    unsafe {
        *out_ptr = ptr;
        *out_len = len;
    }
    Z42_COMPRESSION_OK
}

#[cfg(test)]
#[path = "compression_tests.rs"]
mod compression_tests;

pub(crate) unsafe fn resolve_symbol(name: *const c_char) -> *const std::ffi::c_void {
    if name.is_null() { return std::ptr::null(); }
    match unsafe { std::ffi::CStr::from_ptr(name) }.to_bytes() {
        b"z42_compression_abi_version" => z42_compression_abi_version as *const () as *const std::ffi::c_void,
        b"z42_compression_deflate_compress" => z42_compression_deflate_compress as *const () as *const std::ffi::c_void,
        b"z42_compression_deflate_decompress" => z42_compression_deflate_decompress as *const () as *const std::ffi::c_void,
        b"z42_compression_zstd_compress" => z42_compression_zstd_compress as *const () as *const std::ffi::c_void,
        b"z42_compression_zstd_decompress" => z42_compression_zstd_decompress as *const () as *const std::ffi::c_void,
        b"z42_compression_brotli_compress" => z42_compression_brotli_compress as *const () as *const std::ffi::c_void,
        b"z42_compression_brotli_decompress" => z42_compression_brotli_decompress as *const () as *const std::ffi::c_void,
        b"z42_compression_lz4_compress" => z42_compression_lz4_compress as *const () as *const std::ffi::c_void,
        b"z42_compression_lz4_decompress" => z42_compression_lz4_decompress as *const () as *const std::ffi::c_void,
        b"z42_compression_compressor_begin" => z42_compression_compressor_begin as *const () as *const std::ffi::c_void,
        b"z42_compression_compressor_feed" => z42_compression_compressor_feed as *const () as *const std::ffi::c_void,
        b"z42_compression_compressor_finish" => z42_compression_compressor_finish as *const () as *const std::ffi::c_void,
        b"z42_compression_compressor_dispose" => z42_compression_compressor_dispose as *const () as *const std::ffi::c_void,
        b"z42_compression_free" => z42_compression_free as *const () as *const std::ffi::c_void,
        b"z42_compression_last_error" => z42_compression_last_error as *const () as *const std::ffi::c_void,
        _ => std::ptr::null(),
    }
}
