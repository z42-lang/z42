//! Unit + integration tests for the embedding API.
//!
//! Spec: 变更 add-embedding-api/specs/embedding-host-api/spec.md
//!       (Requirement: 测试覆盖)
//!
//! All tests serialize on `HOST_TEST_LOCK` because the embedded VM is a
//! process singleton — concurrent tests would race on `session::HOST`.
//! The end-to-end `load_invoke_hello_world` test is gated on
//! `cfg(z42_have_embedding_hello)` so the suite still builds when
//! the z42c toolchain / stdlib isn't built yet (e.g. CI agents that
//! haven't run `xtask build stdlib`).

use std::ffi::CStr;
#[cfg_attr(not(z42_have_embedding_hello), allow(unused_imports))]
use std::ffi::CString;
use std::os::raw::{c_char, c_void};
use std::ptr;
use std::sync::{Mutex, OnceLock};

use super::abi_config::{Z42HostConfig, Z42_HOST_ABI_VERSION};
use super::{abi_config, session};
use super::error::Z42HostStatus;
use super::*;
use super::Z42Module;
use crate::vm::VmContext;
#[cfg(z42_have_embedding_hello)]
use super::Z42Entry;

/// Serializes all host-API tests in this binary so they don't trip over
/// each other's singleton state.
static HOST_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Acquire the test lock. Poisoning is ignored — a panicking test still
/// leaves us free to run the rest of the suite (singleton is reset by
/// each test's setup phase).
fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    match HOST_TEST_LOCK.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

fn current_host() -> *mut Z42Host {
    session::current().unwrap_or(1) as *mut Z42Host
}

/// Force the singleton back to "uninitialized" so a test starts clean.
fn reset_host() {
    // Repeated shutdowns are fine; second one returns NotInit which we
    // simply ignore.
    let _ = unsafe { z42_host_shutdown(current_host()) };
}

fn default_config() -> Z42HostConfig {
    Z42HostConfig {
        abi_version: Z42_HOST_ABI_VERSION,
        struct_size: std::mem::size_of::<Z42HostConfig>() as u32,
        exec_mode: abi_config::Z42ExecMode::Interp as i32,
        heap_initial_bytes: 0,
        heap_max_bytes: 0,
        stdout_sink: None,
        stderr_sink: None,
        sink_user_data: ptr::null_mut(),
        search_paths: ptr::null(),
        zpkg_resolver: None,
        zpkg_resolver_user_data: ptr::null_mut(),
        zpkg_resolver_write: None,
        file_resolver: None,
        file_resolver_user_data: ptr::null_mut(),
    }
}

#[test]
fn initialize_then_shutdown() {
    let _g = test_lock();
    reset_host();

    let cfg = default_config();
    let mut handle: *mut Z42Host = ptr::null_mut();
    let status = unsafe { z42_host_initialize(&cfg, &mut handle) };
    assert_eq!(status, Z42HostStatus::Ok);
    assert!(!handle.is_null());

    let status = unsafe { z42_host_shutdown(handle) };
    assert_eq!(status, Z42HostStatus::Ok);
}

#[test]
fn initialize_twice_returns_already_init() {
    let _g = test_lock();
    reset_host();

    let cfg = default_config();
    let mut h1: *mut Z42Host = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_initialize(&cfg, &mut h1) },
        Z42HostStatus::Ok
    );

    let mut h2: *mut Z42Host = ptr::null_mut();
    let status = unsafe { z42_host_initialize(&cfg, &mut h2) };
    assert_eq!(status, Z42HostStatus::AlreadyInit);
    assert!(h2.is_null(), "second initialize must not produce a handle");

    // Cleanup so subsequent tests start clean.
    assert_eq!(
        unsafe { z42_host_shutdown(h1) },
        Z42HostStatus::Ok
    );
}

fn assert_host_sinks_receive_output(capture: &StdoutCapture) {
    let ctx = VmContext::new();
    let _active = crate::corelib::io::HostSinkGuard::enter();
    let _ = crate::corelib::io::typed::println(&ctx, &Value::I64(17)).unwrap();
    let _ = crate::corelib::io::typed::eprintln(&ctx, &Value::I64(23)).unwrap();
    assert_eq!(&*capture.bytes.lock().unwrap(), b"17\n23\n");
}

#[test]
fn duplicate_initialize_preserves_existing_stdout_and_stderr() {
    let _g = test_lock();
    reset_host();
    let capture = StdoutCapture { bytes: Mutex::new(Vec::new()) };
    let rejected = StdoutCapture { bytes: Mutex::new(Vec::new()) };
    let mut cfg = default_config();
    cfg.stdout_sink = Some(host_stdout_sink_callback);
    cfg.stderr_sink = Some(host_stdout_sink_callback);
    cfg.sink_user_data = &capture as *const _ as *mut c_void;
    let mut host = ptr::null_mut();
    assert_eq!(unsafe { z42_host_initialize(&cfg, &mut host) }, Z42HostStatus::Ok);
    cfg.sink_user_data = &rejected as *const _ as *mut c_void;
    let mut duplicate = ptr::null_mut();
    assert_eq!(unsafe { z42_host_initialize(&cfg, &mut duplicate) }, Z42HostStatus::AlreadyInit);
    assert!(duplicate.is_null());
    assert_host_sinks_receive_output(&capture);
    assert!(rejected.bytes.lock().unwrap().is_empty());
    assert_eq!(unsafe { z42_host_shutdown(host) }, Z42HostStatus::Ok);
}

#[test]
fn failed_corelib_probe_preserves_existing_stdout_and_stderr() {
    let _g = test_lock();
    reset_host();
    let capture = StdoutCapture { bytes: Mutex::new(Vec::new()) };
    let mut cfg = default_config();
    cfg.stdout_sink = Some(host_stdout_sink_callback);
    cfg.stderr_sink = Some(host_stdout_sink_callback);
    cfg.sink_user_data = &capture as *const _ as *mut c_void;
    let mut host = ptr::null_mut();
    assert_eq!(unsafe { z42_host_initialize(&cfg, &mut host) }, Z42HostStatus::Ok);

    let dir = std::env::temp_dir().join(format!("z42-host-init-probe-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("z42.core.zpkg"), b"invalid package").unwrap();
    let path = CString::new(dir.to_str().unwrap()).unwrap();
    let paths = [path.as_ptr(), ptr::null()];
    let mut invalid = default_config();
    invalid.search_paths = paths.as_ptr();
    let mut failed = ptr::null_mut();
    let status = unsafe { z42_host_initialize(&invalid, &mut failed) };
    std::fs::remove_dir_all(dir).unwrap();
    assert_eq!(status, Z42HostStatus::BadConfig);
    assert!(failed.is_null());
    assert_host_sinks_receive_output(&capture);
    assert_eq!(unsafe { z42_host_shutdown(host) }, Z42HostStatus::Ok);
}

#[test]
fn shutdown_then_reinitialize() {
    let _g = test_lock();
    reset_host();

    let cfg = default_config();
    let mut h: *mut Z42Host = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_initialize(&cfg, &mut h) },
        Z42HostStatus::Ok
    );
    assert_eq!(
        unsafe { z42_host_shutdown(h) },
        Z42HostStatus::Ok
    );

    // After shutdown, a fresh initialize must succeed.
    let mut h2: *mut Z42Host = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_initialize(&cfg, &mut h2) },
        Z42HostStatus::Ok
    );
    assert!(!h2.is_null());

    assert_eq!(
        unsafe { z42_host_shutdown(h2) },
        Z42HostStatus::Ok
    );
}

#[test]
fn shutdown_when_not_initialized_returns_not_init() {
    let _g = test_lock();
    reset_host();

    let status = unsafe { z42_host_shutdown(current_host()) };
    assert_eq!(status, Z42HostStatus::NotInit);
}

#[test]
fn null_config_returns_bad_config() {
    let _g = test_lock();
    reset_host();

    let mut handle: *mut Z42Host = ptr::null_mut();
    let status = unsafe { z42_host_initialize(ptr::null(), &mut handle) };
    assert_eq!(status, Z42HostStatus::BadConfig);
    assert!(handle.is_null());
}

#[test]
fn bad_abi_version_returns_bad_config() {
    let _g = test_lock();
    reset_host();

    let mut cfg = default_config();
    cfg.abi_version = 999;

    let mut handle: *mut Z42Host = ptr::null_mut();
    let status = unsafe { z42_host_initialize(&cfg, &mut handle) };
    assert_eq!(status, Z42HostStatus::BadConfig);
    assert!(handle.is_null());

    let err = unsafe { z42_host_last_error(ptr::null_mut()) };
    assert_ne!(err.code, 0);
    let msg = unsafe { CStr::from_ptr(err.message) }.to_string_lossy();
    assert!(
        msg.contains("abi_version"),
        "expected abi_version detail, got {msg}"
    );
}

#[test]
fn struct_size_smaller_than_v2_returns_bad_config() {
    let _g = test_lock();
    reset_host();

    // A caller built against a header without the trailing resolver fields.
    let mut cfg = default_config();
    cfg.struct_size = std::mem::offset_of!(Z42HostConfig, zpkg_resolver) as u32;

    let mut handle: *mut Z42Host = ptr::null_mut();
    let status = unsafe { z42_host_initialize(&cfg, &mut handle) };
    assert_eq!(status, Z42HostStatus::BadConfig);
    assert!(handle.is_null());
    let msg = unsafe { CStr::from_ptr(z42_host_last_error(ptr::null_mut()).message) }.to_string_lossy();
    assert!(msg.contains("struct_size"), "expected struct_size detail, got {msg}");
}

#[test]
fn initialize_from_original_v2_config_buffer() {
    use std::mem::{size_of, MaybeUninit};

    let _g = test_lock();
    reset_host();

    let mut cfg = default_config();
    cfg.struct_size = abi_config::MIN_CONFIG_SIZE as u32;
    // Use an actual prefix-sized, pointer-aligned buffer, including its padding.
    let mut prefix = [MaybeUninit::<usize>::uninit();
        abi_config::MIN_CONFIG_SIZE / size_of::<usize>()];
    let prefix_bytes = prefix.as_mut_ptr().cast::<u8>();
    unsafe {
        ptr::copy_nonoverlapping(
            ptr::from_ref(&cfg).cast::<u8>(), prefix_bytes, abi_config::MIN_CONFIG_SIZE,
        );
    }
    let mut handle = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_initialize(prefix_bytes.cast(), &mut handle) },
        Z42HostStatus::Ok
    );
    assert_eq!(unsafe { z42_host_shutdown(handle) }, Z42HostStatus::Ok);
}

#[test]
fn resolver_write_copies_before_callback_returns_and_checks_field_size() {
    unsafe extern "C" fn write_resolver(
        _namespace_name: *const c_char,
        write: abi_config::Z42ZpkgByteSink,
        write_data: *mut c_void,
        _user_data: *mut c_void,
    ) -> i32 {
        let mut bytes = *b"local bytes";
        let hit = unsafe { write(bytes.as_ptr(), bytes.len(), write_data) };
        bytes.fill(0);
        std::hint::black_box(bytes);
        hit
    }

    let mut cfg = default_config();
    cfg.zpkg_resolver_write = Some(write_resolver);
    let (session, _) = unsafe { abi_config::validate(&cfg) }.unwrap();
    let resolver = session.zpkg_resolver.unwrap();
    assert_eq!(resolver.resolve("demo").as_deref(), Some(&b"local bytes"[..]));

    // A caller advertising only part of the appended pointer cannot enable it.
    cfg.struct_size = std::mem::offset_of!(Z42HostConfig, file_resolver) as u32 - 1;
    let (session, _) = unsafe { abi_config::validate(&cfg) }.unwrap();
    assert!(session.zpkg_resolver.is_none());
}

#[test]
fn last_error_clears_on_success() {
    let _g = test_lock();
    reset_host();

    // Trigger a failure to populate last_error.
    let mut cfg = default_config();
    cfg.abi_version = 999;
    let mut h: *mut Z42Host = ptr::null_mut();
    let _ = unsafe { z42_host_initialize(&cfg, &mut h) };
    assert_ne!(
        unsafe { z42_host_last_error(ptr::null_mut()) }.code,
        0,
        "precondition: last_error should be set after the failed init"
    );

    // Now do a successful initialize and assert the slot was cleared.
    cfg.abi_version = Z42_HOST_ABI_VERSION;
    assert_eq!(
        unsafe { z42_host_initialize(&cfg, &mut h) },
        Z42HostStatus::Ok
    );
    assert_eq!(
        unsafe { z42_host_last_error(ptr::null_mut()) }.code,
        0,
        "successful call must clear thread-local last_error"
    );

    assert_eq!(
        unsafe { z42_host_shutdown(h) },
        Z42HostStatus::Ok
    );
}

#[test]
fn last_error_persists_on_failure() {
    let _g = test_lock();
    reset_host();

    let mut cfg = default_config();
    cfg.abi_version = 42;
    let mut h: *mut Z42Host = ptr::null_mut();
    let status = unsafe { z42_host_initialize(&cfg, &mut h) };
    assert_eq!(status, Z42HostStatus::BadConfig);

    // Read twice; both reads must see the same error without it being
    // implicitly cleared.
    let e1 = unsafe { z42_host_last_error(ptr::null_mut()) };
    let e2 = unsafe { z42_host_last_error(ptr::null_mut()) };
    assert_eq!(e1.code, e2.code);
    assert_ne!(e1.code, 0);

    let msg = unsafe { CStr::from_ptr(e1.message) }.to_string_lossy();
    assert!(msg.contains("abi_version"));
}

// ── Additional H1 coverage ──────────────────────────────────────────────

#[test]
fn unknown_exec_mode_returns_bad_config() {
    let _g = test_lock();
    reset_host();

    let mut cfg = default_config();
    cfg.exec_mode = 7; // not a valid Z42ExecMode discriminator
    let mut h: *mut Z42Host = ptr::null_mut();
    let status = unsafe { z42_host_initialize(&cfg, &mut h) };
    assert_eq!(status, Z42HostStatus::BadConfig);
    assert!(h.is_null());
}

#[test]
fn jit_mode_when_feature_off_returns_feature_off() {
    let _g = test_lock();
    reset_host();

    let mut cfg = default_config();
    cfg.exec_mode = abi_config::Z42ExecMode::Jit as i32;
    let mut h: *mut Z42Host = ptr::null_mut();
    let status = unsafe { z42_host_initialize(&cfg, &mut h) };

    #[cfg(all(feature = "jit", z42_jit_supported))]
    {
        assert_eq!(status, Z42HostStatus::Ok);
        assert_eq!(
            unsafe { z42_host_shutdown(h) },
            Z42HostStatus::Ok
        );
    }
    #[cfg(not(all(feature = "jit", z42_jit_supported)))]
    {
        assert_eq!(status, Z42HostStatus::FeatureOff);
        assert!(h.is_null());
    }
}

#[test]
fn load_zbc_before_init_returns_not_init() {
    let _g = test_lock();
    reset_host();

    let mut out: *mut Z42Module = ptr::null_mut();
    let status = unsafe {
        z42_host_load_zbc(
            current_host(),
            ptr::null(),
            0,
            &mut out,
        )
    };
    assert_eq!(status, Z42HostStatus::NotInit);
}

// ── End-to-end hello-world ──────────────────────────────────────────────

/// Captures bytes written through the host stdout sink. Pointer to this
/// struct is the `user_data` for `host_stdout_sink_callback`.
#[cfg_attr(not(z42_have_embedding_hello), allow(dead_code))]
 struct StdoutCapture {
    bytes: Mutex<Vec<u8>>,
}

#[cfg_attr(not(z42_have_embedding_hello), allow(dead_code))]
unsafe extern "C" fn host_stdout_sink_callback(
    bytes: *const c_char,
    length: usize,
    user_data: *mut c_void,
) {
    if user_data.is_null() {
        return;
    }
    let capture = unsafe { &*(user_data as *const StdoutCapture) };
    let slice = if bytes.is_null() || length == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(bytes as *const u8, length) }
    };
    if let Ok(mut guard) = capture.bytes.lock() {
        guard.extend_from_slice(slice);
    }
}

/// Project root under cargo, used to locate `artifacts/intermediate/libraries/flat/release/`.
fn project_root() -> std::path::PathBuf {
    std::path::Path::new(env!("Z42_RUNTIME_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// `CString` form of the libs dir; `OnceLock` so the search-paths array
/// in `Z42HostConfig` can hold a stable pointer.
#[cfg(z42_have_embedding_hello)]
fn libs_dir_cstring() -> &'static CString {
    static LIBS_DIR: OnceLock<CString> = OnceLock::new();
    LIBS_DIR.get_or_init(|| {
        let libs = env!("Z42_EMBEDDING_TEST_LIBS");
        let corelib = std::path::Path::new(libs).join("z42.core.zpkg");
        assert!(
            corelib.is_file(),
            "embedding fixture needs {}; run `xtask build stdlib`",
            corelib.display()
        );
        CString::new(libs).expect("libs dir contains no NUL")
    })
}

#[test]
#[cfg(z42_have_embedding_hello)]
fn load_invoke_hello_world() {
    let _g = test_lock();
    reset_host();

    // Skip cleanly if `dotnet build` hasn't produced the corelib zpkg.
    let libs_dir = project_root().join("artifacts/intermediate/libraries/flat/release/z42.core.zpkg");
    if !libs_dir.is_file() {
        eprintln!(
            "skipping load_invoke_hello_world: {} not found (run `xtask build stdlib`)",
            libs_dir.display()
        );
        return;
    }

    let capture = StdoutCapture {
        bytes: Mutex::new(Vec::new()),
    };
    let user_data = &capture as *const StdoutCapture as *mut c_void;

    let libs_path = libs_dir_cstring();
    let search_paths: [*const c_char; 2] = [libs_path.as_ptr(), ptr::null()];

    let cfg = Z42HostConfig {
        abi_version: Z42_HOST_ABI_VERSION,
        struct_size: abi_config::MIN_CONFIG_SIZE as u32,
        exec_mode: abi_config::Z42ExecMode::Interp as i32,
        heap_initial_bytes: 0,
        heap_max_bytes: 0,
        stdout_sink: Some(host_stdout_sink_callback),
        stderr_sink: None,
        sink_user_data: user_data,
        search_paths: search_paths.as_ptr(),
        zpkg_resolver: None,
        zpkg_resolver_user_data: ptr::null_mut(),
        zpkg_resolver_write: None,
    };

    let mut host: *mut Z42Host = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_initialize(&cfg, &mut host) },
        Z42HostStatus::Ok,
        "initialize must succeed; last_error={}",
        unsafe { CStr::from_ptr(z42_host_last_error(ptr::null_mut()).message) }.to_string_lossy()
    );
    assert!(!host.is_null());

    let zbc_bytes: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/embedding_hello.zbc"));
    let mut module: *mut Z42Module = ptr::null_mut();
    let load_status = unsafe {
        z42_host_load_zbc(host, zbc_bytes.as_ptr(), zbc_bytes.len(), &mut module)
    };
    assert_eq!(
        load_status,
        Z42HostStatus::Ok,
        "load_zbc must succeed; last_error={}",
        unsafe { CStr::from_ptr(z42_host_last_error(ptr::null_mut()).message) }.to_string_lossy()
    );
    assert!(!module.is_null());

    let fqn = CString::new("Embedding.Hello.Main").unwrap();
    let mut entry: *mut Z42Entry = ptr::null_mut();
    let resolve_status =
        unsafe { z42_host_resolve_entry(host, module, fqn.as_ptr(), &mut entry) };
    assert_eq!(
        resolve_status,
        Z42HostStatus::Ok,
        "resolve_entry must succeed; last_error={}",
        unsafe { CStr::from_ptr(z42_host_last_error(ptr::null_mut()).message) }.to_string_lossy()
    );
    assert!(!entry.is_null());

    let mut result = z42_abi::Z42Value {
        tag: u32::MAX,
        reserved: 0,
        payload: 0,
    };
    let invoke_status = unsafe { z42_host_invoke(entry, ptr::null(), 0, &mut result) };
    assert_eq!(
        invoke_status,
        Z42HostStatus::Ok,
        "invoke must succeed; last_error={}",
        unsafe { CStr::from_ptr(z42_host_last_error(ptr::null_mut()).message) }.to_string_lossy()
    );
    // void return ⇒ NULL-tagged Z42Value.
    assert_eq!(result.tag, z42_abi::Z42_VALUE_TAG_NULL);

    let captured = String::from_utf8_lossy(&capture.bytes.lock().unwrap()).into_owned();
    assert_eq!(
        captured, "Hello, World!\n",
        "host stdout sink must receive the exact line emitted by Console.WriteLine"
    );

    assert_eq!(
        unsafe { z42_host_shutdown(host) },
        Z42HostStatus::Ok
    );
}

// ── H3 error-path coverage ──────────────────────────────────────────────

/// Helper: run a host session against the embedding_hello fixture and
/// hand the closure a fully-initialised `(Host*, Module*)` tuple. The
/// session is shut down on the way out regardless of test outcome.
#[cfg(z42_have_embedding_hello)]
fn with_hello_session<R>(
    capture: &Mutex<Vec<u8>>,
    body: impl FnOnce(*mut Z42Host, *mut Z42Module) -> R,
) -> R {
    with_hello_session_mode(capture, abi_config::Z42ExecMode::Interp, body)
}

#[cfg(z42_have_embedding_hello)]
fn with_hello_session_mode<R>(
    capture: &Mutex<Vec<u8>>, mode: abi_config::Z42ExecMode,
    body: impl FnOnce(*mut Z42Host, *mut Z42Module) -> R,
) -> R {
    let user_data = capture as *const Mutex<Vec<u8>> as *mut c_void;
    let libs_path = libs_dir_cstring();
    let search_paths: [*const c_char; 2] = [libs_path.as_ptr(), ptr::null()];
    let cfg = Z42HostConfig {
        abi_version: Z42_HOST_ABI_VERSION,
        struct_size: abi_config::MIN_CONFIG_SIZE as u32,
        exec_mode: mode as i32,
        heap_initial_bytes: 0,
        heap_max_bytes: 0,
        stdout_sink: Some(host_simple_capture_sink),
        stderr_sink: None,
        sink_user_data: user_data,
        search_paths: search_paths.as_ptr(),
        zpkg_resolver: None,
        zpkg_resolver_user_data: ptr::null_mut(),
        zpkg_resolver_write: None,
    };
    let mut host: *mut Z42Host = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_initialize(&cfg, &mut host) },
        Z42HostStatus::Ok
    );
    let zbc_bytes: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/embedding_hello.zbc"));
    let mut module: *mut Z42Module = ptr::null_mut();
    let load_status = unsafe {
        z42_host_load_zbc(host, zbc_bytes.as_ptr(), zbc_bytes.len(), &mut module)
    };
    assert_eq!(load_status, Z42HostStatus::Ok);
    let result = body(host, module);
    assert_eq!(unsafe { z42_host_shutdown(host) }, Z42HostStatus::Ok);
    result
}

/// Capture-only sink used by H3 tests that don't care about extra
/// formatting — the StdoutCapture in `load_invoke_hello_world` adds an
/// `[host]` prefix; here we keep raw bytes for ordering assertions.
#[cfg_attr(not(z42_have_embedding_hello), allow(dead_code))]
unsafe extern "C" fn host_simple_capture_sink(
    bytes: *const c_char,
    length: usize,
    user_data: *mut c_void,
) {
    if user_data.is_null() {
        return;
    }
    let mu = unsafe { &*(user_data as *const Mutex<Vec<u8>>) };
    let slice = if bytes.is_null() || length == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(bytes as *const u8, length) }
    };
    if let Ok(mut g) = mu.lock() {
        g.extend_from_slice(slice);
    }
}

#[test]
#[cfg(z42_have_embedding_hello)]
fn resolve_entry_unknown_fqn_returns_entry_not_found() {
    let _g = test_lock();
    reset_host();
    if !project_root().join("artifacts/intermediate/libraries/flat/release/z42.core.zpkg").is_file() {
        eprintln!("skipping: corelib zpkg not available");
        return;
    }

    let capture: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    with_hello_session(&capture, |host, module| {
        let fqn = CString::new("Embedding.Hello.NoSuchMethod").unwrap();
        let mut entry: *mut Z42Entry = ptr::null_mut();
        let status =
            unsafe { z42_host_resolve_entry(host, module, fqn.as_ptr(), &mut entry) };
        assert_eq!(status, Z42HostStatus::EntryNotFound);
        assert!(entry.is_null());
        let err = unsafe { z42_host_last_error(ptr::null_mut()) };
        let msg = unsafe { CStr::from_ptr(err.message) }.to_string_lossy();
        assert!(
            msg.contains("NoSuchMethod"),
            "expected message to mention the missing FQN, got {msg}"
        );
    });
}

#[test]
#[cfg(z42_have_embedding_hello)]
fn invoke_arg_count_mismatch_returns_arg_mismatch() {
    let _g = test_lock();
    reset_host();
    if !project_root().join("artifacts/intermediate/libraries/flat/release/z42.core.zpkg").is_file() {
        eprintln!("skipping: corelib zpkg not available");
        return;
    }

    let capture: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    with_hello_session(&capture, |host, module| {
        let fqn = CString::new("Embedding.Hello.Main").unwrap();
        let mut entry: *mut Z42Entry = ptr::null_mut();
        assert_eq!(
            unsafe { z42_host_resolve_entry(host, module, fqn.as_ptr(), &mut entry) },
            Z42HostStatus::Ok
        );

        // Main() takes zero args; passing one i64 must fail with ArgMismatch.
        let bogus_args = [z42_abi::Z42Value {
            tag: z42_abi::Z42_VALUE_TAG_I64,
            reserved: 0,
            payload: 42,
        }];
        let mut result = z42_abi::Z42Value {
            tag: u32::MAX,
            reserved: 0,
            payload: 0,
        };
        let status = unsafe {
            z42_host_invoke(entry, bogus_args.as_ptr(), bogus_args.len(), &mut result)
        };
        assert_eq!(status, Z42HostStatus::ArgMismatch);
        let err = unsafe { z42_host_last_error(ptr::null_mut()) };
        let msg = unsafe { CStr::from_ptr(err.message) }.to_string_lossy();
        assert!(msg.contains("expects 0"), "expected expects-0 detail, got {msg}");
        assert!(msg.contains("got 1"), "expected got-1 detail, got {msg}");
    });
}

#[test]
#[cfg(z42_have_embedding_hello)]
fn z42_throw_escapes_as_vm_exception_with_message() {
    let _g = test_lock();
    reset_host();
    if !project_root().join("artifacts/intermediate/libraries/flat/release/z42.core.zpkg").is_file() {
        eprintln!("skipping: corelib zpkg not available");
        return;
    }

    let capture: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    with_hello_session(&capture, |host, module| {
        let fqn = CString::new("Embedding.Hello.Boom").unwrap();
        let mut entry: *mut Z42Entry = ptr::null_mut();
        assert_eq!(
            unsafe { z42_host_resolve_entry(host, module, fqn.as_ptr(), &mut entry) },
            Z42HostStatus::Ok
        );

        let mut result = z42_abi::Z42Value {
            tag: u32::MAX,
            reserved: 0,
            payload: 0,
        };
        let status = unsafe { z42_host_invoke(entry, ptr::null(), 0, &mut result) };
        assert_eq!(
            status,
            Z42HostStatus::VmException,
            "z42 throw must surface as VmException"
        );
        let err = unsafe { z42_host_last_error(ptr::null_mut()) };
        let msg = unsafe { CStr::from_ptr(err.message) }.to_string_lossy();
        assert!(
            msg.contains("intentional embedding-test failure: arg-count-mismatch:"),
            "expected exception message to be propagated, got {msg}"
        );
    });
}

#[test]
#[cfg(z42_have_embedding_hello)]
fn sink_called_in_correct_order_for_multiple_lines() {
    let _g = test_lock();
    reset_host();
    if !project_root().join("artifacts/intermediate/libraries/flat/release/z42.core.zpkg").is_file() {
        eprintln!("skipping: corelib zpkg not available");
        return;
    }

    let capture: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    with_hello_session(&capture, |host, module| {
        let fqn = CString::new("Embedding.Hello.MultiLine").unwrap();
        let mut entry: *mut Z42Entry = ptr::null_mut();
        assert_eq!(
            unsafe { z42_host_resolve_entry(host, module, fqn.as_ptr(), &mut entry) },
            Z42HostStatus::Ok
        );
        assert_eq!(
            unsafe { z42_host_invoke(entry, ptr::null(), 0, ptr::null_mut()) },
            Z42HostStatus::Ok
        );
    });

    let captured = String::from_utf8_lossy(&capture.lock().unwrap()).into_owned();
    assert_eq!(
        captured, "first\nsecond\nthird\n",
        "host stdout sink must receive lines in invocation order"
    );
}

#[test]
fn load_zbc_with_garbage_bytes_returns_bad_zbc() {
    let _g = test_lock();
    reset_host();

    let cfg = default_config();
    let mut h: *mut Z42Host = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_initialize(&cfg, &mut h) },
        Z42HostStatus::Ok
    );

    // Empty buffer — too short to even contain magic bytes.
    let mut out: *mut Z42Module = ptr::null_mut();
    let status = unsafe { z42_host_load_zbc(h, ptr::null(), 0, &mut out) };
    assert_eq!(status, Z42HostStatus::BadZbc);
    assert!(out.is_null());

    // Wrong-magic buffer — recognisable size but bytes are nonsense.
    let garbage = b"NOTAVALIDZBC".to_vec();
    let mut out2: *mut Z42Module = ptr::null_mut();
    let status =
        unsafe { z42_host_load_zbc(h, garbage.as_ptr(), garbage.len(), &mut out2) };
    assert_eq!(status, Z42HostStatus::BadZbc);
    assert!(out2.is_null());

    let err = unsafe { z42_host_last_error(ptr::null_mut()) };
    let msg = unsafe { CStr::from_ptr(err.message) }.to_string_lossy();
    assert!(
        msg.contains("magic") || msg.contains("zbc"),
        "expected magic/zbc detail, got {msg}"
    );

    assert_eq!(
        unsafe { z42_host_shutdown(h) },
        Z42HostStatus::Ok
    );
}

// ── ZpkgResolver tests (add-zpkg-resolver-hook) ─────────────────────────

use std::collections::HashMap;
use std::sync::Arc;

use super::resolver::ZpkgResolver;

/// Test-local `ZpkgResolver` backed by a HashMap. We don't depend on the
/// Tier 2 `z42-host::MapResolver` here because the runtime crate is below
/// it in the dependency graph.
#[cfg_attr(not(z42_have_embedding_hello), allow(dead_code))]
 struct TestMapResolver {
    map: HashMap<String, Vec<u8>>,
}

impl ZpkgResolver for TestMapResolver {
    fn resolve(&self, namespace: &str) -> Option<Vec<u8>> {
        self.map.get(namespace).cloned()
    }
}

/// Resolver that always misses, but counts how many lookup attempts
/// were made — useful for proving fallback behaviour.
#[cfg_attr(not(z42_have_embedding_hello), allow(dead_code))]
 struct AlwaysMissResolver {
    attempts: std::sync::Mutex<Vec<String>>,
}

impl ZpkgResolver for AlwaysMissResolver {
    fn resolve(&self, namespace: &str) -> Option<Vec<u8>> {
        if let Ok(mut g) = self.attempts.lock() {
            g.push(namespace.to_string());
        }
        None
    }
}

/// Load the stdlib bytes from `artifacts/intermediate/libraries/flat/release/`. Returns `None` if
/// the stdlib isn't built yet (run `xtask build stdlib`) — tests skip in that case.
fn load_stdlib_bytes(name: &str) -> Option<Vec<u8>> {
    let path = project_root().join("artifacts/intermediate/libraries/flat/release").join(name);
    std::fs::read(path).ok()
}

// ── z42_zpkg_read_namespaces (drop-index-json-self-describing) ──────────

unsafe extern "C" fn ns_collect(ns: *const c_char, len: usize, ud: *mut c_void) {
    let out = unsafe { &mut *(ud as *mut Vec<String>) };
    let slice = unsafe { std::slice::from_raw_parts(ns as *const u8, len) };
    out.push(String::from_utf8_lossy(slice).into_owned());
}

#[test]
fn zpkg_read_namespaces_lists_nspc() {
    // Needs a real stdlib zpkg; skip cleanly if the workspace isn't built.
    let core = match load_stdlib_bytes("z42.core.zpkg") {
        Some(b) => b,
        None => {
            eprintln!("skip: corelib not built");
            return;
        }
    };
    let mut out: Vec<String> = Vec::new();
    let bad_status = unsafe {
        z42_zpkg_read_namespaces(core.as_ptr(), core.len(), None, ptr::null_mut())
    };
    assert_eq!(bad_status, Z42HostStatus::BadConfig);
    assert_eq!(unsafe { z42_host_last_error(ptr::null_mut()).code }, bad_status.as_code());

    let status = unsafe {
        z42_zpkg_read_namespaces(
            core.as_ptr(),
            core.len(),
            Some(ns_collect),
            &mut out as *mut Vec<String> as *mut c_void,
        )
    };
    assert_eq!(status, Z42HostStatus::Ok);
    assert_eq!(unsafe { z42_host_last_error(ptr::null_mut()).code }, 0);
    assert_eq!(out.first().map(String::as_str), Some("z42.core"));
    // z42.core.zpkg resolves by its package name ("z42.core", the prelude
    // key) plus the namespaces it declares (Std, ...) — exactly the keys a
    // hand-maintained index would otherwise have to encode (and drift from).
    assert!(out.iter().any(|n| n == "z42.core"), "expected z42.core in {out:?}");
    assert!(out.iter().any(|n| n == "Std"), "expected Std in {out:?}");
    assert_eq!(super::read_zpkg_namespaces(&core).unwrap(), out);
}

#[test]
fn zpkg_read_namespaces_rejects_garbage() {
    let garbage = b"NOTAZPKG".to_vec();
    let mut out = Vec::<String>::new();
    let status = unsafe {
        z42_zpkg_read_namespaces(
            garbage.as_ptr(),
            garbage.len(),
            Some(ns_collect),
            &mut out as *mut Vec<String> as *mut c_void,
        )
    };
    assert_eq!(status, Z42HostStatus::BadZbc);
    assert!(out.is_empty());
    let error = unsafe { z42_host_last_error(ptr::null_mut()) };
    assert_eq!(error.code, status.as_code());
    assert!(unsafe { CStr::from_ptr(error.message) }.to_bytes()
        .starts_with(b"z42_zpkg_read_namespaces: cannot read zpkg metadata"));
    assert!(super::read_zpkg_namespaces(&garbage).is_err());
}

#[test]
fn zpkg_read_namespaces_null_visitor_is_bad_config() {
    let bytes = b"whatever".to_vec();
    let status =
        unsafe { z42_zpkg_read_namespaces(bytes.as_ptr(), bytes.len(), None, ptr::null_mut()) };
    assert_eq!(status, Z42HostStatus::BadConfig);
}

#[test]
#[cfg(z42_have_embedding_hello)]
fn resolver_via_map_resolver_loads_corelib_without_search_paths() {
    let _g = test_lock();
    reset_host();

    let core_bytes = match load_stdlib_bytes("z42.core.zpkg") {
        Some(b) => b,
        None => {
            eprintln!("skip: corelib not built");
            return;
        }
    };
    let io_bytes = load_stdlib_bytes("z42.io.zpkg").expect("z42.io.zpkg");

    let mut map = HashMap::new();
    map.insert("z42.core".to_string(), core_bytes);
    map.insert("Std.IO".to_string(), io_bytes);
    let resolver: Arc<dyn ZpkgResolver> = Arc::new(TestMapResolver { map });

    // Capture stdout
    let capture: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    let user_data = &capture as *const Mutex<Vec<u8>> as *mut c_void;

    // No search_paths — relies entirely on the resolver.
    let cfg = Z42HostConfig {
        abi_version: Z42_HOST_ABI_VERSION,
        struct_size: abi_config::MIN_CONFIG_SIZE as u32,
        exec_mode: abi_config::Z42ExecMode::Interp as i32,
        heap_initial_bytes: 0,
        heap_max_bytes: 0,
        stdout_sink: Some(host_simple_capture_sink),
        stderr_sink: None,
        sink_user_data: user_data,
        search_paths: ptr::null(),
        zpkg_resolver: None,
        zpkg_resolver_user_data: ptr::null_mut(),
        zpkg_resolver_write: None,
    };
    let mut host: *mut Z42Host = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_initialize(&cfg, &mut host) },
        Z42HostStatus::Ok
    );

    // Plug resolver in post-init via the Rust-only escape hatch.
    assert_eq!(super::install_zpkg_resolver(resolver), Z42HostStatus::Ok);

    let zbc_bytes: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/embedding_hello.zbc"));
    let mut module: *mut Z42Module = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_load_zbc(host, zbc_bytes.as_ptr(), zbc_bytes.len(), &mut module) },
        Z42HostStatus::Ok,
        "load_zbc must succeed; last_error={}",
        unsafe { CStr::from_ptr(z42_host_last_error(ptr::null_mut()).message) }.to_string_lossy()
    );

    let fqn = CString::new("Embedding.Hello.Main").unwrap();
    let mut entry: *mut Z42Entry = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_resolve_entry(host, module, fqn.as_ptr(), &mut entry) },
        Z42HostStatus::Ok
    );

    assert_eq!(
        unsafe { z42_host_invoke(entry, ptr::null(), 0, ptr::null_mut()) },
        Z42HostStatus::Ok
    );

    let captured = String::from_utf8_lossy(&capture.lock().unwrap()).into_owned();
    assert_eq!(captured, "Hello, World!\n");

    assert_eq!(unsafe { z42_host_shutdown(host) }, Z42HostStatus::Ok);
}

// ── C-hook resolver test infra ──────────────────────────────────────────

#[cfg_attr(not(z42_have_embedding_hello), allow(dead_code))]
static C_HOOK_ZPKGS: OnceLock<HashMap<String, Vec<u8>>> = OnceLock::new();

#[cfg_attr(not(z42_have_embedding_hello), allow(dead_code))]
fn c_hook_zpkgs() -> &'static HashMap<String, Vec<u8>> {
    C_HOOK_ZPKGS.get_or_init(|| {
        let mut m = HashMap::new();
        if let Some(b) = load_stdlib_bytes("z42.core.zpkg") {
            m.insert("z42.core".to_string(), b);
        }
        if let Some(b) = load_stdlib_bytes("z42.io.zpkg") {
            m.insert("Std.IO".to_string(), b);
        }
        m
    })
}

#[cfg_attr(not(z42_have_embedding_hello), allow(dead_code))]
unsafe extern "C" fn c_hook_resolver_fn(
    namespace_name: *const c_char,
    out_bytes: *mut *const u8,
    out_length: *mut usize,
    _user_data: *mut c_void,
) -> i32 {
    if namespace_name.is_null() {
        return 0;
    }
    let ns = match unsafe { CStr::from_ptr(namespace_name) }.to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let map = c_hook_zpkgs();
    match map.get(ns) {
        Some(bytes) => {
            unsafe {
                *out_bytes = bytes.as_ptr();
                *out_length = bytes.len();
            }
            1
        }
        None => 0,
    }
}

#[test]
#[cfg(z42_have_embedding_hello)]
fn resolver_via_c_hook_loads_corelib() {
    let _g = test_lock();
    reset_host();

    if c_hook_zpkgs().is_empty() {
        eprintln!("skip: corelib not built");
        return;
    }

    let capture: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    let user_data = &capture as *const Mutex<Vec<u8>> as *mut c_void;

    let cfg = Z42HostConfig {
        abi_version: Z42_HOST_ABI_VERSION,
        struct_size: abi_config::MIN_CONFIG_SIZE as u32,
        exec_mode: abi_config::Z42ExecMode::Interp as i32,
        heap_initial_bytes: 0,
        heap_max_bytes: 0,
        stdout_sink: Some(host_simple_capture_sink),
        stderr_sink: None,
        sink_user_data: user_data,
        search_paths: ptr::null(),
        zpkg_resolver: Some(c_hook_resolver_fn),
        zpkg_resolver_user_data: ptr::null_mut(),
        zpkg_resolver_write: None,
    };
    let mut host: *mut Z42Host = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_initialize(&cfg, &mut host) },
        Z42HostStatus::Ok
    );

    let zbc_bytes: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/embedding_hello.zbc"));
    let mut module: *mut Z42Module = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_load_zbc(host, zbc_bytes.as_ptr(), zbc_bytes.len(), &mut module) },
        Z42HostStatus::Ok
    );

    let fqn = CString::new("Embedding.Hello.Main").unwrap();
    let mut entry: *mut Z42Entry = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_resolve_entry(host, module, fqn.as_ptr(), &mut entry) },
        Z42HostStatus::Ok
    );
    assert_eq!(
        unsafe { z42_host_invoke(entry, ptr::null(), 0, ptr::null_mut()) },
        Z42HostStatus::Ok
    );

    let captured = String::from_utf8_lossy(&capture.lock().unwrap()).into_owned();
    assert_eq!(captured, "Hello, World!\n");

    assert_eq!(unsafe { z42_host_shutdown(host) }, Z42HostStatus::Ok);
}

#[test]
#[cfg(z42_have_embedding_hello)]
fn resolver_miss_falls_back_to_search_paths() {
    let _g = test_lock();
    reset_host();

    let libs_dir = project_root().join("artifacts/intermediate/libraries/flat/release");
    if !libs_dir.join("z42.core.zpkg").is_file() {
        eprintln!("skip: corelib not built");
        return;
    }

    let always_miss: Arc<AlwaysMissResolver> = Arc::new(AlwaysMissResolver {
        attempts: std::sync::Mutex::new(Vec::new()),
    });
    let resolver_dyn: Arc<dyn ZpkgResolver> = always_miss.clone();

    let capture: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    let user_data = &capture as *const Mutex<Vec<u8>> as *mut c_void;

    let libs_cstr = CString::new(libs_dir.to_string_lossy().as_bytes()).unwrap();
    let search_paths: [*const c_char; 2] = [libs_cstr.as_ptr(), ptr::null()];

    let cfg = Z42HostConfig {
        abi_version: Z42_HOST_ABI_VERSION,
        struct_size: abi_config::MIN_CONFIG_SIZE as u32,
        exec_mode: abi_config::Z42ExecMode::Interp as i32,
        heap_initial_bytes: 0,
        heap_max_bytes: 0,
        stdout_sink: Some(host_simple_capture_sink),
        stderr_sink: None,
        sink_user_data: user_data,
        search_paths: search_paths.as_ptr(),
        zpkg_resolver: None,
        zpkg_resolver_user_data: ptr::null_mut(),
        zpkg_resolver_write: None,
    };
    let mut host: *mut Z42Host = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_initialize(&cfg, &mut host) },
        Z42HostStatus::Ok
    );
    assert_eq!(super::install_zpkg_resolver(resolver_dyn), Z42HostStatus::Ok);

    let zbc_bytes: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/embedding_hello.zbc"));
    let mut module: *mut Z42Module = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_load_zbc(host, zbc_bytes.as_ptr(), zbc_bytes.len(), &mut module) },
        Z42HostStatus::Ok
    );

    // The resolver was queried at least twice (z42.core + Std.IO) before
    // search_paths kicked in.
    let attempts = always_miss.attempts.lock().unwrap();
    assert!(
        attempts.iter().any(|s| s == "z42.core"),
        "resolver should have been queried for z42.core; attempts={attempts:?}"
    );
    assert!(
        attempts.iter().any(|s| s == "Std.IO"),
        "resolver should have been queried for Std.IO; attempts={attempts:?}"
    );
    drop(attempts);

    // search_paths fallback supplied corelib + Std.IO, so invoke works.
    let fqn = CString::new("Embedding.Hello.Main").unwrap();
    let mut entry: *mut Z42Entry = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_resolve_entry(host, module, fqn.as_ptr(), &mut entry) },
        Z42HostStatus::Ok
    );
    assert_eq!(
        unsafe { z42_host_invoke(entry, ptr::null(), 0, ptr::null_mut()) },
        Z42HostStatus::Ok
    );

    let captured = String::from_utf8_lossy(&capture.lock().unwrap()).into_owned();
    assert_eq!(captured, "Hello, World!\n");

    assert_eq!(unsafe { z42_host_shutdown(host) }, Z42HostStatus::Ok);
}

#[test]
#[cfg(z42_have_embedding_hello)]
fn resolver_both_unset_load_zbc_succeeds_if_zbc_self_contained() {
    let _g = test_lock();
    reset_host();

    // No resolver, no search_paths. load_zbc should still succeed
    // because corelib miss is silent — the runtime only complains
    // when user code actually references a missing symbol at invoke.
    let cfg = Z42HostConfig {
        abi_version: Z42_HOST_ABI_VERSION,
        struct_size: abi_config::MIN_CONFIG_SIZE as u32,
        exec_mode: abi_config::Z42ExecMode::Interp as i32,
        heap_initial_bytes: 0,
        heap_max_bytes: 0,
        stdout_sink: None,
        stderr_sink: None,
        sink_user_data: ptr::null_mut(),
        search_paths: ptr::null(),
        zpkg_resolver: None,
        zpkg_resolver_user_data: ptr::null_mut(),
        zpkg_resolver_write: None,
    };
    let mut host: *mut Z42Host = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_initialize(&cfg, &mut host) },
        Z42HostStatus::Ok
    );

    let zbc_bytes: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/embedding_hello.zbc"));
    let mut module: *mut Z42Module = ptr::null_mut();
    let status = unsafe {
        z42_host_load_zbc(host, zbc_bytes.as_ptr(), zbc_bytes.len(), &mut module)
    };
    assert_eq!(
        status,
        Z42HostStatus::Ok,
        "load_zbc must succeed even without corelib; only invoke fails"
    );
    assert!(!module.is_null());

    // Resolve also succeeds — the entry symbol is defined in the user .zbc.
    let fqn = CString::new("Embedding.Hello.Main").unwrap();
    let mut entry: *mut Z42Entry = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_resolve_entry(host, module, fqn.as_ptr(), &mut entry) },
        Z42HostStatus::Ok
    );

    assert_eq!(unsafe { z42_host_shutdown(host) }, Z42HostStatus::Ok);
}

#[test]
#[cfg(z42_have_embedding_hello)]
fn resolver_corelib_miss_then_console_writeline_fails_at_invoke() {
    let _g = test_lock();
    reset_host();

    let cfg = Z42HostConfig {
        abi_version: Z42_HOST_ABI_VERSION,
        struct_size: abi_config::MIN_CONFIG_SIZE as u32,
        exec_mode: abi_config::Z42ExecMode::Interp as i32,
        heap_initial_bytes: 0,
        heap_max_bytes: 0,
        stdout_sink: None,
        stderr_sink: None,
        sink_user_data: ptr::null_mut(),
        search_paths: ptr::null(),
        zpkg_resolver: None,
        zpkg_resolver_user_data: ptr::null_mut(),
        zpkg_resolver_write: None,
    };
    let mut host: *mut Z42Host = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_initialize(&cfg, &mut host) },
        Z42HostStatus::Ok
    );

    let zbc_bytes: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/embedding_hello.zbc"));
    let mut module: *mut Z42Module = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_load_zbc(host, zbc_bytes.as_ptr(), zbc_bytes.len(), &mut module) },
        Z42HostStatus::Ok
    );

    let fqn = CString::new("Embedding.Hello.Main").unwrap();
    let mut entry: *mut Z42Entry = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_resolve_entry(host, module, fqn.as_ptr(), &mut entry) },
        Z42HostStatus::Ok
    );

    // Invoke hits Console.WriteLine, which is now undefined.
    let status = unsafe { z42_host_invoke(entry, ptr::null(), 0, ptr::null_mut()) };
    assert_eq!(
        status,
        Z42HostStatus::VmException,
        "missing corelib must surface as VmException at invoke, not Internal"
    );
    let err = unsafe { z42_host_last_error(ptr::null_mut()) };
    let msg = unsafe { CStr::from_ptr(err.message) }.to_string_lossy();
    assert!(
        msg.contains("undefined function") || msg.contains("Console"),
        "expected undefined-function diagnostic, got {msg}"
    );

    assert_eq!(unsafe { z42_host_shutdown(host) }, Z42HostStatus::Ok);
}

/// Resolve + invoke a zero-arg `Embedding.Hello.<name>` and return the raw result.
#[cfg(z42_have_embedding_hello)]
fn invoke_hello_fn(host: *mut Z42Host, module: *mut Z42Module, name: &str) -> z42_abi::Z42Value {
    let fqn = CString::new(format!("Embedding.Hello.{name}")).unwrap();
    let mut entry: *mut Z42Entry = ptr::null_mut();
    assert_eq!(
        unsafe { z42_host_resolve_entry(host, module, fqn.as_ptr(), &mut entry) },
        Z42HostStatus::Ok
    );
    let mut result = z42_abi::Z42Value { tag: u32::MAX, reserved: 0, payload: 0 };
    let status = unsafe { z42_host_invoke(entry, ptr::null(), 0, &mut result) };
    assert_eq!(
        status,
        Z42HostStatus::Ok,
        "invoke {name} must succeed; last_error={}",
        unsafe { CStr::from_ptr(z42_host_last_error(ptr::null_mut()).message) }.to_string_lossy()
    );
    result
}

/// fix-host-static-init: `z42_host_load_zbc` + `invoke` never ran `__static_init__`, so a
/// static field with an initializer read back as its type default (0) instead of an error —
/// for fields in merged stdlib packages (`OSKind.Wasm`) and in the user module alike.
/// First seen on wasm (`Platform.IsWasm()` always false) but the path is shared by every
/// embedding. Also pins "once per module": a second invoke must not re-run the initializers.
#[test]
#[cfg(z42_have_embedding_hello)]
fn invoke_sees_initialized_static_fields() {
    let _g = test_lock();
    reset_host();
    if !project_root().join("artifacts/intermediate/libraries/flat/release/z42.core.zpkg").is_file() {
        eprintln!("skipping: corelib zpkg not available");
        return;
    }

    let capture: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    with_hello_session(&capture, |host, module| {
        let core = invoke_hello_fn(host, module, "CoreStatic");
        assert_eq!((core.tag, core.payload), (z42_abi::Z42_VALUE_TAG_I64, 6), "OSKind.Wasm");
        let user = invoke_hello_fn(host, module, "UserStatic");
        assert_eq!((user.tag, user.payload), (z42_abi::Z42_VALUE_TAG_I64, 42), "Counter.Start");
        let again = invoke_hello_fn(host, module, "CoreStatic");
        assert_eq!(again.payload, 6, "second invoke keeps the initialized value");
    });
}

/// Sink that shuts the host down from inside the callback — a host reacting
/// to program output by tearing the VM down.
#[cfg(z42_have_embedding_hello)]
unsafe extern "C" fn host_shutdown_sink(_bytes: *const c_char, _length: usize, _user_data: *mut c_void) {
    let _ = unsafe { z42_host_shutdown(current_host()) };
}

/// `z42_host_invoke` used to run user code under the `HOST` read lock and the
/// sink callback under the sink slot's read lock; a callback calling back into
/// an API that takes either write lock (`shutdown` takes both) deadlocked.
#[test]
#[cfg(z42_have_embedding_hello)]
fn sink_callback_can_shut_down_the_host_mid_invoke() {
    let _g = test_lock();
    reset_host();
    let libs_path = libs_dir_cstring();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let search_paths: [*const c_char; 2] = [libs_path.as_ptr(), ptr::null()];
        let mut cfg = default_config();
        cfg.stdout_sink = Some(host_shutdown_sink);
        cfg.search_paths = search_paths.as_ptr();
        let mut host: *mut Z42Host = ptr::null_mut();
        assert_eq!(unsafe { z42_host_initialize(&cfg, &mut host) }, Z42HostStatus::Ok);
        let zbc_bytes: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/embedding_hello.zbc"));
        let mut module: *mut Z42Module = ptr::null_mut();
        assert_eq!(
            unsafe { z42_host_load_zbc(host, zbc_bytes.as_ptr(), zbc_bytes.len(), &mut module) },
            Z42HostStatus::Ok
        );
        let fqn = CString::new("Embedding.Hello.MultiLine").unwrap();
        let mut entry: *mut Z42Entry = ptr::null_mut();
        assert_eq!(
            unsafe { z42_host_resolve_entry(host, module, fqn.as_ptr(), &mut entry) },
            Z42HostStatus::Ok
        );
        let mut result = z42_abi::Z42Value { tag: u32::MAX, reserved: 0, payload: 0 };
        let status = unsafe { z42_host_invoke(entry, ptr::null(), 0, &mut result) };
        let _ = tx.send(status);
    });
    let status = rx
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("invoke deadlocked: a sink callback re-entering the host API never returned");
    // The module stays alive for the rest of the call; the host is gone after it.
    assert_eq!(status, Z42HostStatus::Ok);
    assert_eq!(unsafe { z42_host_shutdown(current_host()) }, Z42HostStatus::NotInit);
}

/// runtime-audit P0-4: unbounded recursion is a fatal VM error. `z42_host_invoke`
/// returns `Z42_HOST_ERR_FATAL` with the report; the z42 `catch` / `finally` around
/// the recursion never run.
#[test]
#[cfg(z42_have_embedding_hello)]
fn invoke_deep_recursion_is_fatal() {
    let _g = test_lock();
    let _f = crate::exec::stack_guard::FATAL_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    reset_host();

    let capture: Mutex<Vec<u8>> = Mutex::new(Vec::new());
    with_hello_session(&capture, |host, module| {
        let fqn = CString::new("Embedding.Hello.Deep").unwrap();
        let mut entry: *mut Z42Entry = ptr::null_mut();
        assert_eq!(
            unsafe { z42_host_resolve_entry(host, module, fqn.as_ptr(), &mut entry) },
            Z42HostStatus::Ok
        );
        let mut result = z42_abi::Z42Value { tag: u32::MAX, reserved: 0, payload: 0 };
        let status = unsafe { z42_host_invoke(entry, ptr::null(), 0, &mut result) };
        assert_eq!(status, Z42HostStatus::Fatal);
        let msg = unsafe { CStr::from_ptr(z42_host_last_error(ptr::null_mut()).message) }
            .to_string_lossy()
            .into_owned();
        assert!(msg.contains("stack overflow"), "unexpected message: {msg}");
        assert!(msg.contains("Embedding.Hello.Down"), "no z42 frames in: {msg}");
    });
    let out = String::from_utf8_lossy(&capture.lock().unwrap()).into_owned();
    assert!(!out.contains("caught") && !out.contains("finally"), "a handler ran: {out}");
    assert!(crate::exec::stack_guard::take_report().is_none(), "C ABI must consume its fatal report after returning it in last_error");

}

#[test]
fn load_cannot_publish_after_resolver_replaces_the_session() {
    let _g = test_lock();
    reset_host();
    let cfg = default_config();
    let mut handle = ptr::null_mut();
    assert_eq!(unsafe { z42_host_initialize(&cfg, &mut handle) }, Z42HostStatus::Ok);
    struct RestartResolver;
    impl ZpkgResolver for RestartResolver {
        fn resolve(&self, _namespace: &str) -> Option<Vec<u8>> {
            assert_eq!(unsafe { z42_host_shutdown(current_host()) }, Z42HostStatus::Ok);
            start_host(&default_config());
            None
        }
    }
    assert_eq!(install_zpkg_resolver(Arc::new(RestartResolver)), Z42HostStatus::Ok);
    let bytes = crate::loader::test_support::empty_zbc();
    let mut module = ptr::null_mut();
    let status = unsafe { z42_host_load_zbc(handle, bytes.as_ptr(), bytes.len(), &mut module) };
    assert_eq!(status, Z42HostStatus::NotInit);
    assert!(module.is_null());
    let new_host = current_host();
    assert_ne!(handle, new_host);
    assert_eq!(unsafe { z42_host_load_zbc(new_host, bytes.as_ptr(), bytes.len(), &mut module) }, Z42HostStatus::Ok);
    assert_eq!(unsafe { z42_host_shutdown(new_host) }, Z42HostStatus::Ok);
}

fn start_host(cfg: &Z42HostConfig) -> *mut Z42Host {
    let mut host = ptr::null_mut();
    assert_eq!(unsafe { z42_host_initialize(cfg, &mut host) }, Z42HostStatus::Ok);
    host
}

#[test]
fn a_module_handle_does_not_alias_a_reinitialized_session() {
    let _g = test_lock();
    reset_host();
    let cfg = default_config();
    let old = start_host(&cfg);
    let bytes = crate::loader::test_support::empty_zbc();
    let mut module = ptr::null_mut();
    assert_eq!(unsafe { z42_host_load_zbc(old, bytes.as_ptr(), bytes.len(), &mut module) }, Z42HostStatus::Ok);
    assert_eq!(unsafe { z42_host_shutdown(old) }, Z42HostStatus::Ok);

    let new = start_host(&cfg);
    let mut new_module = ptr::null_mut();
    assert_eq!(unsafe { z42_host_load_zbc(new, bytes.as_ptr(), bytes.len(), &mut new_module) }, Z42HostStatus::Ok);
    assert_ne!(module, new_module);
    let fqn = CString::new("Missing.Main").unwrap();
    assert_eq!(unsafe { z42_host_resolve_entry(new, module, fqn.as_ptr(), ptr::null_mut()) }, Z42HostStatus::NotInit);
    assert_eq!(unsafe { z42_host_shutdown(old) }, Z42HostStatus::NotInit);
    assert_eq!(unsafe { z42_host_shutdown(new) }, Z42HostStatus::Ok);
}

#[test]
#[cfg(z42_have_embedding_hello)]
fn an_entry_handle_does_not_alias_a_reinitialized_session() {
    let _g = test_lock();
    reset_host();
    let capture = Mutex::new(Vec::new());
    let fqn = CString::new("Embedding.Hello.Main").unwrap();
    let resolve = |host, module| {
        let mut entry = ptr::null_mut();
        assert_eq!(
            unsafe { z42_host_resolve_entry(host, module, fqn.as_ptr(), &mut entry) },
            Z42HostStatus::Ok
        );
        entry
    };
    let old_entry = with_hello_session(&capture, resolve);
    with_hello_session(&capture, |host, module| {
        let new_entry = resolve(host, module);
        assert_ne!(old_entry, new_entry);
        assert_eq!(
            unsafe { z42_host_invoke(old_entry, ptr::null(), 0, ptr::null_mut()) },
            Z42HostStatus::NotInit
        );
        assert_eq!(
            unsafe { z42_host_invoke(new_entry, ptr::null(), 0, ptr::null_mut()) },
            Z42HostStatus::Ok
        );
    });
}

#[test]
fn vfs_validation_can_reenter_and_cannot_publish_into_the_replacement_session() {
    use crate::pal::fs::Vfs;
    use std::io::{ErrorKind, Result};

    // Only existence is queried during the prelude probe.
    struct RestartVfs;
    impl Vfs for RestartVfs {
        fn exists(&self, _: &str) -> bool {
            assert_eq!(unsafe { z42_host_shutdown(current_host()) }, Z42HostStatus::Ok);
            start_host(&default_config());
            false
        }
        fn is_dir(&self, _: &str) -> bool { false }
        fn read_to_string(&self, _: &str) -> Result<String> { Err(ErrorKind::Unsupported.into()) }
        fn read(&self, _: &str) -> Result<Vec<u8>> { Err(ErrorKind::Unsupported.into()) }
        fn write(&self, _: &str, _: &[u8]) -> Result<()> { Err(ErrorKind::Unsupported.into()) }
        fn append(&self, _: &str, _: &[u8]) -> Result<()> { Err(ErrorKind::Unsupported.into()) }
        fn remove_file(&self, _: &str) -> Result<()> { Err(ErrorKind::Unsupported.into()) }
        fn remove_dir(&self, _: &str, _: bool) -> Result<()> { Err(ErrorKind::Unsupported.into()) }
        fn copy(&self, _: &str, _: &str) -> Result<()> { Err(ErrorKind::Unsupported.into()) }
        fn rename(&self, _: &str, _: &str) -> Result<()> { Err(ErrorKind::Unsupported.into()) }
        fn create_dir_all(&self, _: &str) -> Result<()> { Err(ErrorKind::Unsupported.into()) }
        fn modified_ms(&self, _: &str) -> Result<i64> { Err(ErrorKind::Unsupported.into()) }
        fn file_len(&self, _: &str) -> Result<u64> { Err(ErrorKind::Unsupported.into()) }
        fn read_dir(&self, _: &str) -> Result<Vec<String>> { Err(ErrorKind::Unsupported.into()) }
        fn glob(&self, _: &str, _: &str) -> Result<Vec<String>> { Err(ErrorKind::Unsupported.into()) }
        fn write_atomic(&self, _: &str, _: &[u8]) -> Result<()> { Err(ErrorKind::Unsupported.into()) }
    }

    let _g = test_lock();
    reset_host();
    let directory = CString::new("/virtual-host-vfs-test").unwrap();
    let dirs = [directory.as_ptr(), ptr::null()];
    let mut cfg = default_config();
    cfg.search_paths = dirs.as_ptr();
    let old = start_host(&cfg);
    let (tx, rx) = std::sync::mpsc::channel();
    let probe = std::thread::spawn(move || tx.send(install_vfs(Arc::new(RestartVfs))).unwrap());
    assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(10)).expect("VFS probe deadlocked"), Z42HostStatus::NotInit);
    probe.join().unwrap();
    assert_ne!(old, current_host());
    assert_eq!(unsafe { z42_host_shutdown(current_host()) }, Z42HostStatus::Ok);
}

#[test]
fn rust_host_uses_the_shared_session_and_its_own_error_channel() {
    use super::rust::{ExecMode, Host, HostConfig, HostError};
    let _g = test_lock();
    reset_host();
    let host = Host::new(HostConfig { exec_mode: ExecMode::Interp, ..Default::default() }).unwrap();
    let mut c_host = ptr::null_mut();
    assert_eq!(unsafe { z42_host_initialize(&default_config(), &mut c_host) }, Z42HostStatus::AlreadyInit);
    let module = host.load_zbc(&crate::loader::test_support::empty_zbc()).unwrap();
    assert!(matches!(host.resolve_entry(&module, "Missing.Main"), Err(HostError::EntryNotFound(_))));
    let c_error = super::error::snapshot_last_error();
    assert_eq!(c_error.code, Z42HostStatus::AlreadyInit.as_code());
    drop(host);
    let new = start_host(&default_config());
    assert_eq!(unsafe { z42_host_shutdown(new) }, Z42HostStatus::Ok);
}

#[test]
fn file_resolver_loads_configs_and_same_namespace_packages_by_name() {
    use crate::zpkg::format::{ZPKG_MAGIC, ZPKG_VERSION_MAJOR, ZPKG_VERSION_MINOR};

    fn packed_zpkg(name: &str) -> Vec<u8> {
        let mut meta = Vec::new();
        for field in [name, "0.1", ""] {
            meta.extend_from_slice(&(field.len() as u16).to_le_bytes());
            meta.extend_from_slice(field.as_bytes());
        }
        // One pooled string ("Shared") and one namespace pointing to it.
        let strs = b"\x01\0\0\0\x06Shared\x01\0\0\0\x01\0";
        let nspc = b"\x01\0\0\0\0\0\0\0";
        // One empty module with a four-byte FUNC section containing a zero count.
        let mut mods = 1u32.to_le_bytes().to_vec();
        mods.extend_from_slice(&[0; 18]);
        mods.extend_from_slice(&4u32.to_le_bytes());
        mods.extend_from_slice(&[0; 16]);
        let sections: [(&[u8; 4], &[u8]); 4] =
            [(b"META", &meta), (b"STRS", strs), (b"NSPC", nspc), (b"MODS", &mods)];
        let mut bytes = ZPKG_MAGIC.to_vec();
        bytes.extend_from_slice(&ZPKG_VERSION_MAJOR.to_le_bytes());
        bytes.extend_from_slice(&ZPKG_VERSION_MINOR.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&(sections.len() as u16).to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        let mut offset = 16 + sections.len() as u32 * 12;
        for (tag, data) in sections {
            bytes.extend_from_slice(tag);
            bytes.extend_from_slice(&offset.to_le_bytes());
            bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
            offset += data.len() as u32;
        }
        for (_, data) in sections {
            bytes.extend_from_slice(data);
        }
        bytes
    }

    unsafe extern "C" fn resolve_file(
        request: *const Z42FileRequest,
        writer: *mut Z42FileWriter,
        context: *mut c_void,
    ) -> i32 {
        let request = unsafe { &*request };
        let name = unsafe { CStr::from_ptr(request.file_name) }.to_str().unwrap();
        let calls = unsafe { &mut *context.cast::<Vec<(String, u32)>>() };
        calls.push((name.to_owned(), request.kind));
        let mut bytes = match name {
            "entry.runtimeconfig.toml" => b"[properties]".to_vec(),
            "entry.zbc.deps" => b"a.zpkg\nb.zpkg\ntool.zbc".to_vec(),
            "tool.zbc.deps" => b"extra.zpkg".to_vec(),
            "z42.core.zpkg" => packed_zpkg("z42.core"),
            "a.zpkg" => packed_zpkg("a"),
            "b.zpkg" => packed_zpkg("b"),
            "extra.zpkg" => packed_zpkg("extra"),
            "tool.zbc" => crate::loader::test_support::empty_zbc(),
            "bad.runtimeconfig.toml" => vec![0xff],
            _ => return 0,
        };
        let write = unsafe { z42_file_write(writer, bytes.as_ptr(), bytes.len()) };
        bytes.fill(0);
        std::hint::black_box(bytes);
        if write == Z42HostStatus::Ok { 1 } else { -1 }
    }

    let _guard = test_lock();
    reset_host();
    let mut config = default_config();
    let mut calls = Vec::<(String, u32)>::new();
    config.file_resolver = Some(resolve_file);
    config.file_resolver_user_data = (&mut calls as *mut Vec<_>).cast();
    config.struct_size = std::mem::offset_of!(Z42HostConfig, file_resolver_user_data) as u32;
    let (validated, _) = unsafe { abi_config::validate(&config) }.unwrap();
    assert!(validated.file_resolver.is_none());
    config.struct_size = std::mem::size_of::<Z42HostConfig>() as u32;

    let mut host = ptr::null_mut();
    assert_eq!(unsafe { z42_host_initialize(&config, &mut host) }, Z42HostStatus::Ok);
    let mut module = ptr::null_mut();
    let entry = crate::loader::test_support::empty_zbc();
    let status = unsafe {
        z42_host_load_bytes(host, c"entry.zbc".as_ptr(), entry.as_ptr(), entry.len(), &mut module)
    };
    let error = unsafe { z42_host_last_error(host) };
    assert_eq!(status, Z42HostStatus::Ok, "{}", if error.message.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(error.message) }.to_string_lossy().into_owned()
    });
    let expected = [
        ("entry.runtimeconfig.toml", FileKind::RuntimeConfig),
        ("entry.zbc.deps", FileKind::BytecodeDependencies),
        ("z42.core.zpkg", FileKind::Zpkg),
        ("a.zpkg", FileKind::Zpkg),
        ("b.zpkg", FileKind::Zpkg),
        ("tool.zbc", FileKind::Zbc),
        ("tool.zbc.deps", FileKind::BytecodeDependencies),
        ("extra.zpkg", FileKind::Zpkg),
    ];
    assert_eq!(calls, expected.map(|(name, kind)| (name.to_owned(), kind as u32)));

    let status = unsafe {
        z42_host_load_bytes(host, c"bad.zbc".as_ptr(), entry.as_ptr(), entry.len(), &mut module)
    };
    assert_eq!(status, Z42HostStatus::BadZbc);
    assert!(module.is_null());
    assert_eq!(unsafe { z42_host_shutdown(host) }, Z42HostStatus::Ok);
}
