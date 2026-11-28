//! Host configuration: C ABI struct layout + validation.
//!
//! Mirrors `Z42HostConfig` / `Z42WriteSink` / `Z42ExecMode` from
//! `src/runtime/include/z42_host.h`. Spec: docs/internals/src/runtime/embedding.md §4.2.

use std::ffi::CString;
use std::os::raw::{c_char, c_void};
use std::sync::Arc;

use super::{FileKind, FileResolver, ZpkgResolver, session::SessionConfig};

/// Wire-compatible version of `Z42_HOST_ABI_VERSION` from the C header.
pub const Z42_HOST_ABI_VERSION: u32 = 2;

/// `Z42WriteSink` callback. `Option` so a null function pointer maps to
/// "use the configured default" (real stdout / accumulating sink / etc.).
pub type Z42WriteSink =
    Option<unsafe extern "C" fn(bytes: *const c_char, length: usize, user_data: *mut c_void)>;

/// `Z42ZpkgResolverFn` C ABI signature: hit returns non-zero, miss returns 0.
/// A hit's bytes must remain readable and unchanged while the runtime copies
/// them after the callback returns.
pub type Z42ZpkgResolverFn = Option<ZpkgResolverCallback>;

/// Synchronous resolver for bytes borrowed inside the callback.
pub type Z42ZpkgResolverWriteFn = Option<ZpkgResolverWriteCallback>;
pub type Z42ZpkgByteSink = unsafe extern "C" fn(*const u8, usize, *mut c_void) -> i32;

type ZpkgResolverCallback = unsafe extern "C" fn(
    namespace_name: *const c_char,
    out_bytes: *mut *const u8,
    out_length: *mut usize,
    user_data: *mut c_void,
) -> i32;

type ZpkgResolverWriteCallback = unsafe extern "C" fn(
    namespace_name: *const c_char,
    write: Z42ZpkgByteSink,
    write_data: *mut c_void,
    user_data: *mut c_void,
) -> i32;

#[repr(C)]
pub struct Z42FileRequest {
    pub struct_size: u32,
    pub kind: u32,
    pub file_name: *const c_char,
}

pub type Z42FileResolverFn = Option<FileResolverCallback>;

type FileResolverCallback = unsafe extern "C" fn(
    request: *const Z42FileRequest,
    writer: *mut Z42FileWriter,
    user_data: *mut c_void,
) -> i32;

/// Stack-owned byte destination valid for one synchronous callback.
pub struct Z42FileWriter {
    bytes: Option<Vec<u8>>,
    failed: bool,
}

impl Z42FileWriter {
    pub(super) fn submit(&mut self, bytes: Option<Vec<u8>>) -> bool {
        if self.failed || self.bytes.is_some() || bytes.is_none() {
            self.failed = true;
            return false;
        }
        self.bytes = bytes;
        true
    }
}

struct CFileResolver {
    callback: FileResolverCallback,
    user_data: usize,
}

impl FileResolver for CFileResolver {
    fn resolve(&self, name: &str, kind: FileKind) -> anyhow::Result<Option<Vec<u8>>> {
        let name = CString::new(name)?;
        let request = Z42FileRequest {
            struct_size: size_of::<Z42FileRequest>() as u32,
            kind: kind as u32,
            file_name: name.as_ptr(),
        };
        let mut writer = Z42FileWriter { bytes: None, failed: false };
        // SAFETY: request and writer are valid through the synchronous callback.
        let result = unsafe { (self.callback)(&request, &mut writer, self.user_data as *mut c_void) };
        match (result, writer.bytes, writer.failed) {
            (1, Some(bytes), false) => Ok(Some(bytes)),
            (0, None, false) => Ok(None),
            (-1, _, _) => anyhow::bail!("host file resolver failed"),
            _ => anyhow::bail!("host file resolver returned an invalid status or byte submission"),
        }
    }
}

enum ResolverCallback {
    Borrowed(ZpkgResolverCallback),
    Write(ZpkgResolverWriteCallback),
}

/// Keep user data opaque so the adapter can be shared. The C host provides
/// a thread-safe callback and keeps its data valid for the resolver's lifetime.
struct CHookResolver {
    callback: ResolverCallback,
    user_data: usize,
}

impl ZpkgResolver for CHookResolver {
    fn resolve(&self, namespace: &str) -> Option<Vec<u8>> {
        let cname = CString::new(namespace).ok()?;
        let user_data = self.user_data as *mut c_void;
        match self.callback {
            ResolverCallback::Borrowed(callback) => {
                let mut bytes = std::ptr::null();
                let mut length = 0;
                // SAFETY: the host provides a valid callback and user data.
                let hit = unsafe { callback(cname.as_ptr(), &mut bytes, &mut length, user_data) };
                if hit == 0 { return None; }
                // SAFETY: the host keeps the bytes valid through this copy.
                unsafe { copy_resolver_bytes(bytes, length) }
            }
            ResolverCallback::Write(callback) => {
                let mut written: Option<Vec<u8>> = None;
                let write_data = std::ptr::from_mut(&mut written).cast();
                // SAFETY: the host uses the sink synchronously on this thread
                // and keeps its arguments valid through the callback.
                let hit = unsafe { callback(cname.as_ptr(), write_zpkg_bytes, write_data, user_data) };
                if hit != 0 { written } else { None }
            }
        }
    }
}

/// The caller supplies readable, unchanging bytes for the duration of this copy.
pub(super) unsafe fn copy_resolver_bytes(bytes: *const u8, length: usize) -> Option<Vec<u8>> {
    if bytes.is_null() || length == 0 { return None; }
    let mut copy = Vec::new();
    copy.try_reserve_exact(length).ok()?;
    // SAFETY: the caller guarantees readability, and reservation checked the length.
    copy.extend_from_slice(unsafe { std::slice::from_raw_parts(bytes, length) });
    Some(copy)
}

unsafe extern "C" fn write_zpkg_bytes(bytes: *const u8, length: usize, write_data: *mut c_void) -> i32 {
    // SAFETY: the resolver contract requires the original opaque pointer and
    // synchronous calls on the invoking thread.
    let written = unsafe { &mut *write_data.cast::<Option<Vec<u8>>>() };
    if written.is_some() { return 0; }
    // SAFETY: the resolver supplies readable bytes for the duration of this call.
    *written = unsafe { copy_resolver_bytes(bytes, length) };
    i32::from(written.is_some())
}

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
/// least [`MIN_CONFIG_SIZE`] for the original prefix.
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

    // add-zpkg-resolver-hook (append-only).
    pub zpkg_resolver: Z42ZpkgResolverFn,
    pub zpkg_resolver_user_data: *mut c_void,
    pub zpkg_resolver_write: Z42ZpkgResolverWriteFn,
    pub file_resolver: Z42FileResolverFn,
    pub file_resolver_user_data: *mut c_void,
}

/// The size of the original v2 prefix, through the last pointer-sized field.
pub const MIN_CONFIG_SIZE: usize = std::mem::offset_of!(Z42HostConfig, zpkg_resolver_user_data)
    + std::mem::size_of::<*mut c_void>();
const RESOLVER_WRITE_END: usize = std::mem::offset_of!(Z42HostConfig, zpkg_resolver_write)
    + std::mem::size_of::<Z42ZpkgResolverWriteFn>();

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
/// configuration prefix and the readable fields covered by its declared size,
/// with a valid `search_paths` array (NULL-terminated, or null).
pub(super) unsafe fn validate(
    cfg: *const Z42HostConfig,
) -> Result<(SessionConfig, crate::corelib::io::HostSinks), ConfigError> {
    if cfg.is_null() {
        return Err(ConfigError::NullConfig);
    }
    // Read fields through the raw pointer. An older caller may provide only
    // the original prefix, which cannot be borrowed as the full current type.
    use std::ptr::addr_of;
    let abi_version = unsafe { addr_of!((*cfg).abi_version).read() };
    if abi_version != Z42_HOST_ABI_VERSION {
        return Err(ConfigError::AbiVersionMismatch {
            expected: Z42_HOST_ABI_VERSION,
            got: abi_version,
        });
    }
    let struct_size = unsafe { addr_of!((*cfg).struct_size).read() };
    if (struct_size as usize) < MIN_CONFIG_SIZE {
        return Err(ConfigError::StructTooSmall { min: MIN_CONFIG_SIZE, got: struct_size });
    }
    let raw = unsafe { addr_of!((*cfg).exec_mode).read() };
    let exec_mode = Z42ExecMode::from_raw(raw)
        .ok_or(ConfigError::UnknownExecMode { raw })?;

    check_feature_available(exec_mode)?;

    let search_paths = unsafe { collect_search_paths(addr_of!((*cfg).search_paths).read()) }?;
    let resolver_write = if struct_size as usize >= RESOLVER_WRITE_END {
        unsafe { addr_of!((*cfg).zpkg_resolver_write).read() }
    } else {
        None
    };
    let resolver = resolver_write.map(ResolverCallback::Write)
        .or_else(|| unsafe { addr_of!((*cfg).zpkg_resolver).read() }.map(ResolverCallback::Borrowed));
    let zpkg_resolver = resolver.map(|callback| {
        Arc::new(CHookResolver {
            callback,
            user_data: unsafe { addr_of!((*cfg).zpkg_resolver_user_data).read() } as usize,
        }) as Arc<dyn ZpkgResolver>
    });

    let file_resolver_end = std::mem::offset_of!(Z42HostConfig, file_resolver_user_data)
        + size_of::<*mut c_void>();
    let file_resolver = if struct_size as usize >= file_resolver_end {
        unsafe { addr_of!((*cfg).file_resolver).read() }.map(|callback| {
            Arc::new(CFileResolver {
                callback,
                user_data: unsafe { addr_of!((*cfg).file_resolver_user_data).read() } as usize,
            }) as Arc<dyn FileResolver>
        })
    } else {
        None
    };
    let resolved = SessionConfig {
        exec_mode,
        heap_initial_bytes: unsafe { addr_of!((*cfg).heap_initial_bytes).read() },
        heap_max_bytes: unsafe { addr_of!((*cfg).heap_max_bytes).read() },
        search_paths,
        zpkg_resolver,
        file_resolver,
        vfs: crate::pal::fs::default_vfs(),
    };
    let sink_user_data = unsafe { addr_of!((*cfg).sink_user_data).read() };
    let sinks = crate::corelib::io::HostSinks {
        stdout: output_sink(unsafe { addr_of!((*cfg).stdout_sink).read() }, sink_user_data),
        stderr: output_sink(unsafe { addr_of!((*cfg).stderr_sink).read() }, sink_user_data),
    };
    Ok((resolved, sinks))
}

pub(super) fn check_feature_available(mode: Z42ExecMode) -> Result<(), ConfigError> {
    match mode {
        Z42ExecMode::Default | Z42ExecMode::Interp => Ok(()),
        Z42ExecMode::Jit => if cfg!(all(feature = "jit", z42_jit_supported)) { Ok(()) } else { Err(ConfigError::FeatureOff { mode }) },
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


pub(super) fn output_sink(
    callback: Z42WriteSink, user_data: *mut c_void,
) -> Option<crate::corelib::io::OutputSink> {
    use crate::corelib::io::{HostSink, OutputSink};
    callback.map(|callback| OutputSink::C(HostSink { callback, user_data }))
}
