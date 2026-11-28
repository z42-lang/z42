//! Host configuration: C ABI struct layout + validation.
//!
//! Mirrors `Z42HostConfig` / `Z42WriteSink` / `Z42ExecMode` from
//! `src/runtime/include/z42_host.h`. Spec: docs/internals/src/runtime/embedding.md §4.2.

use std::ffi::CString;
use std::os::raw::{c_char, c_void};
use std::sync::Arc;

use super::{FileKind, FileResolver, session::SessionConfig};

/// Wire-compatible version of `Z42_HOST_ABI_VERSION` from the C header.
pub const Z42_HOST_ABI_VERSION: u32 = 3;

/// `Z42WriteSink` callback. `Option` so a null function pointer maps to
/// "use the configured default" (real stdout / accumulating sink / etc.).
pub type Z42WriteSink =
    Option<unsafe extern "C" fn(bytes: *const c_char, length: usize, user_data: *mut c_void)>;

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

/// The caller supplies readable, unchanging bytes for the duration of this copy.
pub(super) unsafe fn copy_resolver_bytes(bytes: *const u8, length: usize) -> Option<Vec<u8>> {
    if bytes.is_null() || length == 0 { return None; }
    let mut copy = Vec::new();
    copy.try_reserve_exact(length).ok()?;
    // SAFETY: the caller guarantees readability, and reservation checked the length.
    copy.extend_from_slice(unsafe { std::slice::from_raw_parts(bytes, length) });
    Some(copy)
}

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
/// The required fields end at `search_paths`. Optional callbacks are read
/// only when both the function and its context are present.
#[repr(C)]
pub struct Z42HostConfig {
    pub abi_version: u32,
    pub struct_size: u32,

    pub exec_mode: i32, // Z42ExecMode wire form (avoid Rust enum UB on bad input)
    pub stdout_sink: Z42WriteSink,
    pub stderr_sink: Z42WriteSink,
    pub sink_user_data: *mut c_void,

    pub search_paths: *const *const c_char,

    pub file_resolver: Z42FileResolverFn,
    pub file_resolver_user_data: *mut c_void,
}

/// Size of the required prefix, excluding the optional file resolver.
pub(super) const MIN_CONFIG_SIZE: usize = std::mem::offset_of!(Z42HostConfig, file_resolver);

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
        search_paths,
        zpkg_resolver: None,
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
