//! Embedding / Hosting API — Tier 1 C ABI implementation.
//!
//! Spec: docs/internals/src/runtime/embedding.md
//!       变更 add-embedding-api/
//!
//! H1 scope: lifecycle (initialize / shutdown / sinks / last_error).
//! H2 scope: load_zbc / resolve_entry / invoke + stdout sink VM wiring.
//!
//! Sibling: `crate::interop` (interop — native code registering types into z42).

use super::{ZpkgResolver, abi_config, marshal, session};

use std::os::raw::{c_char, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::sync::Arc;

use super::abi_config::{Z42FileWriter, Z42HostConfig};
use crate::host::error::{Z42HostStatus, clear_error, set_error, snapshot_last_error};
use crate::loader::program::read_zpkg_namespaces;
use z42_abi::{Z42Error, Z42Value};

/// Opaque token for the currently initialized embedding session.
#[repr(C)]
pub struct Z42Host {
    _private: [u8; 0],
}

/// Opaque module token; its backing record belongs to the host session's module table.
#[repr(C)]
pub struct Z42Module {
    _private: [u8; 0],
}

/// Opaque resolved-entry token; its backing record belongs to the host session's entry table.
#[repr(C)]
pub struct Z42Entry {
    _private: [u8; 0],
}

/// Wrap an extern "C" function body so a Rust panic becomes
/// `Z42_HOST_ERR_INTERNAL` with a stable message rather than UB.
fn guard<F>(f: F) -> Z42HostStatus
where
    F: FnOnce() -> Z42HostStatus,
{
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(status) => status,
        Err(_) => set_error(
            Z42HostStatus::Internal,
            "z42_host: internal panic crossed FFI boundary",
        ),
    }
}

fn respond<T>(
    operation: &str,
    result: Result<T, session::Failure>,
    publish: impl FnOnce(T),
) -> Z42HostStatus {
    match result {
        Ok(value) => {
            publish(value);
            clear_error();
            Z42HostStatus::Ok
        }
        Err(error) => set_error(error.status, format!("{operation}: {}", error.message)),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_host_initialize(
    cfg: *const Z42HostConfig,
    out_host: *mut *mut Z42Host,
) -> Z42HostStatus {
    guard(|| {
        if out_host.is_null() {
            return set_error(
                Z42HostStatus::BadConfig,
                "z42_host_initialize: out_host pointer is NULL",
            );
        }
        unsafe { *out_host = ptr::null_mut() };
        let (config, sinks) = match unsafe { abi_config::validate(cfg) } {
            Ok(resolved) => resolved,
            Err(error) => return classify_config_error(error),
        };
        respond(
            "z42_host_initialize",
            session::initialize(config, sinks),
            |token| {
                unsafe { *out_host = token as *mut Z42Host };
            },
        )
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_host_shutdown(host: *mut Z42Host) -> Z42HostStatus {
    guard(|| {
        if host.is_null() {
            return set_error(
                Z42HostStatus::BadConfig,
                "z42_host_shutdown: host handle is NULL",
            );
        }
        respond("z42_host_shutdown", session::shutdown(host as usize), |_| {})
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_host_load_zbc(
    host: *mut Z42Host,
    bytes: *const u8,
    length: usize,
    out_module: *mut *mut Z42Module,
) -> Z42HostStatus {
    unsafe { z42_host_load_bytes(host, ptr::null(), bytes, length, out_module) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_host_load_bytes(
    host: *mut Z42Host,
    name: *const c_char,
    bytes: *const u8,
    length: usize,
    out_module: *mut *mut Z42Module,
) -> Z42HostStatus {
    guard(|| {
        if !out_module.is_null() {
            unsafe { *out_module = ptr::null_mut() };
        }
        let name = if name.is_null() {
            None
        } else {
            match unsafe { std::ffi::CStr::from_ptr(name) }.to_str() {
                Ok(name) if !name.is_empty() => Some(name),
                _ => return set_error(Z42HostStatus::BadConfig, "expected a nonempty UTF-8 file name"),
            }
        };
        let token = host as usize;
        if !matches!(session::current(), Ok(current) if current == token) {
            return set_error(
                Z42HostStatus::NotInit,
                "z42_host_load_bytes: VM is not initialized",
            );
        }
        let bytes = if length == 0 {
            &[]
        } else if bytes.is_null() {
            return set_error(
                Z42HostStatus::BadZbc,
                "z42_host_load_bytes: bytes pointer is NULL but length is non-zero",
            );
        } else {
            unsafe { std::slice::from_raw_parts(bytes, length) }
        };
        respond("z42_host_load_bytes", session::load(token, name, bytes), |module| {
            if !out_module.is_null() {
                unsafe { *out_module = module as *mut Z42Module };
            }
        })
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_file_write(
    writer: *mut Z42FileWriter,
    bytes: *const u8,
    length: usize,
) -> Z42HostStatus {
    let Some(writer) = (unsafe { writer.as_mut() }) else {
        return Z42HostStatus::BadConfig;
    };
    let bytes = if length == 0 {
        Some(Vec::new())
    } else {
        unsafe { abi_config::copy_resolver_bytes(bytes, length) }
    };
    if writer.submit(bytes) {
        Z42HostStatus::Ok
    } else {
        Z42HostStatus::BadConfig
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_host_resolve_entry(
    host: *mut Z42Host,
    module: *mut Z42Module,
    fqn: *const c_char,
    out_entry: *mut *mut Z42Entry,
) -> Z42HostStatus {
    guard(|| {
        let token = host as usize;
        if !matches!(session::current(), Ok(current) if current == token) {
            return set_error(
                Z42HostStatus::NotInit,
                "z42_host_resolve_entry: VM is not initialized",
            );
        }
        if !out_entry.is_null() {
            unsafe { *out_entry = ptr::null_mut() };
        }
        if fqn.is_null() {
            return set_error(
                Z42HostStatus::BadConfig,
                "z42_host_resolve_entry: fqn pointer is NULL",
            );
        }
        if module.is_null() {
            return set_error(
                Z42HostStatus::EntryNotFound,
                "z42_host_resolve_entry: module handle is NULL",
            );
        }
        let Ok(fqn) = (unsafe { std::ffi::CStr::from_ptr(fqn) }).to_str() else {
            return set_error(
                Z42HostStatus::BadConfig,
                "z42_host_resolve_entry: fqn is not valid UTF-8",
            );
        };
        respond(
            "z42_host_resolve_entry",
            session::resolve(token, module as usize, fqn),
            |entry| {
                if !out_entry.is_null() {
                    unsafe { *out_entry = entry as *mut Z42Entry };
                }
            },
        )
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_host_invoke(
    entry: *mut Z42Entry,
    args: *const Z42Value,
    argc: usize,
    out_result: *mut Z42Value,
) -> Z42HostStatus {
    guard(|| {
        let token = match session::current() {
            Ok(token) => token,
            Err(error) => {
                return set_error(error.status, format!("z42_host_invoke: {}", error.message));
            }
        };
        if entry.is_null() {
            return set_error(
                Z42HostStatus::EntryNotFound,
                "z42_host_invoke: entry handle is NULL",
            );
        }
        let args = if argc == 0 {
            &[]
        } else if args.is_null() {
            return set_error(
                Z42HostStatus::ArgMismatch,
                "z42_host_invoke: args pointer is NULL but n is non-zero",
            );
        } else {
            unsafe { std::slice::from_raw_parts(args, argc) }
        };
        let result = marshal::invoke(token, entry as usize, args.iter());
        respond("z42_host_invoke", result, |value| {
            if !out_result.is_null() {
                unsafe { *out_result = value };
            }
        })
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_host_set_stdout_sink(
    host: *mut Z42Host,
    sink: abi_config::Z42WriteSink,
    user_data: *mut c_void,
) -> Z42HostStatus {
    guard(|| {
        respond(
            "z42_host_set_stdout_sink",
            session::set_sink(host as usize, false, abi_config::output_sink(sink, user_data)),
            |_| {},
        )
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_host_set_stderr_sink(
    host: *mut Z42Host,
    sink: abi_config::Z42WriteSink,
    user_data: *mut c_void,
) -> Z42HostStatus {
    guard(|| {
        respond(
            "z42_host_set_stderr_sink",
            session::set_sink(host as usize, true, abi_config::output_sink(sink, user_data)),
            |_| {},
        )
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_host_last_error(_host: *mut Z42Host) -> Z42Error {
    snapshot_last_error()
}

// ── zpkg introspection ──────────────────────────────────────────────────

/// Read the keys a zpkg can be resolved by — its **package name** (e.g.
/// `"z42.core"`; the prelude is requested by package name) plus every
/// **namespace** it declares in `NSPC` (e.g. `"Std"`, `"Std.IO"`). For
/// each key the runtime calls `visit(key, len, user_data)` once.
///
/// This lets embedding hosts build a resolution map from the package
/// itself. This call requires no session and uses the host's thread-local
/// last-error record. Returns `BadZbc` when the bytes aren't a parseable zpkg.
///
/// # Safety
/// A non-null `bytes` must point to `length` readable bytes for the duration of
/// the call. Each key is borrowed only while `visit` is running. Any data that
/// the callback dereferences via `user_data` must remain valid for the callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_zpkg_read_namespaces(
    bytes: *const u8,
    length: usize,
    visit: abi_config::Z42NamespaceVisitor,
    user_data: *mut c_void,
) -> Z42HostStatus {
    guard(|| {
        let Some(visit) = visit else {
            return set_error(
                Z42HostStatus::BadConfig,
                "z42_zpkg_read_namespaces: visit callback is NULL",
            );
        };
        if bytes.is_null() && length != 0 {
            return set_error(
                Z42HostStatus::BadZbc,
                "z42_zpkg_read_namespaces: bytes pointer is NULL but length is non-zero",
            );
        }
        let slice = if length == 0 {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(bytes, length) }
        };
        match read_zpkg_namespaces(slice) {
            Ok(keys) => {
                // Package name first (the corelib prelude "z42.core" is
                // requested by package name, not a declared namespace),
                // then each NSPC namespace. SAFETY: each `&str` outlives
                // the synchronous callback; the host copies before return.
                for ns in &keys {
                    let b = ns.as_bytes();
                    unsafe { visit(b.as_ptr() as *const c_char, b.len(), user_data) };
                }
                clear_error();
                Z42HostStatus::Ok
            }
            Err(e) => set_error(
                Z42HostStatus::BadZbc,
                format!("z42_zpkg_read_namespaces: cannot read zpkg metadata: {e:#}"),
            ),
        }
    })
}

// ── Tier 2 escape hatch: install a Rust ZpkgResolver post-init ──────────

/// Install a Rust [`ZpkgResolver`] trait object into the running host
/// state, replacing any resolver currently configured (including ones
/// derived from the C `zpkg_resolver` field of `Z42HostConfig`).
///
/// Rust clients can also supply the resolver in their initial configuration.
/// C clients use `Z42HostConfig::zpkg_resolver` at initialization.
///
/// Returns `Z42HostStatus::Ok` on success or `NotInit` if no VM is
/// currently initialized.
pub fn install_zpkg_resolver(resolver: Arc<dyn ZpkgResolver>) -> Z42HostStatus {
    respond(
        "install_zpkg_resolver",
        session::current().and_then(|token| session::set_resolver(token, resolver)),
        |_| {},
    )
}

/// Replace the file system for subsequent loads after validating its corelib.
pub fn install_vfs(vfs: Arc<dyn crate::pal::fs::Vfs>) -> Z42HostStatus {
    respond(
        "install_vfs",
        session::current().and_then(|token| session::set_vfs(token, vfs)),
        |_| {},
    )
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn classify_config_error(e: abi_config::ConfigError) -> Z42HostStatus {
    use abi_config::ConfigError as CE;
    match e {
        CE::NullConfig => set_error(
            Z42HostStatus::BadConfig,
            "z42_host_initialize: cfg pointer is NULL",
        ),
        CE::AbiVersionMismatch { expected, got } => set_error(
            Z42HostStatus::BadConfig,
            format!("z42_host_initialize: abi_version mismatch (expected {expected}, got {got})"),
        ),
        CE::StructTooSmall { min, got } => set_error(
            Z42HostStatus::BadConfig,
            format!(
                "z42_host_initialize: struct_size {got} is smaller than the v2 config ({min} bytes); set it to sizeof(Z42HostConfig)"
            ),
        ),
        CE::UnknownExecMode { raw } => set_error(
            Z42HostStatus::BadConfig,
            format!("z42_host_initialize: unknown exec_mode {raw}"),
        ),
        CE::FeatureOff { mode } => set_error(
            Z42HostStatus::FeatureOff,
            format!(
                "z42_host_initialize: exec_mode {mode:?} requires a feature disabled at compile time"
            ),
        ),
        CE::BadSearchPath { reason } => set_error(
            Z42HostStatus::BadConfig,
            format!("z42_host_initialize: bad search_paths ({reason})"),
        ),
    }
}

// ── One-shot embedded app run (add-embedded-app-run) ─────────────────────────
//
// Run an app.zpkg embedded (self-contained via `crate::host::run_app` — its own
// VmContext, independent of the handle-based load/invoke API above). The C entry
// for native test-host shells (desktop C / Swift / JNI): the same app-run core
// the z42vm binary and `z42-host::run_app` use, so all platforms run apps
// identically. Returns a process-style exit code (0 = ran ok; non-zero on
// load/run error, message on stderr). `entry` / `libs_dir` may be NULL;
// `argv[0..argc]` are forwarded to the app's `GetCommandLineArgs()`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_host_run_app(
    app_zpkg: *const c_char,
    entry: *const c_char,
    libs_dir: *const c_char,
    argc: std::os::raw::c_int,
    argv: *const *const c_char,
) -> i32 {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let cstr = |p: *const c_char| -> Option<String> {
            if p.is_null() {
                None
            } else {
                unsafe { std::ffi::CStr::from_ptr(p) }
                    .to_str()
                    .ok()
                    .map(str::to_owned)
            }
        };
        let app = match cstr(app_zpkg) {
            Some(s) => s,
            None => {
                eprintln!("z42_host_run_app: app_zpkg is NULL");
                return 2;
            }
        };
        let entry_owned = cstr(entry);
        let libs = cstr(libs_dir).map(std::path::PathBuf::from);
        let mut prog_args: Vec<String> = Vec::new();
        if !argv.is_null() && argc > 0 {
            for i in 0..argc as isize {
                let p = unsafe { *argv.offset(i) };
                if let Some(s) = cstr(p) {
                    prog_args.push(s);
                }
            }
        }
        let opts = crate::composition::embedded_app_options(&app, libs.as_deref(), None, prog_args);
        let run_vm = move || -> i32 {
            let result = crate::host::run_app(&app, entry_owned.as_deref(), opts);
            // runtime-audit P0-4: fatal VM error → its report + the fixed code.
            if let Some(report) = crate::exec::stack_guard::take_report() {
                eprintln!("z42_host_run_app: {report}");
                return crate::exec::stack_guard::EXIT_CODE;
            }
            match result {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("z42_host_run_app: {e:#}");
                    1
                }
            }
        };
        // The interpreter uses the native stack. Give it a configured stack
        // independent of the smaller caller stacks on iOS and Android.
        let embed_stack = crate::config::runtime_config().vm_thread_stack_size();
        match crate::pal::thread::run_with_stack("z42-embedded-run", embed_stack, run_vm) {
            Ok(code) => code,
            Err(crate::pal::thread::RunError::Panicked) => {
                eprintln!("z42_host_run_app: VM thread panicked (see stderr above)");
                70
            }
            Err(crate::pal::thread::RunError::Spawn(e)) => {
                eprintln!("z42_host_run_app: spawn VM thread failed: {e}");
                71
            }
        }
    }));
    outcome.unwrap_or_else(|_| {
        eprintln!("z42_host_run_app: internal panic crossed FFI");
        70
    })
}
