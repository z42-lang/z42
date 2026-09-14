//! Unit + integration tests for the embedding API.
//!
//! Spec: docs/spec/archive/2026-05-10-add-embedding-api/specs/embedding-host-api/spec.md
//!       (Requirement: 测试覆盖)
//!
//! All tests serialize on `HOST_TEST_LOCK` because the embedded VM is a
//! process singleton — concurrent tests would race on `state::HOST`.
//! The end-to-end `load_invoke_hello_world` test is gated on
//! `cfg(z42_have_embedding_hello)` so the suite still builds when
//! the z42c toolchain / stdlib isn't built yet (e.g. CI agents that
//! haven't run `xtask build stdlib`).

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::ptr;
use std::sync::{Mutex, OnceLock};

use super::config::{Z42HostConfig, Z42_HOST_ABI_VERSION};
use super::error::Z42HostStatus;
use super::*;

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

/// Force the singleton back to "uninitialized" so a test starts clean.
/// Called at the top of every test; tolerates both "already off" and
/// "leftover from previous test".
fn reset_host() {
    // Repeated shutdowns are fine; second one returns NotInit which we
    // simply ignore.
    let _ = unsafe { z42_host_shutdown(state::HOST_SENTINEL as *mut Z42Host) };
}

fn default_config() -> Z42HostConfig {
    Z42HostConfig {
        abi_version: Z42_HOST_ABI_VERSION,
        reserved: 0,
        exec_mode: config::Z42ExecMode::Interp as i32,
        heap_initial_bytes: 0,
        heap_max_bytes: 0,
        stdout_sink: None,
        stderr_sink: None,
        sink_user_data: ptr::null_mut(),
        search_paths: ptr::null(),
        zpkg_resolver: None,
        zpkg_resolver_user_data: ptr::null_mut(),
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

    let status = unsafe { z42_host_shutdown(state::HOST_SENTINEL as *mut Z42Host) };
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
    cfg.exec_mode = config::Z42ExecMode::Jit as i32;
    let mut h: *mut Z42Host = ptr::null_mut();
    let status = unsafe { z42_host_initialize(&cfg, &mut h) };

    #[cfg(feature = "jit")]
    {
        assert_eq!(status, Z42HostStatus::Ok);
        assert_eq!(
            unsafe { z42_host_shutdown(h) },
            Z42HostStatus::Ok
        );
    }
    #[cfg(not(feature = "jit"))]
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
            state::HOST_SENTINEL as *mut Z42Host,
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
struct StdoutCapture {
    bytes: Mutex<Vec<u8>>,
}

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

/// Project root under cargo, used to locate `artifacts/build/libraries/dist/release/`.
fn project_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// `CString` form of the libs dir; `OnceLock` so the search-paths array
/// in `Z42HostConfig` can hold a stable pointer.
fn libs_dir_cstring() -> &'static CString {
    static LIBS_DIR: OnceLock<CString> = OnceLock::new();
    LIBS_DIR.get_or_init(|| {
        let p = project_root().join("artifacts/build/libraries/dist/release");
        CString::new(p.to_string_lossy().as_bytes()).expect("libs dir contains no NUL")
    })
}

#[test]
#[cfg(z42_have_embedding_hello)]
fn load_invoke_hello_world() {
    let _g = test_lock();
    reset_host();

    // Skip cleanly if `dotnet build` hasn't produced the corelib zpkg.
    let libs_dir = project_root().join("artifacts/build/libraries/dist/release/z42.core.zpkg");
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
        reserved: 0,
        exec_mode: config::Z42ExecMode::Interp as i32,
        heap_initial_bytes: 0,
        heap_max_bytes: 0,
        stdout_sink: Some(host_stdout_sink_callback),
        stderr_sink: None,
        sink_user_data: user_data,
        search_paths: search_paths.as_ptr(),
        zpkg_resolver: None,
        zpkg_resolver_user_data: ptr::null_mut(),
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
    let user_data = capture as *const Mutex<Vec<u8>> as *mut c_void;
    let libs_path = libs_dir_cstring();
    let search_paths: [*const c_char; 2] = [libs_path.as_ptr(), ptr::null()];
    let cfg = Z42HostConfig {
        abi_version: Z42_HOST_ABI_VERSION,
        reserved: 0,
        exec_mode: config::Z42ExecMode::Interp as i32,
        heap_initial_bytes: 0,
        heap_max_bytes: 0,
        stdout_sink: Some(host_simple_capture_sink),
        stderr_sink: None,
        sink_user_data: user_data,
        search_paths: search_paths.as_ptr(),
        zpkg_resolver: None,
        zpkg_resolver_user_data: ptr::null_mut(),
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
    if !project_root().join("artifacts/build/libraries/dist/release/z42.core.zpkg").is_file() {
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
    if !project_root().join("artifacts/build/libraries/dist/release/z42.core.zpkg").is_file() {
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
    if !project_root().join("artifacts/build/libraries/dist/release/z42.core.zpkg").is_file() {
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
            msg.contains("intentional embedding-test failure"),
            "expected exception message to be propagated, got {msg}"
        );
    });
}

#[test]
#[cfg(z42_have_embedding_hello)]
fn sink_called_in_correct_order_for_multiple_lines() {
    let _g = test_lock();
    reset_host();
    if !project_root().join("artifacts/build/libraries/dist/release/z42.core.zpkg").is_file() {
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

/// Load the stdlib bytes from `artifacts/build/libraries/dist/release/`. Returns `None` if
/// the stdlib isn't built yet (run `xtask build stdlib`) — tests skip in that case.
fn load_stdlib_bytes(name: &str) -> Option<Vec<u8>> {
    let path = project_root().join("artifacts/build/libraries/dist/release").join(name);
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
    let status = unsafe {
        z42_zpkg_read_namespaces(
            core.as_ptr(),
            core.len(),
            Some(ns_collect),
            &mut out as *mut Vec<String> as *mut c_void,
        )
    };
    assert_eq!(status, Z42HostStatus::Ok);
    // z42.core.zpkg resolves by its package name ("z42.core", the prelude
    // key) plus the namespaces it declares (Std, ...) — exactly the keys a
    // hand-maintained index would otherwise have to encode (and drift from).
    assert!(out.iter().any(|n| n == "z42.core"), "expected z42.core in {out:?}");
    assert!(out.iter().any(|n| n == "Std"), "expected Std in {out:?}");
}

#[test]
fn zpkg_read_namespaces_rejects_garbage() {
    let garbage = b"NOTAZPKG".to_vec();
    let status = unsafe {
        z42_zpkg_read_namespaces(
            garbage.as_ptr(),
            garbage.len(),
            Some(ns_collect),
            ptr::null_mut(),
        )
    };
    assert_eq!(status, Z42HostStatus::BadZbc);
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
        reserved: 0,
        exec_mode: config::Z42ExecMode::Interp as i32,
        heap_initial_bytes: 0,
        heap_max_bytes: 0,
        stdout_sink: Some(host_simple_capture_sink),
        stderr_sink: None,
        sink_user_data: user_data,
        search_paths: ptr::null(),
        zpkg_resolver: None,
        zpkg_resolver_user_data: ptr::null_mut(),
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

static C_HOOK_ZPKGS: OnceLock<HashMap<String, Vec<u8>>> = OnceLock::new();

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
        reserved: 0,
        exec_mode: config::Z42ExecMode::Interp as i32,
        heap_initial_bytes: 0,
        heap_max_bytes: 0,
        stdout_sink: Some(host_simple_capture_sink),
        stderr_sink: None,
        sink_user_data: user_data,
        search_paths: ptr::null(),
        zpkg_resolver: Some(c_hook_resolver_fn),
        zpkg_resolver_user_data: ptr::null_mut(),
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

    let libs_dir = project_root().join("artifacts/build/libraries/dist/release");
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
        reserved: 0,
        exec_mode: config::Z42ExecMode::Interp as i32,
        heap_initial_bytes: 0,
        heap_max_bytes: 0,
        stdout_sink: Some(host_simple_capture_sink),
        stderr_sink: None,
        sink_user_data: user_data,
        search_paths: search_paths.as_ptr(),
        zpkg_resolver: None,
        zpkg_resolver_user_data: ptr::null_mut(),
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
        reserved: 0,
        exec_mode: config::Z42ExecMode::Interp as i32,
        heap_initial_bytes: 0,
        heap_max_bytes: 0,
        stdout_sink: None,
        stderr_sink: None,
        sink_user_data: ptr::null_mut(),
        search_paths: ptr::null(),
        zpkg_resolver: None,
        zpkg_resolver_user_data: ptr::null_mut(),
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
        reserved: 0,
        exec_mode: config::Z42ExecMode::Interp as i32,
        heap_initial_bytes: 0,
        heap_max_bytes: 0,
        stdout_sink: None,
        stderr_sink: None,
        sink_user_data: ptr::null_mut(),
        search_paths: ptr::null(),
        zpkg_resolver: None,
        zpkg_resolver_user_data: ptr::null_mut(),
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
    if !project_root().join("artifacts/build/libraries/dist/release/z42.core.zpkg").is_file() {
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
