//! Stdlib native extension loader (add-z42-compression).
//!
//! Opens `lib<basename>.{so,dylib,dll}` from the SDK native search path the
//! first time one of its builtins runs, resolves a known set of `extern "C"`
//! entries by name, and keeps the fn ptrs in a process-static table. The
//! builtins themselves (`__<entry>`, referenced by z42 facades via
//! `[Native(lib="<basename>", entry="...")]`) are ordinary typed builtins in
//! the main `BUILTINS` table ([`typed`]); their bodies call through that table.
//!
//! # Difference vs Tier 1 (`tier1/loader.rs`)
//!
//! - **Tier 1** (`tier1/loader.rs`): general-purpose user-supplied native types
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
//! Static providers and dynamic libraries use the same lazy symbol table.
//! The final WASM facade enables `bundled-compression`; custom static hosts
//! register their linked provider through the host API before creating a VM.

use std::sync::OnceLock;

use anyhow::Result;

use crate::exec::call::builtin_abi::BuiltinError;
use crate::context::VmContext;

// ── Entry point ──────────────────────────────────────────────────────────────

/// Load the known library needed by a typed builtin wrapper, at most once per VM.
/// Returns whether this call opened it; resolved symbols live in the process symbol table.
pub fn ensure_lib_for(ctx: &VmContext, name: &str) -> Result<bool> {
    let Some(library) = KNOWN_EXT_LIBS.iter()
        .find(|(_, names)| names.contains(&name)).map(|(library, _)| *library)
    else { return Ok(false) };
    if !super::state::claim_extension_load(ctx, library)? { return Ok(false); }
    #[cfg(feature = "bundled-compression")]
    if library == "compression" && linked::find("z42_compression").is_none() {
        unsafe { register_static_library("z42_compression", z42_compression::z42_compression_get_symbol)?; }
    }
    load_by_name(library)
}

/// The stdlib ext libs this loader knows how to wire up, with the builtins
/// each one provides — the map from a missing builtin name to the library to
/// open. Only these names are ever loaded from the search paths: the
/// cargo-target directory (one of them, see [`native_search_paths`]) holds
/// every cdylib of the workspace, e.g. `libz42_repl`, which has its own loader.
/// The builtin names this loader routes to the compression library — typed
/// builtins in the main table carry the wrappers; this list only names what
/// `ensure_lib_for` may open `libz42_compression` for.
const COMPRESSION_NAMES: &[&str] = &[
    "__deflate_compress", "__deflate_decompress",
    "__zstd_compress", "__zstd_decompress",
    "__brotli_compress", "__brotli_decompress",
    "__lz4_compress", "__lz4_decompress",
    "__compressor_begin", "__compressor_feed", "__compressor_finish", "__compressor_dispose",
];

const KNOWN_EXT_LIBS: &[(&str, &[&str])] = &[
    ("compression", COMPRESSION_NAMES),
];

/// Open `lib<z42_{name}>.{so,dylib,dll}` from the first native search path
/// that has it and bind its entry points. A named probe per directory — no
/// directory listing. Logs and returns `false` when the library is missing
/// or fails to load; the typed wrapper reports the unavailable native entry.
fn load_by_name(name: &str) -> Result<bool> {
    let stem = format!("z42_{name}");
    if let Some(resolve) = linked::find(&stem) {
        unsafe { compression_symbols(SymbolSource::Static(resolve))?; }
        return Ok(true);
    }
    #[cfg(any(feature = "native-interop", feature = "component-loading"))]
    for path in crate::pal::dynlib::candidate_paths(&stem, native_search_paths()) {
        if !path.is_file() && !crate::pal::dynlib::is_loader_name(&path) { continue; }
        let result = unsafe { crate::pal::dynlib::Library::open_component(&path) }
            .map_err(anyhow::Error::from)
            .and_then(|library| unsafe { compression_symbols(SymbolSource::Dynamic(library)) });
        return match result {
            Ok(()) => Ok(true),
            Err(error) => { tracing::warn!("ext: failed to load {}: {error:#}", path.display()); Ok(false) }
        };
    }
    Ok(false)
}

pub(crate) use crate::components::native_paths as native_search_paths;
mod linked;
pub use linked::{register_static_library, NativeSymbolResolver};

enum SymbolSource {
    Static(NativeSymbolResolver),
    #[cfg(any(feature = "native-interop", feature = "component-loading"))]
    Dynamic(std::sync::Arc<crate::pal::dynlib::Library>),
}
impl SymbolSource {
    unsafe fn symbol<T: Copy>(&self, name: &str) -> Result<T> {
        match self {
            #[cfg(any(feature = "native-interop", feature = "component-loading"))]
            Self::Dynamic(library) => unsafe { library.symbol(name).map_err(Into::into) },
            Self::Static(resolve) => {
                let name = std::ffi::CString::new(name)?;
                let pointer = unsafe { resolve(name.as_ptr()) };
                anyhow::ensure!(!pointer.is_null(), "missing static native symbol `{}`", name.to_string_lossy());
                anyhow::ensure!(std::mem::size_of::<T>() == std::mem::size_of_val(&pointer), "invalid native function-pointer size");
                // These private callers request the exact C function types checked
                // by the library's ABI handshake before any other symbol is used.
                Ok(unsafe { std::mem::transmute_copy(&pointer) })
            }
        }
    }
}

// ── compression symbol table (dlopen path) ───────────────────────────────────

/// The `z42_compression` C-ABI contract version this loader was written against.
/// Must equal `z42_compression::Z42_COMPRESSION_ABI_VERSION`; bump both together
/// on any `z42_compression_*` signature or error-code change. The dlopen path
/// checks it via [`CAbiVersionFn`] before binding any other symbol — link-time
/// version-locking (the doc below) only covers the bundled / packaged case, not
/// a stale library dlopen'd from the executable's own directory.
const EXPECTED_COMPRESSION_ABI: u32 = 1;

/// `z42_compression_abi_version() -> u32` — resolved first on the dlopen path.
type CAbiVersionFn = unsafe extern "C" fn() -> u32;

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
// add-z42-compression-brotli : same shape as zstd.
type CBrotliCompressFn = unsafe extern "C" fn(
    *const u8, usize, i32,
    *mut *mut u8, *mut usize,
) -> i32;
type CBrotliDecompressFn = unsafe extern "C" fn(
    *const u8, usize,
    *mut *mut u8, *mut usize,
) -> i32;
// add-z42-compression-lz4 : same shape as brotli.
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
/// up without re-querying the library on every call.
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
    _source: SymbolSource,
}

/// Set once per process by the first VM that loads the library. Read without a
/// lock: the wrappers call into the library and then, on failure, read
/// `last_error` (thread-local on the library side). The former
/// `Mutex<Option<_>>` was held across that call and re-locked inside
/// `last_error_string`, so every error path deadlocked, and every compression
/// call in the process ran one at a time.
static LOADED_COMPRESSION: OnceLock<LoadedCompression> = OnceLock::new();

/// The loaded fn ptr table, or an error naming the builtin that needed it.
/// Tests: whether the process-global compression fn-ptr table is filled.
#[cfg(test)]
pub(crate) fn compression_loaded_for_test() -> bool {
    LOADED_COMPRESSION.get().is_some()
}

fn loaded(ctx: &VmContext, name: &str) -> Result<&'static LoadedCompression> {
    if LOADED_COMPRESSION.get().is_none() {
        // typed 主表命中后不再经 ext 查找,装载由这里按名触发(dlopen 路径;
        // linked providers resolve through the same lazy path)。
        ensure_lib_for(ctx, name)?;
    }
    LOADED_COMPRESSION.get().ok_or_else(|| anyhow::anyhow!("{}: compression component unavailable (not linked or its dynamic library is missing)", name))
}

unsafe fn compression_symbols(
    lib: SymbolSource,
) -> Result<()> {
    // ABI handshake — resolve the version symbol FIRST and refuse to bind the
    // rest unless it matches what these signatures were written against. The
    // loader searches the executable's own directory (`native_search_paths`), so
    // a stale `libz42_compression` sitting there has the same symbol names but a
    // possibly different layout; calling it through the current `C*Fn` types
    // would be UB. A missing symbol (pre-versioning build) fails the `?` the same
    // way — the caller warns and skips the library rather than guess.
    let abi_version: CAbiVersionFn = lib.symbol("z42_compression_abi_version")?;
    let got = abi_version();
    if got != EXPECTED_COMPRESSION_ABI {
        anyhow::bail!(
            "libz42_compression ABI version mismatch: library reports {got}, z42vm expects \
             {EXPECTED_COMPRESSION_ABI} — skipping it (rebuild the native library from the \
             matching source tree)"
        );
    }

    // A later VM in the same process (or a second copy of the library) keeps
    // using the table the first load installed; this handle is just dropped.
    if LOADED_COMPRESSION.get().is_some() {
        return Ok(());
    }

    // Copy the fn ptrs out and move the library into the table next to them
    // so the symbols stay resident.
    let deflate_compress: CDeflateCompressFn = lib.symbol("z42_compression_deflate_compress")?;
    let deflate_decompress: CDeflateDecompressFn = lib.symbol("z42_compression_deflate_decompress")?;
    let zstd_compress: CZstdCompressFn = lib.symbol("z42_compression_zstd_compress")?;
    let zstd_decompress: CZstdDecompressFn = lib.symbol("z42_compression_zstd_decompress")?;
    let brotli_compress: CBrotliCompressFn = lib.symbol("z42_compression_brotli_compress")?;
    let brotli_decompress: CBrotliDecompressFn = lib.symbol("z42_compression_brotli_decompress")?;
    let lz4_compress: CLz4CompressFn = lib.symbol("z42_compression_lz4_compress")?;
    let lz4_decompress: CLz4DecompressFn = lib.symbol("z42_compression_lz4_decompress")?;
    let compressor_begin: CCompressorBeginFn = lib.symbol("z42_compression_compressor_begin")?;
    let compressor_feed: CCompressorFeedFn = lib.symbol("z42_compression_compressor_feed")?;
    let compressor_finish: CCompressorFinishFn = lib.symbol("z42_compression_compressor_finish")?;
    let compressor_dispose: CCompressorDisposeFn = lib.symbol("z42_compression_compressor_dispose")?;
    let free: CFreeFn = lib.symbol("z42_compression_free")?;
    let last_error: CLastErrorFn = lib.symbol("z42_compression_last_error")?;

    // Losing a race with another thread's load is fine: the winner's table is
    // equivalent, and this handle is dropped with the rejected value.
    let _ = LOADED_COMPRESSION.set(LoadedCompression {
        deflate_compress, deflate_decompress,
        zstd_compress, zstd_decompress,
        brotli_compress, brotli_decompress,
        lz4_compress, lz4_decompress,
        compressor_begin, compressor_feed, compressor_finish, compressor_dispose,
        free, last_error,
        _source: lib,
    });

    Ok(())
}

// ── Wrapper helpers ──────────────────────────────────────────────────────────

fn take_owned_buffer(free: CFreeFn, out_ptr: *mut u8, out_len: usize) -> Vec<u8> {
    if out_ptr.is_null() || out_len == 0 { return Vec::new(); }
    // unify-gc-heap PR-3 fix : **copy** the cdylib's output into a Rust-owned Vec
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
    if let Some(lc) = LOADED_COMPRESSION.get() {
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

// ── Builtin bodies (called by the typed wrappers in `typed`) ──────────

fn deflate_compress(ctx: &VmContext, input: Vec<u8>, level: i64, mode: i64) -> Result<Vec<u8>, BuiltinError> {
    const NAME: &str = "__deflate_compress";
    let input = input;
    let level = level as i32;
    let mode  = mode as i32;

    let lc = loaded(ctx, NAME).map_err(BuiltinError::Legacy)?;

    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let rc = unsafe {
        (lc.deflate_compress)(input.as_ptr(), input.len(),
                              level, mode,
                              &mut out_ptr, &mut out_len)
    };
    if rc != 0 {
        return Err(BuiltinError::Io(format!("{}: {} (rc={})", NAME, last_error_string(), rc)));
    }
    Ok(take_owned_buffer(lc.free, out_ptr, out_len))
}

fn deflate_decompress(ctx: &VmContext, input: Vec<u8>, mode: i64) -> Result<Vec<u8>, BuiltinError> {
    const NAME: &str = "__deflate_decompress";
    let input = input;
    let mode  = mode as i32;

    let lc = loaded(ctx, NAME).map_err(BuiltinError::Legacy)?;

    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let rc = unsafe {
        (lc.deflate_decompress)(input.as_ptr(), input.len(),
                                mode,
                                &mut out_ptr, &mut out_len)
    };
    if rc != 0 {
        return Err(BuiltinError::Io(format!("{}: {} (rc={})", NAME, last_error_string(), rc)));
    }
    Ok(take_owned_buffer(lc.free, out_ptr, out_len))
}

fn zstd_compress(ctx: &VmContext, input: Vec<u8>, level: i64) -> Result<Vec<u8>, BuiltinError> {
    const NAME: &str = "__zstd_compress";
    let input = input;
    let level = level as i32;

    let lc = loaded(ctx, NAME).map_err(BuiltinError::Legacy)?;

    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let rc = unsafe {
        (lc.zstd_compress)(input.as_ptr(), input.len(), level,
                           &mut out_ptr, &mut out_len)
    };
    if rc != 0 {
        return Err(BuiltinError::Io(format!("{}: {} (rc={})", NAME, last_error_string(), rc)));
    }
    Ok(take_owned_buffer(lc.free, out_ptr, out_len))
}

fn zstd_decompress(ctx: &VmContext, input: Vec<u8>) -> Result<Vec<u8>, BuiltinError> {
    const NAME: &str = "__zstd_decompress";
    let input = input;

    let lc = loaded(ctx, NAME).map_err(BuiltinError::Legacy)?;

    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let rc = unsafe {
        (lc.zstd_decompress)(input.as_ptr(), input.len(),
                             &mut out_ptr, &mut out_len)
    };
    if rc != 0 {
        return Err(BuiltinError::Io(format!("{}: {} (rc={})", NAME, last_error_string(), rc)));
    }
    Ok(take_owned_buffer(lc.free, out_ptr, out_len))
}

fn brotli_compress(ctx: &VmContext, input: Vec<u8>, level: i64) -> Result<Vec<u8>, BuiltinError> {
    const NAME: &str = "__brotli_compress";
    let input = input;
    let level = level as i32;

    let lc = loaded(ctx, NAME).map_err(BuiltinError::Legacy)?;

    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let rc = unsafe {
        (lc.brotli_compress)(input.as_ptr(), input.len(), level,
                             &mut out_ptr, &mut out_len)
    };
    if rc != 0 {
        return Err(BuiltinError::Io(format!("{}: {} (rc={})", NAME, last_error_string(), rc)));
    }
    Ok(take_owned_buffer(lc.free, out_ptr, out_len))
}

fn brotli_decompress(ctx: &VmContext, input: Vec<u8>) -> Result<Vec<u8>, BuiltinError> {
    const NAME: &str = "__brotli_decompress";
    let input = input;

    let lc = loaded(ctx, NAME).map_err(BuiltinError::Legacy)?;

    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let rc = unsafe {
        (lc.brotli_decompress)(input.as_ptr(), input.len(),
                               &mut out_ptr, &mut out_len)
    };
    if rc != 0 {
        return Err(BuiltinError::Io(format!("{}: {} (rc={})", NAME, last_error_string(), rc)));
    }
    Ok(take_owned_buffer(lc.free, out_ptr, out_len))
}

fn lz4_compress(ctx: &VmContext, input: Vec<u8>, level: i64) -> Result<Vec<u8>, BuiltinError> {
    const NAME: &str = "__lz4_compress";
    let input = input;
    let level = level as i32;

    let lc = loaded(ctx, NAME).map_err(BuiltinError::Legacy)?;

    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let rc = unsafe {
        (lc.lz4_compress)(input.as_ptr(), input.len(), level,
                          &mut out_ptr, &mut out_len)
    };
    if rc != 0 {
        return Err(BuiltinError::Io(format!("{}: {} (rc={})", NAME, last_error_string(), rc)));
    }
    Ok(take_owned_buffer(lc.free, out_ptr, out_len))
}

fn lz4_decompress(ctx: &VmContext, input: Vec<u8>) -> Result<Vec<u8>, BuiltinError> {
    const NAME: &str = "__lz4_decompress";
    let input = input;

    let lc = loaded(ctx, NAME).map_err(BuiltinError::Legacy)?;

    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let rc = unsafe {
        (lc.lz4_decompress)(input.as_ptr(), input.len(),
                            &mut out_ptr, &mut out_len)
    };
    if rc != 0 {
        return Err(BuiltinError::Io(format!("{}: {} (rc={})", NAME, last_error_string(), rc)));
    }
    Ok(take_owned_buffer(lc.free, out_ptr, out_len))
}

fn compressor_begin(ctx: &VmContext, algo: i64, level: i64, is_decompress: bool) -> Result<i64, BuiltinError> {
    const NAME: &str = "__compressor_begin";
    let algo  = algo as i32;
    let level = level as i32;

    let lc = loaded(ctx, NAME).map_err(BuiltinError::Legacy)?;

    let mut slot_id: u64 = 0;
    let rc = unsafe {
        (lc.compressor_begin)(algo, level, is_decompress as i32, &mut slot_id)
    };
    if rc != 0 {
        return Err(BuiltinError::Io(format!("{}: {} (rc={})", NAME, last_error_string(), rc)));
    }
    Ok(slot_id as i64)
}

fn compressor_feed(ctx: &VmContext, slot_id: i64, chunk: Vec<u8>) -> Result<Vec<u8>, BuiltinError> {
    const NAME: &str = "__compressor_feed";
    let slot_id = slot_id as u64;

    let lc = loaded(ctx, NAME).map_err(BuiltinError::Legacy)?;

    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let rc = unsafe {
        (lc.compressor_feed)(slot_id,
                             chunk.as_ptr(), chunk.len(),
                             &mut out_ptr, &mut out_len)
    };
    if rc != 0 {
        return Err(BuiltinError::Io(format!("{}: {} (rc={})", NAME, last_error_string(), rc)));
    }
    Ok(take_owned_buffer(lc.free, out_ptr, out_len))
}

fn compressor_finish(ctx: &VmContext, slot_id: i64) -> Result<Vec<u8>, BuiltinError> {
    const NAME: &str = "__compressor_finish";
    let slot_id = slot_id as u64;

    let lc = loaded(ctx, NAME).map_err(BuiltinError::Legacy)?;

    let mut out_ptr: *mut u8 = std::ptr::null_mut();
    let mut out_len: usize = 0;
    let rc = unsafe {
        (lc.compressor_finish)(slot_id, &mut out_ptr, &mut out_len)
    };
    if rc != 0 {
        return Err(BuiltinError::Io(format!("{}: {} (rc={})", NAME, last_error_string(), rc)));
    }
    Ok(take_owned_buffer(lc.free, out_ptr, out_len))
}

fn compressor_dispose(slot_id: i64) {
    let slot_id = slot_id as u64;
    if let Some(lc) = LOADED_COMPRESSION.get() {
        let _ = unsafe { (lc.compressor_dispose)(slot_id) };
    }
}

// The ABI handshake's two halves — this loader's `EXPECTED_COMPRESSION_ABI` and
// the crate's `Z42_COMPRESSION_ABI_VERSION` — must agree, or every dlopen of a
// correctly-built library would be wrongly rejected. Only checkable when the
// crate is linked in (bundled builds); the dlopen happy path is exercised by the
// runtime whenever `libz42_compression` is loaded.
#[cfg(all(test, feature = "bundled-compression"))]
mod abi_handshake_tests {
    #[test]
    fn expected_abi_matches_the_crate() {
        assert_eq!(
            super::EXPECTED_COMPRESSION_ABI,
            z42_compression::Z42_COMPRESSION_ABI_VERSION,
            "interop/ext EXPECTED_COMPRESSION_ABI is out of sync with \
             z42-compression's Z42_COMPRESSION_ABI_VERSION — bump both together",
        );
        assert_eq!(
            z42_compression::z42_compression_abi_version(),
            z42_compression::Z42_COMPRESSION_ABI_VERSION,
            "the exported C function must return the crate constant",
        );
    }
}

/// 压缩 builtin 迁 ABI v2(P1-8 批次 9):12 个 wrap 胶水改类型化形态进主表,
/// ext 表的 fn-ptr 注册退场(装载仍按需触发——`loaded` 未命中时 ensure)。
/// byte[] 参数拷出、返回打包 byte[];压缩失败(rc≠0)→ `IOException`。
#[z42_macros::builtins]
pub mod typed {
    use super::*;

    #[builtin(name = "__deflate_compress")]
    pub fn deflate_compress(ctx: &VmContext, input: Vec<u8>, level: i64, mode: i64) -> Result<Vec<u8>, BuiltinError> {
        super::deflate_compress(ctx, input, level, mode)
    }

    #[builtin(name = "__deflate_decompress")]
    pub fn deflate_decompress(ctx: &VmContext, input: Vec<u8>, mode: i64) -> Result<Vec<u8>, BuiltinError> {
        super::deflate_decompress(ctx, input, mode)
    }

    #[builtin(name = "__zstd_compress")]
    pub fn zstd_compress(ctx: &VmContext, input: Vec<u8>, level: i64) -> Result<Vec<u8>, BuiltinError> {
        super::zstd_compress(ctx, input, level)
    }

    #[builtin(name = "__zstd_decompress")]
    pub fn zstd_decompress(ctx: &VmContext, input: Vec<u8>) -> Result<Vec<u8>, BuiltinError> {
        super::zstd_decompress(ctx, input)
    }

    #[builtin(name = "__brotli_compress")]
    pub fn brotli_compress(ctx: &VmContext, input: Vec<u8>, level: i64) -> Result<Vec<u8>, BuiltinError> {
        super::brotli_compress(ctx, input, level)
    }

    #[builtin(name = "__brotli_decompress")]
    pub fn brotli_decompress(ctx: &VmContext, input: Vec<u8>) -> Result<Vec<u8>, BuiltinError> {
        super::brotli_decompress(ctx, input)
    }

    #[builtin(name = "__lz4_compress")]
    pub fn lz4_compress(ctx: &VmContext, input: Vec<u8>, level: i64) -> Result<Vec<u8>, BuiltinError> {
        super::lz4_compress(ctx, input, level)
    }

    #[builtin(name = "__lz4_decompress")]
    pub fn lz4_decompress(ctx: &VmContext, input: Vec<u8>) -> Result<Vec<u8>, BuiltinError> {
        super::lz4_decompress(ctx, input)
    }

    #[builtin(name = "__compressor_begin")]
    pub fn compressor_begin(ctx: &VmContext, algo: i64, level: i64, is_decompress: bool) -> Result<i64, BuiltinError> {
        super::compressor_begin(ctx, algo, level, is_decompress)
    }

    #[builtin(name = "__compressor_feed")]
    pub fn compressor_feed(ctx: &VmContext, slot_id: i64, chunk: Vec<u8>) -> Result<Vec<u8>, BuiltinError> {
        super::compressor_feed(ctx, slot_id, chunk)
    }

    #[builtin(name = "__compressor_finish")]
    pub fn compressor_finish(ctx: &VmContext, slot_id: i64) -> Result<Vec<u8>, BuiltinError> {
        super::compressor_finish(ctx, slot_id)
    }

    #[builtin(name = "__compressor_dispose")]
    pub fn compressor_dispose(slot_id: i64) { super::compressor_dispose(slot_id) }
}

#[cfg(test)]
mod ext_tests;
