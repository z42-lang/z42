//! Stdlib native extension loader (add-z42-compression, 2026-05-22).
//!
//! Loads `lib<basename>.{so,dylib,dll}` files from the SDK native search
//! path at VM startup, dlopen's each, resolves a known set of `extern
//! "C"` entries by name, and registers VM-side wrapper functions into
//! `VmCore.ext_builtins` keyed by the `__<entry>` builtin names that
//! z42 facades reference via `[Native(lib="<basename>", entry="...")]`.
//!
//! # Difference vs Tier 1 (`loader.rs`)
//!
//! - **Tier 1** (`loader.rs`): general-purpose user-supplied native types
//!   with libffi-based marshal. Requires `Z42_VALUE_TAG_OBJECT` /
//!   `_STR` marshal (spec C5, not yet done).
//! - **Ext loader** (this file): curated stdlib native extensions with
//!   a hand-written wrapper per known entry. Marshalling is direct
//!   (Rust ↔ Rust) — both sides compile from the same source tree and
//!   ship version-locked together. Bypasses the C5 marshal gap.
//!
//! Currently the only known extension is `z42-compression`; adding a
//! new one means (1) ship its `libz42_<name>.*` artifact alongside
//! z42vm, and (2) extend the symbol set + wrappers below.
//!
//! # Wasm
//!
//! When the `bundled-compression` feature is on (wasm32 default), this
//! module bypasses dlopen and calls into the statically-linked
//! `z42_compression` rlib directly during [`load_all`]. The
//! `ext_builtins` registry is populated the same way; consumers can't
//! tell the difference.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use parking_lot::Mutex;

use crate::corelib::NativeFn;
use crate::metadata::Value;
use crate::vm_context::VmContext;

// ── ExtBuiltinTable ──────────────────────────────────────────────────────────

/// Indexed builtin table populated at VM startup from the ext libs.
///
/// `by_name` resolves a textual `__<entry>` name to a stable index; the
/// resolver caches this index as the low 31 bits of a `BuiltinId` with
/// the high bit (`0x8000_0000`) set so dispatch can distinguish static
/// vs ext builtins. `by_idx` lets the dispatch fast path go straight
/// from id → fn ptr without re-hashing the name.
#[derive(Default)]
pub struct ExtBuiltinTable {
    by_name: HashMap<String, u32>,
    by_idx:  Vec<NativeFn>,
}

impl ExtBuiltinTable {
    /// Register `(name, fn_ptr)`. Returns the stable index. Idempotent:
    /// re-registering the same name returns the existing index without
    /// changing the fn ptr (warn-only — first registration wins, so
    /// duplicate libs don't silently swap implementations).
    pub fn register(&mut self, name: &str, fn_ptr: NativeFn) -> u32 {
        if let Some(&idx) = self.by_name.get(name) {
            tracing::warn!("ext builtin `{name}` already registered; ignoring duplicate");
            return idx;
        }
        let idx = self.by_idx.len() as u32;
        self.by_idx.push(fn_ptr);
        self.by_name.insert(name.to_string(), idx);
        idx
    }

    pub fn lookup_id(&self, name: &str) -> Option<u32> {
        self.by_name.get(name).copied()
    }

    pub fn dispatch(&self, idx: u32) -> Option<NativeFn> {
        self.by_idx.get(idx as usize).copied()
    }
}

// ── Entry point ──────────────────────────────────────────────────────────────

/// Scan native search paths, dlopen each `libz42_*.{so,dylib,dll}`, and
/// register its symbols into `ctx.core.ext_builtins`. Failures are
/// logged via `tracing::warn` but never abort VM startup — apps that
/// don't need any ext lib still boot.
///
/// When `bundled-compression` is enabled (wasm default), the
/// z42-compression cdylib is statically linked; this function short-
/// circuits to the bundled registration path and skips dlopen entirely.
pub fn load_all(ctx: &VmContext) -> Result<()> {
    #[cfg(feature = "bundled-compression")]
    {
        register_bundled_compression(ctx);
        return Ok(());
    }

    #[cfg(not(feature = "bundled-compression"))]
    {
        load_via_dlopen(ctx)
    }
}

#[cfg(feature = "bundled-compression")]
fn register_bundled_compression(ctx: &VmContext) {
    let mut table = ctx.core.ext_builtins.lock();
    let symbols = compression_symbols_bundled();
    for (name, fn_ptr) in symbols {
        table.register(name, *fn_ptr);
    }
    tracing::debug!(
        "ext: registered {} bundled compression builtins",
        table.by_idx.len()
    );
}

#[cfg(not(feature = "bundled-compression"))]
fn load_via_dlopen(ctx: &VmContext) -> Result<()> {
    for dir in native_search_paths() {
        if !dir.is_dir() { continue; }
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                tracing::debug!("ext: skip {}: {}", dir.display(), e);
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = parse_z42_lib_name(&path) {
                if let Err(e) = load_one(ctx, &path, &name) {
                    tracing::warn!("ext: failed to load {}: {:#}", path.display(), e);
                }
            }
        }
    }
    Ok(())
}

// ── Search paths ─────────────────────────────────────────────────────────────

pub(crate) fn native_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // 1. Explicit override (CI / dev / power user). `Z42_NATIVE_PATH`
    //    is colon-separated on Unix, semicolon-separated on Windows.
    //    runtime-config-phase2 (2026-06-03): centralised through
    //    `RuntimeConfig::native_search_paths` (pre-split at config init).
    paths.extend(crate::config::runtime_config().native_search_paths.iter().cloned());

    // 2. Default SDK layout, relative to the running z42vm binary:
    //      <exec_dir>/../native/    — SDK package layout (bin/ and native/ siblings)
    //      <exec_dir>/native/       — dev / cargo-target layout when scripts symlink
    //      <exec_dir>              — raw cargo-target layout (libz42_*.dylib sit next
    //                                 to z42vm directly; added 2026-05-24 to remove
    //                                 the manual `ln -sf ../libz42_compression.dylib
    //                                 release/native/libz42_compression.dylib` setup
    //                                 step every fresh `cargo build` previously
    //                                 required). `parse_z42_lib_name` filters to
    //                                 `libz42_<name>.{so,dylib,dll}`, so dependency
    //                                 dylibs / `libz42.rlib` / `*.a` are ignored.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent_dir) = exe.parent() {
            if let Some(sdk_root) = parent_dir.parent() {
                paths.push(sdk_root.join("native"));
            }
            paths.push(parent_dir.join("native"));
            paths.push(parent_dir.to_path_buf());
        }
    }

    paths
}

/// Match `libz42_<name>.{so,dylib,dll}` (with or without `lib` prefix on
/// Windows). Returns the `<name>` suffix or `None` if the file doesn't
/// fit the convention.
pub(crate) fn parse_z42_lib_name(path: &std::path::Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let ext  = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if !matches!(ext, "so" | "dylib" | "dll") {
        return None;
    }
    let core = stem.strip_prefix("lib").unwrap_or(stem);
    core.strip_prefix("z42_").map(String::from)
}

// ── dlopen path (non-wasm) ───────────────────────────────────────────────────

#[cfg(not(feature = "bundled-compression"))]
fn load_one(ctx: &VmContext, path: &std::path::Path, name: &str) -> Result<()> {
    let lib = unsafe { libloading::Library::new(path)? };

    match name {
        "compression" => {
            let symbols = unsafe { compression_symbols_via_dlopen(&lib)? };
            let mut table = ctx.core.ext_builtins.lock();
            for (sym_name, fn_ptr) in symbols {
                table.register(sym_name, *fn_ptr);
            }
            tracing::debug!("ext: registered compression builtins from {}", path.display());
        }
        other => {
            tracing::warn!("ext: ignoring unknown lib `{}`", other);
        }
    }

    // Keep the library alive for the VM lifetime so its function pointers
    // stay valid. Lifetime parking pattern mirrors `loader.rs`.
    ctx.core.native_libs.lock().push(lib);
    Ok(())
}

// ── compression symbol table (dlopen path) ───────────────────────────────────

/// Raw C ABI signatures matching `src/runtime/crates/z42-compression/src/lib.rs`.
/// These must stay in sync byte-for-byte with the cdylib's `#[unsafe(no_mangle)]`
/// exports. Because we ship z42vm and z42-compression version-locked from
/// the same source tree, mismatches surface at link time when packaging.
type CDeflateCompressFn = unsafe extern "C" fn(
    *const u8, usize, i32, i32,
    *mut *mut u8, *mut usize,
) -> i32;
type CDeflateDecompressFn = unsafe extern "C" fn(
    *const u8, usize, i32,
    *mut *mut u8, *mut usize,
) -> i32;
type CZstdCompressFn = unsafe extern "C" fn(
    *const u8, usize, i32,
    *mut *mut u8, *mut usize,
) -> i32;
type CZstdDecompressFn = unsafe extern "C" fn(
    *const u8, usize,
    *mut *mut u8, *mut usize,
) -> i32;
// add-z42-compression-brotli (2026-05-27): same shape as zstd.
type CBrotliCompressFn = unsafe extern "C" fn(
    *const u8, usize, i32,
    *mut *mut u8, *mut usize,
) -> i32;
type CBrotliDecompressFn = unsafe extern "C" fn(
    *const u8, usize,
    *mut *mut u8, *mut usize,
) -> i32;
// add-z42-compression-lz4 (2026-05-27): same shape as brotli.
type CLz4CompressFn = unsafe extern "C" fn(
    *const u8, usize, i32,
    *mut *mut u8, *mut usize,
) -> i32;
type CLz4DecompressFn = unsafe extern "C" fn(
    *const u8, usize,
    *mut *mut u8, *mut usize,
) -> i32;
type CCompressorBeginFn = unsafe extern "C" fn(
    i32, i32, i32,
    *mut u64,
) -> i32;
type CCompressorFeedFn = unsafe extern "C" fn(
    u64,
    *const u8, usize,
    *mut *mut u8, *mut usize,
) -> i32;
type CCompressorFinishFn = unsafe extern "C" fn(
    u64,
    *mut *mut u8, *mut usize,
) -> i32;
type CCompressorDisposeFn = unsafe extern "C" fn(u64) -> i32;
type CFreeFn = unsafe extern "C" fn(*mut u8, usize);
type CLastErrorFn = unsafe extern "C" fn() -> *const std::os::raw::c_char;

/// The complete set of C ABI fn ptrs from libz42_compression. Stashed in
/// process-static `LoadedCompression` so wrapper closures can look them
/// up without re-querying the libloading::Library on every call.
///
/// `free` frees a buffer the cdylib returned (via its OWN allocator). unify-gc-heap
/// PR-3 fix: `take_owned_buffer` now copies the returned bytes into a Rust-owned Vec and
/// calls `free` on the original — the old approach (reconstruct a Rust `Vec::from_raw_parts`
/// + drop via z42vm's allocator) was an allocator-mismatch bug latent while the buffer was
/// never dropped (kept as `Bytes(Vec)` backing, no GC), surfaced by PR-3's copy-then-drop.
#[allow(dead_code)]
struct LoadedCompression {
    deflate_compress:    CDeflateCompressFn,
    deflate_decompress:  CDeflateDecompressFn,
    zstd_compress:       CZstdCompressFn,
    zstd_decompress:     CZstdDecompressFn,
    brotli_compress:     CBrotliCompressFn,
    brotli_decompress:   CBrotliDecompressFn,
    lz4_compress:        CLz4CompressFn,
    lz4_decompress:      CLz4DecompressFn,
    compressor_begin:    CCompressorBeginFn,
    compressor_feed:     CCompressorFeedFn,
    compressor_finish:   CCompressorFinishFn,
    compressor_dispose:  CCompressorDisposeFn,
    free:                CFreeFn,
    last_error:          CLastErrorFn,
}

static LOADED_COMPRESSION: Mutex<Option<LoadedCompression>> = Mutex::new(None);

#[cfg(not(feature = "bundled-compression"))]
unsafe fn compression_symbols_via_dlopen(
    lib: &libloading::Library,
) -> Result<&'static [(&'static str, NativeFn)]> {
    // libloading::Symbol::* deref to the underlying fn ptr. We copy the
    // fn ptrs out (Copy) and keep the Library alive separately via
    // VmCore.native_libs so the symbols stay resident.
    let deflate_compress: CDeflateCompressFn = *(lib.get(b"z42_compression_deflate_compress")?);
    let deflate_decompress: CDeflateDecompressFn = *(lib.get(b"z42_compression_deflate_decompress")?);
    let zstd_compress: CZstdCompressFn = *(lib.get(b"z42_compression_zstd_compress")?);
    let zstd_decompress: CZstdDecompressFn = *(lib.get(b"z42_compression_zstd_decompress")?);
    let brotli_compress: CBrotliCompressFn = *(lib.get(b"z42_compression_brotli_compress")?);
    let brotli_decompress: CBrotliDecompressFn = *(lib.get(b"z42_compression_brotli_decompress")?);
    let lz4_compress: CLz4CompressFn = *(lib.get(b"z42_compression_lz4_compress")?);
    let lz4_decompress: CLz4DecompressFn = *(lib.get(b"z42_compression_lz4_decompress")?);
    let compressor_begin: CCompressorBeginFn = *(lib.get(b"z42_compression_compressor_begin")?);
    let compressor_feed: CCompressorFeedFn = *(lib.get(b"z42_compression_compressor_feed")?);
    let compressor_finish: CCompressorFinishFn = *(lib.get(b"z42_compression_compressor_finish")?);
    let compressor_dispose: CCompressorDisposeFn = *(lib.get(b"z42_compression_compressor_dispose")?);
    let free: CFreeFn = *(lib.get(b"z42_compression_free")?);
    let last_error: CLastErrorFn = *(lib.get(b"z42_compression_last_error")?);

    *LOADED_COMPRESSION.lock() = Some(LoadedCompression {
        deflate_compress, deflate_decompress,
        zstd_compress, zstd_decompress,
        brotli_compress, brotli_decompress,
        lz4_compress, lz4_decompress,
        compressor_begin, compressor_feed, compressor_finish, compressor_dispose,
        free, last_error,
    });

    Ok(COMPRESSION_BUILTINS)
}

#[cfg(feature = "bundled-compression")]
fn compression_symbols_bundled() -> &'static [(&'static str, NativeFn)] {
    // Stash the rlib's `extern "C"` fn ptrs into LOADED_COMPRESSION so the
    // wrapper functions below can use the same path as the dlopen case.
    *LOADED_COMPRESSION.lock() = Some(LoadedCompression {
        deflate_compress:   z42_compression::z42_compression_deflate_compress,
        deflate_decompress: z42_compression::z42_compression_deflate_decompress,
        zstd_compress:      z42_compression::z42_compression_zstd_compress,
        zstd_decompress:    z42_compression::z42_compression_zstd_decompress,
        brotli_compress:    z42_compression::z42_compression_brotli_compress,
        brotli_decompress:  z42_compression::z42_compression_brotli_decompress,
        lz4_compress:       z42_compression::z42_compression_lz4_compress,
        lz4_decompress:     z42_compression::z42_compression_lz4_decompress,
        compressor_begin:   z42_compression::z42_compression_compressor_begin,
        compressor_feed:    z42_compression::z42_compression_compressor_feed,
        compressor_finish:  z42_compression::z42_compression_compressor_finish,
        compressor_dispose: z42_compression::z42_compression_compressor_dispose,
        free:               z42_compression::z42_compression_free,
        last_error:         z42_compression::z42_compression_last_error,
    });
    COMPRESSION_BUILTINS
}

/// Static mapping from `__<entry>` builtin name → wrapper that marshals
/// `Value` ↔ `*const u8` and dispatches into the cdylib via the
/// `LOADED_COMPRESSION` fn ptr table.
const COMPRESSION_BUILTINS: &[(&str, NativeFn)] = &[
    ("__deflate_compress",     wrap_deflate_compress),
    ("__deflate_decompress",   wrap_deflate_decompress),
    ("__zstd_compress",        wrap_zstd_compress),
    ("__zstd_decompress",      wrap_zstd_decompress),
    ("__brotli_compress",      wrap_brotli_compress),
    ("__brotli_decompress",    wrap_brotli_decompress),
    ("__lz4_compress",         wrap_lz4_compress),
    ("__lz4_decompress",       wrap_lz4_decompress),
    ("__compressor_begin",     wrap_compressor_begin),
    ("__compressor_feed",      wrap_compressor_feed),
    ("__compressor_finish",    wrap_compressor_finish),
    ("__compressor_dispose",   wrap_compressor_dispose),
];

// ── Wrapper helpers ──────────────────────────────────────────────────────────

fn require_byte_array(args: &[Value], idx: usize, ctx: &str) -> Result<Vec<u8>> {
    use anyhow::bail;
    match args.get(idx) {
        Some(Value::Array(rc)) => {
            let borrowed = rc.borrow();
            // packed-primitive-arrays Step 3: a packed `byte[]` (`Bytes` backing)
            // is already a contiguous `&[u8]` — copy it straight out, no per-byte
            // unbox (the "简化 extern call" win). Boxed falls back to validation.
            if let Some(b) = borrowed.as_bytes() {
                return Ok(b.to_vec());
            }
            let mut out = Vec::with_capacity(borrowed.len());
            for (i, v) in borrowed.iter_boxed().enumerate() {
                match v {
                    Value::I64(n) if (0..=255).contains(&n) => out.push(n as u8),
                    other => bail!("{}: arg {} byte {} not u8 in 0..=255: {:?}",
                                   ctx, idx, i, other),
                }
            }
            Ok(out)
        }
        Some(other) => bail!("{}: arg {} expected byte array, got {:?}", ctx, idx, other),
        None => bail!("{}: missing arg {}", ctx, idx),
    }
}

fn arg_i64(args: &[Value], idx: usize, ctx: &str) -> Result<i64> {
    use anyhow::bail;
    match args.get(idx) {
        Some(Value::I64(n)) => Ok(*n),
        Some(other) => bail!("{}: arg {} expected I64, got {:?}", ctx, idx, other),
        None => bail!("{}: missing arg {}", ctx, idx),
    }
}

fn arg_bool(args: &[Value], idx: usize, ctx: &str) -> Result<bool> {
    use anyhow::bail;
    match args.get(idx) {
        Some(Value::Bool(b)) => Ok(*b),
        Some(other) => bail!("{}: arg {} expected Bool, got {:?}", ctx, idx, other),
        None => bail!("{}: missing arg {}", ctx, idx),
    }
}

fn bytes_to_value(ctx: &VmContext, bytes: Vec<u8>) -> Value {
    // packed-primitive-arrays Step 3: pack the native result straight into a
    // `Bytes` backing — no intermediate `Vec<Value>` boxing.
    ctx.heap().alloc_bytes(bytes)
}

fn take_owned_buffer(free: CFreeFn, out_ptr: *mut u8, out_len: usize) -> Vec<u8> {
    if out_ptr.is_null() || out_len == 0 { return Vec::new(); }
    // unify-gc-heap PR-3 fix (2026-08-15): **copy** the cdylib's output into a Rust-owned Vec
    // (z42vm's global allocator), then free the cdylib's buffer via ITS OWN allocator
    // (`z42_compression_free`). The previous code reconstructed a Rust `Vec::from_raw_parts`
    // over the foreign buffer and let Rust's allocator drop it — a latent allocator-mismatch
    // bug (the dlopen'd cdylib may use a different global allocator than z42vm's mimalloc). It
    // was masked on origin/main because the reconstructed Vec was moved into the `Bytes(Vec)`
    // array backing and, with no GC pressure in the test harness, never dropped. PR-3 copies
    // the bytes into a GC block and drops the Vec immediately → freed the foreign buffer with
    // the wrong allocator → SIGSEGV on Linux (tolerated on macOS). Freeing via the cdylib's
    // own `free` is correct regardless of allocator.
    // SAFETY: `out_ptr`/`out_len` name the cdylib's freshly-returned buffer (non-null, len>0).
    let v = unsafe { std::slice::from_raw_parts(out_ptr, out_len) }.to_vec();
    // SAFETY: hand the same (ptr, len) back to the cdylib's allocator exactly once.
    unsafe { free(out_ptr, out_len); }
    v
}

fn last_error_string() -> String {
    let guard = LOADED_COMPRESSION.lock();
    if let Some(lc) = guard.as_ref() {
        let ptr = unsafe { (lc.last_error)() };
        if !ptr.is_null() {
            unsafe { std::ffi::CStr::from_ptr(ptr) }
                .to_string_lossy().trim_end_matches('\0').to_string()
        } else {
            String::new()
        }
    } else {
        String::new()
    }
}

// ── Wrappers (NativeFn signatures, called by interp / JIT dispatch) ──────────

fn wrap_deflate_compress(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    use anyhow::bail;
    const NAME: &str = "__deflate_compress";
    let input = require_byte_array(args, 0, NAME)?;
    let level = arg_i64(args, 1, NAME)? as i32;
    let mode  = arg_i64(args, 2, NAME)? as i32;

    let guard = LOADED_COMPRESSION.lock();
    let lc = guard.as_ref().ok_or_else(|| anyhow::anyhow!("{}: z42-compression not loaded", NAME))?;

    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let rc = unsafe {
        (lc.deflate_compress)(input.as_ptr(), input.len(),
                              level, mode,
                              &mut out_ptr, &mut out_len)
    };
    if rc != 0 {
        bail!("{}: {} (rc={})", NAME, last_error_string(), rc);
    }
    Ok(bytes_to_value(ctx, take_owned_buffer(lc.free, out_ptr, out_len)))
}

fn wrap_deflate_decompress(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    use anyhow::bail;
    const NAME: &str = "__deflate_decompress";
    let input = require_byte_array(args, 0, NAME)?;
    let mode  = arg_i64(args, 1, NAME)? as i32;

    let guard = LOADED_COMPRESSION.lock();
    let lc = guard.as_ref().ok_or_else(|| anyhow::anyhow!("{}: z42-compression not loaded", NAME))?;

    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let rc = unsafe {
        (lc.deflate_decompress)(input.as_ptr(), input.len(),
                                mode,
                                &mut out_ptr, &mut out_len)
    };
    if rc != 0 {
        bail!("{}: {} (rc={})", NAME, last_error_string(), rc);
    }
    Ok(bytes_to_value(ctx, take_owned_buffer(lc.free, out_ptr, out_len)))
}

fn wrap_zstd_compress(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    use anyhow::bail;
    const NAME: &str = "__zstd_compress";
    let input = require_byte_array(args, 0, NAME)?;
    let level = arg_i64(args, 1, NAME)? as i32;

    let guard = LOADED_COMPRESSION.lock();
    let lc = guard.as_ref().ok_or_else(|| anyhow::anyhow!("{}: z42-compression not loaded", NAME))?;

    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let rc = unsafe {
        (lc.zstd_compress)(input.as_ptr(), input.len(), level,
                           &mut out_ptr, &mut out_len)
    };
    if rc != 0 {
        bail!("{}: {} (rc={})", NAME, last_error_string(), rc);
    }
    Ok(bytes_to_value(ctx, take_owned_buffer(lc.free, out_ptr, out_len)))
}

fn wrap_zstd_decompress(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    use anyhow::bail;
    const NAME: &str = "__zstd_decompress";
    let input = require_byte_array(args, 0, NAME)?;

    let guard = LOADED_COMPRESSION.lock();
    let lc = guard.as_ref().ok_or_else(|| anyhow::anyhow!("{}: z42-compression not loaded", NAME))?;

    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let rc = unsafe {
        (lc.zstd_decompress)(input.as_ptr(), input.len(),
                             &mut out_ptr, &mut out_len)
    };
    if rc != 0 {
        bail!("{}: {} (rc={})", NAME, last_error_string(), rc);
    }
    Ok(bytes_to_value(ctx, take_owned_buffer(lc.free, out_ptr, out_len)))
}

fn wrap_brotli_compress(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    use anyhow::bail;
    const NAME: &str = "__brotli_compress";
    let input = require_byte_array(args, 0, NAME)?;
    let level = arg_i64(args, 1, NAME)? as i32;

    let guard = LOADED_COMPRESSION.lock();
    let lc = guard.as_ref().ok_or_else(|| anyhow::anyhow!("{}: z42-compression not loaded", NAME))?;

    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let rc = unsafe {
        (lc.brotli_compress)(input.as_ptr(), input.len(), level,
                             &mut out_ptr, &mut out_len)
    };
    if rc != 0 {
        bail!("{}: {} (rc={})", NAME, last_error_string(), rc);
    }
    Ok(bytes_to_value(ctx, take_owned_buffer(lc.free, out_ptr, out_len)))
}

fn wrap_brotli_decompress(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    use anyhow::bail;
    const NAME: &str = "__brotli_decompress";
    let input = require_byte_array(args, 0, NAME)?;

    let guard = LOADED_COMPRESSION.lock();
    let lc = guard.as_ref().ok_or_else(|| anyhow::anyhow!("{}: z42-compression not loaded", NAME))?;

    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let rc = unsafe {
        (lc.brotli_decompress)(input.as_ptr(), input.len(),
                               &mut out_ptr, &mut out_len)
    };
    if rc != 0 {
        bail!("{}: {} (rc={})", NAME, last_error_string(), rc);
    }
    Ok(bytes_to_value(ctx, take_owned_buffer(lc.free, out_ptr, out_len)))
}

fn wrap_lz4_compress(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    use anyhow::bail;
    const NAME: &str = "__lz4_compress";
    let input = require_byte_array(args, 0, NAME)?;
    let level = arg_i64(args, 1, NAME)? as i32;

    let guard = LOADED_COMPRESSION.lock();
    let lc = guard.as_ref().ok_or_else(|| anyhow::anyhow!("{}: z42-compression not loaded", NAME))?;

    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let rc = unsafe {
        (lc.lz4_compress)(input.as_ptr(), input.len(), level,
                          &mut out_ptr, &mut out_len)
    };
    if rc != 0 {
        bail!("{}: {} (rc={})", NAME, last_error_string(), rc);
    }
    Ok(bytes_to_value(ctx, take_owned_buffer(lc.free, out_ptr, out_len)))
}

fn wrap_lz4_decompress(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    use anyhow::bail;
    const NAME: &str = "__lz4_decompress";
    let input = require_byte_array(args, 0, NAME)?;

    let guard = LOADED_COMPRESSION.lock();
    let lc = guard.as_ref().ok_or_else(|| anyhow::anyhow!("{}: z42-compression not loaded", NAME))?;

    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let rc = unsafe {
        (lc.lz4_decompress)(input.as_ptr(), input.len(),
                            &mut out_ptr, &mut out_len)
    };
    if rc != 0 {
        bail!("{}: {} (rc={})", NAME, last_error_string(), rc);
    }
    Ok(bytes_to_value(ctx, take_owned_buffer(lc.free, out_ptr, out_len)))
}

fn wrap_compressor_begin(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    use anyhow::bail;
    const NAME: &str = "__compressor_begin";
    let algo  = arg_i64(args, 0, NAME)? as i32;
    let level = arg_i64(args, 1, NAME)? as i32;
    let is_decompress = arg_bool(args, 2, NAME)?;

    let guard = LOADED_COMPRESSION.lock();
    let lc = guard.as_ref().ok_or_else(|| anyhow::anyhow!("{}: z42-compression not loaded", NAME))?;

    let mut slot_id: u64 = 0;
    let rc = unsafe {
        (lc.compressor_begin)(algo, level, is_decompress as i32, &mut slot_id)
    };
    if rc != 0 {
        bail!("{}: {} (rc={})", NAME, last_error_string(), rc);
    }
    Ok(Value::I64(slot_id as i64))
}

fn wrap_compressor_feed(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    use anyhow::bail;
    const NAME: &str = "__compressor_feed";
    let slot_id = arg_i64(args, 0, NAME)? as u64;
    let chunk   = require_byte_array(args, 1, NAME)?;

    let guard = LOADED_COMPRESSION.lock();
    let lc = guard.as_ref().ok_or_else(|| anyhow::anyhow!("{}: z42-compression not loaded", NAME))?;

    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let rc = unsafe {
        (lc.compressor_feed)(slot_id,
                             chunk.as_ptr(), chunk.len(),
                             &mut out_ptr, &mut out_len)
    };
    if rc != 0 {
        bail!("{}: {} (rc={})", NAME, last_error_string(), rc);
    }
    Ok(bytes_to_value(ctx, take_owned_buffer(lc.free, out_ptr, out_len)))
}

fn wrap_compressor_finish(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    use anyhow::bail;
    const NAME: &str = "__compressor_finish";
    let slot_id = arg_i64(args, 0, NAME)? as u64;

    let guard = LOADED_COMPRESSION.lock();
    let lc = guard.as_ref().ok_or_else(|| anyhow::anyhow!("{}: z42-compression not loaded", NAME))?;

    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let rc = unsafe {
        (lc.compressor_finish)(slot_id, &mut out_ptr, &mut out_len)
    };
    if rc != 0 {
        bail!("{}: {} (rc={})", NAME, last_error_string(), rc);
    }
    Ok(bytes_to_value(ctx, take_owned_buffer(lc.free, out_ptr, out_len)))
}

fn wrap_compressor_dispose(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    const NAME: &str = "__compressor_dispose";
    let slot_id = arg_i64(args, 0, NAME)? as u64;

    let guard = LOADED_COMPRESSION.lock();
    if let Some(lc) = guard.as_ref() {
        let _ = unsafe { (lc.compressor_dispose)(slot_id) };
    }
    Ok(Value::Null)
}
