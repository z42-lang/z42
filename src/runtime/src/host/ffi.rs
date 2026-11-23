//! Embedding / Hosting API — Tier 1 C ABI implementation.
//!
//! Spec: docs/internals/src/runtime/embedding.md
//!       变更 add-embedding-api/
//!
//! H1 scope: lifecycle (initialize / shutdown / sinks / last_error).
//! H2 scope: load_zbc / resolve_entry / invoke + stdout sink VM wiring.
//!
//! Sibling: `crate::interop` (interop — native code registering types into z42).

use super::{ZpkgResolver, config, marshal, read_zpkg_namespaces, state};

use std::os::raw::{c_char, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::sync::Arc;

use super::config::Z42HostConfig;
use crate::host::error::{Z42HostStatus, clear_error, set_error, snapshot_last_error};
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
    result: Result<T, state::Failure>,
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
        let (config, sinks) = match unsafe { config::validate(cfg) } {
            Ok(resolved) => resolved,
            Err(error) => return classify_config_error(error),
        };
        respond(
            "z42_host_initialize",
            state::initialize(config, sinks),
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
        respond("z42_host_shutdown", state::shutdown(host as usize), |_| {})
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_host_load_zbc(
    host: *mut Z42Host,
    bytes: *const u8,
    length: usize,
    out_module: *mut *mut Z42Module,
) -> Z42HostStatus {
    guard(|| {
        let token = host as usize;
        if !matches!(state::current(), Ok(current) if current == token) {
            return set_error(
                Z42HostStatus::NotInit,
                "z42_host_load_zbc: VM is not initialized",
            );
        }
        if !out_module.is_null() {
            unsafe { *out_module = ptr::null_mut() };
        }
        let bytes = if length == 0 {
            &[]
        } else if bytes.is_null() {
            return set_error(
                Z42HostStatus::BadZbc,
                "z42_host_load_zbc: bytes pointer is NULL but length is non-zero",
            );
        } else {
            unsafe { std::slice::from_raw_parts(bytes, length) }
        };
        respond("z42_host_load_zbc", state::load(token, bytes), |module| {
            if !out_module.is_null() {
                unsafe { *out_module = module as *mut Z42Module };
            }
        })
    })
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
        if !matches!(state::current(), Ok(current) if current == token) {
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
            state::resolve(token, module as usize, fqn),
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
        let token = match state::current() {
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
    sink: config::Z42WriteSink,
    user_data: *mut c_void,
) -> Z42HostStatus {
    guard(|| {
        respond(
            "z42_host_set_stdout_sink",
            state::set_sink(host as usize, false, config::output_sink(sink, user_data)),
            |_| {},
        )
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_host_set_stderr_sink(
    host: *mut Z42Host,
    sink: config::Z42WriteSink,
    user_data: *mut c_void,
) -> Z42HostStatus {
    guard(|| {
        respond(
            "z42_host_set_stderr_sink",
            state::set_sink(host as usize, true, config::output_sink(sink, user_data)),
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
/// itself — no separate index file. Stateless: does not require an
/// initialized VM, and never mutates host state. Returns `ERR_BAD_ZBC`
/// when the bytes aren't a parseable zpkg.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn z42_zpkg_read_namespaces(
    bytes: *const u8,
    length: usize,
    visit: config::Z42NamespaceVisitor,
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
        state::current().and_then(|token| state::set_resolver(token, resolver)),
        |_| {},
    )
}

/// Replace the file system for subsequent loads after validating its corelib.
pub fn install_vfs(vfs: Arc<dyn crate::pal::fs::Vfs>) -> Z42HostStatus {
    respond(
        "install_vfs",
        state::current().and_then(|token| state::set_vfs(token, vfs)),
        |_| {},
    )
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn classify_config_error(e: config::ConfigError) -> Z42HostStatus {
    use config::ConfigError as CE;
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
        // tier2-shard-full-coverage: the z42 interpreter recurses on the *native*
        // call stack (one native frame per z42 call — no reified frame stack). This
        // C entry is what every native embedded shell calls (desktop C test-host,
        // iOS Swift, Android JNI), and those callers invoke us on a thread whose
        // stack is far smaller than a desktop main thread's ~8 MB: Android's
        // AndroidJUnitRunner and iOS XCTest threads are ~512 KB–1 MB. So a
        // deeply-but-FINITELY recursive z42 program that runs fine on desktop
        // overflows and SIGSEGVs the whole process ("stack pointer is not in a rw
        // map; likely due to stack overflow"). Running the full mobile corpus
        // surfaced this — the 60-case smoke sample never hit a deep-enough case.
        // Fix: run the VM on a dedicated large-stack thread so the embedded stack
        // budget matches/exceeds desktop.
        let run_vm = move || -> i32 {
            let vfs = crate::pal::fs::default_vfs();
            let opts = crate::host::RunOpts {
                mode: crate::host::default_mode(),
                search_dirs: crate::composition::resolve_search_dirs(
                    &app,
                    libs.as_deref(),
                    &*vfs,
                    crate::config::runtime_config(),
                ),
                vfs,
                resolver: None,
                program_args: prog_args,
                print_stats: false,
                stats_json: false,
                exit_after_run: false,
            };
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
        // 16 MB = 2× the desktop main-thread default (8 MB on Linux/macOS, which
        // the corpus is known to run within). The bound is principled: the
        // crashing cases overflow the ~1 MB Android/iOS runner thread but pass on
        // desktop's 8 MB, so ≥8 MB suffices; 2× is margin. Bigger buys nothing —
        // a program needing >8 MB would already fail on desktop. Virtual
        // reservation on 64-bit (only touched pages commit), well within
        // Android/iOS per-thread limits.
        // runtime-audit P0-4: configurable as `thread-stack-bytes` (default 16M).
        // wasm is single-threaded and enters via z42_wasm (not this C symbol);
        // `run_with_stack` runs inline there.
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
