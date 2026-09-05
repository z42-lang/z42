//! Locate a deployed `z42vm` (+ libs) and run an app's zpkg **directly** — the
//! apphost run path. Originally a standalone `z42-hostrun` crate (extracted from
//! the z42 launcher lib, apphost-to-workload 2026-06-18) so the apphost stub and
//! the launcher trampoline could share one impl; the launcher was since rewritten
//! in z42 (`src/toolchain/launcher/core/*.z42`), leaving the apphost as the sole
//! Rust consumer, so this logic was folded back in as an apphost module
//! (merge-hostrun-into-apphost).
//!
//! "Find a bare z42vm + libs and exec `z42vm app.zpkg`" — no `launcher.zpkg`,
//! no muxer, single VM. Zero external dependencies (pure std).

use std::env;
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

/// Platform `z42vm` filename.
pub fn vm_name() -> &'static str {
    if cfg!(windows) {
        "z42vm.exe"
    } else {
        "z42vm"
    }
}

/// `$Z42_HOME` if set and non-empty.
pub fn env_z42_home() -> Option<PathBuf> {
    match env::var("Z42_HOME") {
        Ok(h) if !h.is_empty() => Some(PathBuf::from(h)),
        _ => None,
    }
}

/// The per-user home `$HOME/.z42` (`%USERPROFILE%\.z42` on Windows).
pub fn home_z42() -> Option<PathBuf> {
    for key in ["HOME", "USERPROFILE"] {
        if let Ok(home) = env::var(key) {
            if !home.is_empty() {
                return Some(PathBuf::from(home).join(".z42"));
            }
        }
    }
    None
}

/// A located z42vm + libs for running a deployed app's zpkg **directly**
/// (apphost run path). There is no `launcher.zpkg`: the apphost bypasses the
/// launcher core and runs `z42vm app.zpkg` itself.
#[derive(Debug, Clone)]
pub struct AppRuntime {
    pub vm: PathBuf,
    pub libs: PathBuf,
}

/// Probe a directory for a runnable z42vm (+ adjacent `libs`). z42vm may sit
/// directly in `<dir>` (installed layout) or in `<dir>/bin` (portable package).
/// Does **not** require `launcher.zpkg` — the apphost runs the app's zpkg directly.
pub fn probe_app_runtime(dir: &Path) -> Option<AppRuntime> {
    let direct = dir.join(vm_name());
    let nested = dir.join("bin").join(vm_name());
    let vm = if direct.exists() {
        direct
    } else if nested.exists() {
        nested
    } else {
        return None;
    };
    Some(AppRuntime { vm, libs: dir.join("libs") })
}

/// `$Z42_PORTABLE_VM` if set and non-empty. Points at a z42vm — either the
/// binary itself or a directory containing it (`<dir>/z42vm` or `<dir>/bin/z42vm`).
pub fn env_portable_vm() -> Option<PathBuf> {
    match env::var("Z42_PORTABLE_VM") {
        Ok(v) if !v.is_empty() => Some(PathBuf::from(v)),
        _ => None,
    }
}

/// Derive `{vm, libs}` from a `$Z42_PORTABLE_VM` hint. The hint may be the vm
/// binary or a directory; libs is `<bin-parent>/libs` when the vm sits in a
/// `bin/` dir, else `<vm-parent>/libs`.
fn runtime_from_portable_vm(hint: &Path) -> Option<AppRuntime> {
    let vm = if hint.is_file() {
        hint.to_path_buf()
    } else if hint.is_dir() {
        let direct = hint.join(vm_name());
        let nested = hint.join("bin").join(vm_name());
        if direct.is_file() {
            direct
        } else if nested.is_file() {
            nested
        } else {
            return None;
        }
    } else {
        return None;
    };
    let parent = vm.parent().unwrap_or_else(|| Path::new("."));
    let libs = if parent.file_name() == Some(std::ffi::OsStr::new("bin")) {
        parent.parent().unwrap_or(parent).join("libs")
    } else {
        parent.join("libs")
    };
    Some(AppRuntime { vm, libs })
}

/// SDK-internal colocated bootstrap: if `$Z42_PORTABLE_VM` is **unset** and a
/// z42vm sits next to `exe_dir` (the SDK package layout — `{exe_dir}/z42vm` for
/// an apphost in `bin/`, or `{exe_dir}/bin/z42vm` for one at the package root),
/// set `$Z42_PORTABLE_VM` to it. The exe-colocated lookup lives here (not as a
/// `resolve_app_runtime` tier) so that the package's own ABI-matched vm is used
/// by **this** apphost AND inherited by any SDK app it spawns. No-op when the
/// var is already set or no colocated vm exists.
pub fn ensure_portable_vm(exe_dir: &Path) {
    if env::var_os("Z42_PORTABLE_VM").is_some() {
        return;
    }
    if let Some(rt) = probe_app_runtime(exe_dir) {
        // SAFETY: called once at apphost startup before any threads spawn.
        env::set_var("Z42_PORTABLE_VM", &rt.vm);
    }
}

/// apphost run-path resolution: locate a z42vm + libs to run the app's zpkg
/// directly. Most-local-wins order (align-bin-z42vm-probe, 2026-06-21):
///   ① `$Z42_PORTABLE_VM`           (explicit override / SDK-colocated vm — set
///                                    by [`ensure_portable_vm`])
///   ② local: walk up from `exe_dir`, `<d>/.z42/launcher` then `<d>/.z42`
///                                    (apphost-relative project venv)
///   ③ user:   `$HOME/.z42/launcher`
///   ④ system: `$Z42_HOME/launcher`  (lowest — explicit overrides go via ①)
pub fn resolve_app_runtime(exe_dir: &Path) -> Option<AppRuntime> {
    resolve_app_runtime_in(
        env_portable_vm().as_deref(),
        env_z42_home().as_deref(),
        exe_dir,
        home_z42().as_deref(),
    )
}

/// Pure form of [`resolve_app_runtime`] with the env roots injected (tests).
/// `env_home` = `$Z42_HOME` (system); `sys_home` = `$HOME/.z42` (user).
pub fn resolve_app_runtime_in(
    portable_vm: Option<&Path>,
    env_home: Option<&Path>,
    exe_dir: &Path,
    sys_home: Option<&Path>,
) -> Option<AppRuntime> {
    // ① $Z42_PORTABLE_VM — explicit override, also how an SDK package's own
    //    colocated vm is selected (set by `ensure_portable_vm` at startup).
    if let Some(hint) = portable_vm {
        if let Some(rt) = runtime_from_portable_vm(hint) {
            return Some(rt);
        }
    }
    // ② local (most specific): exe's dir upward, `<d>/.z42/launcher` then `<d>/.z42`.
    let mut cur = Some(exe_dir);
    while let Some(d) = cur {
        let dotz42 = d.join(".z42");
        if let Some(rt) = probe_app_runtime(&dotz42.join("launcher")) {
            return Some(rt);
        }
        if let Some(rt) = probe_app_runtime(&dotz42) {
            return Some(rt);
        }
        cur = d.parent();
    }
    // ③ user home: $HOME/.z42/launcher.
    if let Some(h) = sys_home {
        if let Some(rt) = probe_app_runtime(&h.join("launcher")) {
            return Some(rt);
        }
    }
    // ④ system: $Z42_HOME/launcher (lowest — an explicit choice belongs in ①).
    if let Some(h) = env_home {
        if let Some(rt) = probe_app_runtime(&h.join("launcher")) {
            return Some(rt);
        }
    }
    None
}

/// Exec `z42vm <app_zpkg> -- <argv>` directly (apphost run path) with
/// `Z42_LIBS` set, and propagate the child exit code.
///
/// The app's `<stem>.runtimeconfig.toml` sidecar is **not** looked up here:
/// z42vm derives it from the app path itself (app-config-follows-the-app,
/// 2026-09-05). Computing the path only to hand it back to the thing that can
/// compute it was pure duplication of a convention that belongs in one place —
/// and the apphost's job is "find z42vm and hand it the app", nothing more. No `launcher.zpkg`, no
/// muxer, single VM. Never returns.
pub fn exec_app(rt: &AppRuntime, app_zpkg: &Path, argv: &[String]) -> ! {
    let mut cmd = Command::new(&rt.vm);
    cmd.arg(app_zpkg);
    if rt.libs.is_dir() {
        cmd.env("Z42_LIBS", &rt.libs);
    }
    if !argv.is_empty() {
        cmd.arg("--");
        cmd.args(argv);
    }
    match cmd.status() {
        Ok(status) => exit(status.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("apphost: failed to launch z42vm ({}): {e}", rt.vm.display());
            exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    /// A fresh, unique temp dir (no external `tempfile` dep — zero-dependency).
    fn temp_dir(tag: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let d = env::temp_dir().join(format!("z42-hostrun-test-{}-{}-{}", std::process::id(), tag, n));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Materialize a runtime at `dir` (installed- or portable-style). Writes a
    /// `launcher.zpkg` too (harmless for the app-runtime probe, which ignores it).
    fn make_runtime(dir: &Path, portable: bool) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("launcher.zpkg"), b"zpkg").unwrap();
        fs::create_dir_all(dir.join("libs")).unwrap();
        if portable {
            fs::create_dir_all(dir.join("bin")).unwrap();
            fs::write(dir.join("bin").join(vm_name()), b"vm").unwrap();
        } else {
            fs::write(dir.join(vm_name()), b"vm").unwrap();
        }
    }

    #[test]
    fn probe_app_runtime_needs_no_launcher_zpkg() {
        // apphost run path requires only z42vm (+ libs), NOT launcher.zpkg.
        let d = temp_dir("app-nolaunch");
        fs::write(d.join(vm_name()), b"vm").unwrap();
        fs::create_dir_all(d.join("libs")).unwrap();
        let rt = probe_app_runtime(&d).expect("apphost probe finds z42vm+libs without launcher.zpkg");
        assert_eq!(rt.vm, d.join(vm_name()));
        assert_eq!(rt.libs, d.join("libs"));
    }

    #[test]
    fn probe_app_missing_is_none() {
        let d = temp_dir("app-none");
        assert!(probe_app_runtime(&d).is_none());
    }

    // Most-local-wins ordering: local `.z42` (②) beats both user $HOME (③) and
    // system $Z42_HOME (④), even when all three exist. (2026-06-21)
    #[test]
    fn local_beats_user_and_system() {
        let env_home = temp_dir("env");   // $Z42_HOME (system, ④)
        let local_base = temp_dir("local");
        let sys = temp_dir("sys");        // $HOME/.z42 (user, ③)
        make_runtime(&env_home.join("launcher"), false);
        make_runtime(&local_base.join(".z42").join("launcher"), false);
        make_runtime(&sys.join("launcher"), false);
        let rt = resolve_app_runtime_in(None, Some(&env_home), &local_base, Some(&sys)).expect("found");
        assert!(rt.vm.starts_with(&local_base), "local .z42 wins over $HOME and $Z42_HOME");
    }

    // User $HOME/.z42 (③) beats system $Z42_HOME (④) when there's no local .z42.
    #[test]
    fn user_home_beats_system_z42_home() {
        let env_home = temp_dir("env2");  // $Z42_HOME (④)
        let sys = temp_dir("sys-user");   // $HOME/.z42 (③)
        let exe_dir = temp_dir("exe-nolocal");
        make_runtime(&env_home.join("launcher"), false);
        make_runtime(&sys.join("launcher"), false);
        let rt = resolve_app_runtime_in(None, Some(&env_home), &exe_dir, Some(&sys)).expect("found");
        assert!(rt.vm.starts_with(&sys), "$HOME/.z42 (user) wins over $Z42_HOME (system)");
    }

    #[test]
    fn local_beats_system() {
        let local_base = temp_dir("local2");
        let sys = temp_dir("sys2");
        make_runtime(&local_base.join(".z42").join("launcher"), false);
        make_runtime(&sys.join("launcher"), false);
        let rt = resolve_app_runtime_in(None, None, &local_base, Some(&sys)).expect("found");
        assert!(rt.vm.starts_with(&local_base));
    }

    #[test]
    fn local_walk_upward_finds_ancestor() {
        let base = temp_dir("walk");
        make_runtime(&base.join(".z42").join("launcher"), false);
        let exe_dir = base.join("dist").join("nested");
        fs::create_dir_all(&exe_dir).unwrap();
        let rt = resolve_app_runtime_in(None, None, &exe_dir, None).expect("found");
        assert!(rt.vm.starts_with(&base));
    }

    #[test]
    fn local_portable_style_dotz42() {
        // `<repo>/.z42` is itself the package root (launcher-at-package-root).
        let base = temp_dir("walk-port");
        make_runtime(&base.join(".z42"), true);
        let exe_dir = base.join("dist");
        fs::create_dir_all(&exe_dir).unwrap();
        let rt = resolve_app_runtime_in(None, None, &exe_dir, None).expect("found");
        assert!(rt.vm.starts_with(base.join(".z42")));   // .z42/bin/z42vm
    }

    #[test]
    fn system_fallback() {
        let local_base = temp_dir("local3");
        let sys = temp_dir("sys3");
        make_runtime(&sys.join("launcher"), false);
        let rt = resolve_app_runtime_in(None, None, &local_base, Some(&sys)).expect("found");
        assert!(rt.vm.starts_with(&sys));
    }

    #[test]
    fn nothing_found_is_none() {
        let local_base = temp_dir("local4");
        assert!(resolve_app_runtime_in(None, None, &local_base, None).is_none());
    }

    // align-bin-z42vm-probe (2026-06-21): SDK-internal apps reach their own
    // colocated vm via $Z42_PORTABLE_VM (tier ①), not a dedicated exe-dir probe.

    #[test]
    fn portable_vm_bin_layout_derives_libs() {
        // hint = `<pkg>/bin/z42vm` → libs = `<pkg>/libs`.
        let pkg = temp_dir("pv-bin");
        fs::create_dir_all(pkg.join("bin")).unwrap();
        let vm = pkg.join("bin").join(vm_name());
        fs::write(&vm, b"vm").unwrap();
        fs::create_dir_all(pkg.join("libs")).unwrap();
        let rt = resolve_app_runtime_in(Some(&vm), None, &temp_dir("pv-exe"), None)
            .expect("portable vm resolves");
        assert_eq!(rt.vm, vm);
        assert_eq!(rt.libs, pkg.join("libs"));
    }

    #[test]
    fn portable_vm_sibling_layout_derives_libs() {
        // hint = `<dir>/z42vm` (not in bin/) → libs = `<dir>/libs`.
        let dir = temp_dir("pv-sib");
        let vm = dir.join(vm_name());
        fs::write(&vm, b"vm").unwrap();
        let rt = resolve_app_runtime_in(Some(&vm), None, &temp_dir("pv-exe2"), None)
            .expect("portable vm resolves");
        assert_eq!(rt.vm, vm);
        assert_eq!(rt.libs, dir.join("libs"));
    }

    #[test]
    fn portable_vm_dir_hint_resolves_vm() {
        // hint = a directory → locate z42vm inside (`<dir>/bin/z42vm`).
        let pkg = temp_dir("pv-dir");
        fs::create_dir_all(pkg.join("bin")).unwrap();
        fs::write(pkg.join("bin").join(vm_name()), b"vm").unwrap();
        let rt = resolve_app_runtime_in(Some(&pkg), None, &temp_dir("pv-exe3"), None)
            .expect("dir hint resolves vm");
        assert_eq!(rt.vm, pkg.join("bin").join(vm_name()));
    }

    #[test]
    fn portable_vm_beats_z42_home() {
        let env_home = temp_dir("pv-home");
        make_runtime(&env_home.join("launcher"), false);
        let pkg = temp_dir("pv-win");
        fs::create_dir_all(pkg.join("bin")).unwrap();
        let vm = pkg.join("bin").join(vm_name());
        fs::write(&vm, b"vm").unwrap();
        let rt = resolve_app_runtime_in(Some(&vm), Some(&env_home), &temp_dir("pv-exe4"), None)
            .expect("found");
        assert_eq!(rt.vm, vm, "$Z42_PORTABLE_VM (tier ①) wins over $Z42_HOME");
    }

    #[test]
    fn portable_vm_missing_file_falls_through() {
        // A stale/nonexistent hint must not stop resolution falling to $Z42_HOME.
        let env_home = temp_dir("pv-stale-home");
        make_runtime(&env_home.join("launcher"), false);
        let bogus = temp_dir("pv-stale").join("nope").join(vm_name());
        let rt = resolve_app_runtime_in(Some(&bogus), Some(&env_home), &temp_dir("pv-exe5"), None)
            .expect("falls through to $Z42_HOME");
        assert!(rt.vm.starts_with(&env_home));
    }
}
