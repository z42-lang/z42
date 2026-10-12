//! Host configuration: C ABI struct layout + validation.
//!
//! Mirrors `Z42HostConfig` / `Z42WriteSink` / `Z42ExecMode` from
//! `src/runtime/include/z42_host.h`. Spec: docs/internals/src/runtime/embedding.md §4.2.

use std::os::raw::{c_char, c_void};
use std::sync::Arc;

use super::resolver::ZpkgResolver;

/// Wire-compatible version of `Z42_HOST_ABI_VERSION` from the C header.
pub const Z42_HOST_ABI_VERSION: u32 = 2;

/// `Z42WriteSink` callback. `Option` so a null function pointer maps to
/// "use the configured default" (real stdout / accumulating sink / etc.).
pub type Z42WriteSink =
    Option<unsafe extern "C" fn(bytes: *const c_char, length: usize, user_data: *mut c_void)>;

/// `Z42ZpkgResolverFn` C ABI signature: hit returns non-zero, miss returns 0.
/// Bytes need only stay valid until the callback returns. Spec:
/// `docs/spec/archive/2026-05-12-add-zpkg-resolver-hook/`.
pub type Z42ZpkgResolverFn = Option<
    unsafe extern "C" fn(
        namespace_name: *const c_char,
        out_bytes: *mut *const u8,
        out_length: *mut usize,
        user_data: *mut c_void,
    ) -> i32,
>;

/// Visitor for [`super::z42_zpkg_read_namespaces`]: invoked once per
/// resolution key a zpkg answers to — its package name and each namespace
/// it declares in `NSPC`. `ns` is a UTF-8 byte range of `len` bytes (NOT
/// NUL-terminated); valid only for the duration of the call, so the host
/// must copy it before returning.
pub type Z42NamespaceVisitor =
    Option<unsafe extern "C" fn(ns: *const c_char, len: usize, user_data: *mut c_void)>;

/// `Z42ExecMode` — must stay in sync with the C enum.
#[repr(i32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Z42ExecMode {
    Default = 0,
    Interp = 1,
    Jit = 2,
    Aot = 3,
}

impl Z42ExecMode {
    /// Convert from a raw C enum value (`i32`). Unknown values → `None`.
    pub fn from_raw(raw: i32) -> Option<Self> {
        match raw {
            0 => Some(Self::Default),
            1 => Some(Self::Interp),
            2 => Some(Self::Jit),
            3 => Some(Self::Aot),
            _ => None,
        }
    }
}

/// C ABI layout of `Z42HostConfig`. `#[repr(C)]` keeps field order +
/// alignment compatible with the C struct in `z42_host.h`.
///
/// `struct_size` is `sizeof(Z42HostConfig)` as the caller compiled it.
/// Fields appended after v2 are read only when `struct_size` covers them,
/// so a caller built against an older v2 header is never read past the end
/// of its struct. Every field below exists since v2, so v2 requires at
/// least [`MIN_CONFIG_SIZE`].
#[repr(C)]
pub struct Z42HostConfig {
    pub abi_version: u32,
    pub struct_size: u32,

    pub exec_mode: i32, // Z42ExecMode wire form (avoid Rust enum UB on bad input)
    pub heap_initial_bytes: usize,
    pub heap_max_bytes: usize,

    pub stdout_sink: Z42WriteSink,
    pub stderr_sink: Z42WriteSink,
    pub sink_user_data: *mut c_void,

    pub search_paths: *const *const c_char,

    // 2026-05-11 add-zpkg-resolver-hook (append-only).
    pub zpkg_resolver: Z42ZpkgResolverFn,
    pub zpkg_resolver_user_data: *mut c_void,
}

/// Smallest `struct_size` a v2 caller can pass: the v2 layout in full.
/// Stays fixed when fields are appended later.
pub const MIN_CONFIG_SIZE: usize = std::mem::size_of::<Z42HostConfig>();

/// Rust-side resolved configuration after validation. The thread-safety
/// requirement comes from holding this inside the global `RwLock` —
/// `*mut c_void` is not auto-`Send`, so we wrap the whole config in
/// `unsafe impl Send + Sync` at the storage site (see `state.rs`).
pub struct ResolvedConfig {
    pub exec_mode: Z42ExecMode,
    pub heap_initial_bytes: usize,
    pub heap_max_bytes: usize,
    pub stdout_sink: Z42WriteSink,
    pub stderr_sink: Z42WriteSink,
    pub sink_user_data: usize, // raw pointer kept as usize so this struct is Send
    pub search_paths: Vec<String>,
    /// `None` when neither the C config nor Tier 2 supplied a resolver.
    /// Otherwise either a `CHookResolver` (built from the C pair) or a
    /// Rust trait object handed to us by Tier 2 via
    /// [`super::install_zpkg_resolver`].
    pub zpkg_resolver: Option<Arc<dyn ZpkgResolver>>,
}

impl std::fmt::Debug for ResolvedConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedConfig")
            .field("exec_mode", &self.exec_mode)
            .field("heap_initial_bytes", &self.heap_initial_bytes)
            .field("heap_max_bytes", &self.heap_max_bytes)
            .field("stdout_sink", &self.stdout_sink.is_some())
            .field("stderr_sink", &self.stderr_sink.is_some())
            .field("sink_user_data", &self.sink_user_data)
            .field("search_paths", &self.search_paths)
            .field("zpkg_resolver", &self.zpkg_resolver.is_some())
            .finish()
    }
}

/// Validation outcome. Caller maps `Err` to a `Z42HostStatus`.
#[derive(Debug)]
pub enum ConfigError {
    NullConfig,
    AbiVersionMismatch { expected: u32, got: u32 },
    StructTooSmall { min: usize, got: u32 },
    UnknownExecMode { raw: i32 },
    FeatureOff { mode: Z42ExecMode },
    BadSearchPath { reason: &'static str },
}

/// Validate a raw `*const Z42HostConfig` from the FFI boundary.
///
/// # Safety
/// Caller must ensure that, if `cfg` is non-null, it points to a valid
/// `Z42HostConfig` with a valid `search_paths` array (NULL-terminated, or
/// null pointer if no paths are configured).
pub(crate) unsafe fn validate(cfg: *const Z42HostConfig) -> Result<ResolvedConfig, ConfigError> {
    if cfg.is_null() {
        return Err(ConfigError::NullConfig);
    }
    // Only the two leading u32s are known to exist until both are checked —
    // read them through the raw pointer before forming a reference to the
    // whole struct.
    let abi_version = unsafe { std::ptr::addr_of!((*cfg).abi_version).read() };
    if abi_version != Z42_HOST_ABI_VERSION {
        return Err(ConfigError::AbiVersionMismatch {
            expected: Z42_HOST_ABI_VERSION,
            got: abi_version,
        });
    }
    let struct_size = unsafe { std::ptr::addr_of!((*cfg).struct_size).read() };
    if (struct_size as usize) < MIN_CONFIG_SIZE {
        return Err(ConfigError::StructTooSmall { min: MIN_CONFIG_SIZE, got: struct_size });
    }
    let cfg = unsafe { &*cfg };

    let exec_mode = Z42ExecMode::from_raw(cfg.exec_mode)
        .ok_or(ConfigError::UnknownExecMode { raw: cfg.exec_mode })?;

    check_feature_available(exec_mode)?;

    let search_paths = unsafe { collect_search_paths(cfg.search_paths) }?;

    let zpkg_resolver =
        super::resolver::arc_from_c_pair(cfg.zpkg_resolver, cfg.zpkg_resolver_user_data);

    Ok(ResolvedConfig {
        exec_mode,
        heap_initial_bytes: cfg.heap_initial_bytes,
        heap_max_bytes: cfg.heap_max_bytes,
        stdout_sink: cfg.stdout_sink,
        stderr_sink: cfg.stderr_sink,
        sink_user_data: cfg.sink_user_data as usize,
        search_paths,
        zpkg_resolver,
    })
}

fn check_feature_available(mode: Z42ExecMode) -> Result<(), ConfigError> {
    match mode {
        Z42ExecMode::Default | Z42ExecMode::Interp => Ok(()),
        Z42ExecMode::Jit => {
            #[cfg(feature = "jit")]
            {
                Ok(())
            }
            #[cfg(not(feature = "jit"))]
            {
                Err(ConfigError::FeatureOff { mode })
            }
        }
        Z42ExecMode::Aot => {
            #[cfg(feature = "aot")]
            {
                Ok(())
            }
            #[cfg(not(feature = "aot"))]
            {
                Err(ConfigError::FeatureOff { mode })
            }
        }
    }
}

unsafe fn collect_search_paths(
    raw: *const *const c_char,
) -> Result<Vec<String>, ConfigError> {
    if raw.is_null() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut cursor = raw;
    // Bound the loop so a malformed (un-NUL-terminated) array can't iterate
    // forever. 4096 entries is far more than any real host would supply.
    for _ in 0..4096 {
        let entry = unsafe { *cursor };
        if entry.is_null() {
            return Ok(out);
        }
        let cstr = unsafe { std::ffi::CStr::from_ptr(entry) };
        let s = cstr
            .to_str()
            .map_err(|_| ConfigError::BadSearchPath {
                reason: "search path is not valid UTF-8",
            })?;
        out.push(s.to_owned());
        cursor = unsafe { cursor.add(1) };
    }
    Err(ConfigError::BadSearchPath {
        reason: "search_paths array exceeded 4096 entries (missing NULL terminator?)",
    })
}

