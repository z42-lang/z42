//! Tests for `startup.rs` — the seam between config *resolution* and config
//! *consumption*.
//!
//! fix-phase1-knobs-bypass-config (2026-09-05): `src/config/` already tested
//! resolution thoroughly (env / `[runtime]` / app-config / CLI precedence), but
//! nothing tested that the resolved value is what the boot path actually *uses*.
//! Three Phase-1 knobs (`Z42_LIBS` / `Z42_PATH` / `Z42_CRASH_DIR`) had drifted
//! into re-reading `std::env::var` directly, so every non-env layer was silently
//! dropped — `--info` reported `[user-config]` for `libs` on the same run where
//! the lookup said "not found". These tests pin the seam, not the resolution.

use super::startup::{resolve_libs_dir, resolve_module_paths};
use std::path::PathBuf;
use z42::config::RuntimeConfig;

fn cfg_with_libs(dir: Option<PathBuf>) -> RuntimeConfig {
    RuntimeConfig { libs_dir: dir, ..Default::default() }
}

/// The regression: a `libs_dir` that came from *any* layer (config file, CLI
/// `--set`, app sidecar — not just env) must be honoured. Before the fix this
/// returned the cwd-fallback because only `std::env::var("Z42_LIBS")` was read.
#[test]
fn resolve_libs_dir_honours_resolved_config_not_just_env() {
    let tmp = std::env::temp_dir().join("z42-startup-libs-test");
    std::fs::create_dir_all(&tmp).expect("create temp libs dir");

    let cfg = cfg_with_libs(Some(tmp.clone()));
    assert_eq!(
        resolve_libs_dir(&cfg).as_deref(),
        Some(tmp.as_path()),
        "resolved cfg.libs_dir must win — a config-file/CLI layer value is not \
         reachable via std::env::var and would be dropped",
    );

    let _ = std::fs::remove_dir(&tmp);
}

/// An explicit-but-nonexistent value must fall *through* to the search path
/// rather than being returned as-is. This mirrors the pre-fix env behaviour
/// (`if p.is_dir()`), so honouring more layers did not loosen validation.
#[test]
fn resolve_libs_dir_falls_through_when_configured_dir_is_missing() {
    let cfg = cfg_with_libs(Some(PathBuf::from("/definitely/not/a/real/dir/z42")));
    let got = resolve_libs_dir(&cfg);
    assert_ne!(
        got.as_deref(),
        Some(std::path::Path::new("/definitely/not/a/real/dir/z42")),
        "a non-directory override must not be returned; fall through to the search path",
    );
}

/// `None` means "no override" — resolution proceeds to the binary-relative /
/// cwd fallbacks. We assert only that it does not panic and does not invent the
/// override, since what it finds depends on where the test binary lives.
#[test]
fn resolve_libs_dir_without_override_does_not_panic() {
    let _ = resolve_libs_dir(&cfg_with_libs(None));
}

/// `module_path` entries reach the search list. Also pins that splitting is the
/// *config layer's* job: `cfg.module_path` is already a `Vec<PathBuf>` split on
/// the platform separator, whereas the pre-fix code split the raw env string on
/// `':'` unconditionally — wrong on Windows.
#[test]
fn resolve_module_paths_honours_resolved_config() {
    let tmp = std::env::temp_dir().join("z42-startup-modpath-test");
    std::fs::create_dir_all(&tmp).expect("create temp module dir");

    let cfg = RuntimeConfig { module_path: vec![tmp.clone()], ..Default::default() };
    let paths = resolve_module_paths(&cfg);
    assert!(
        paths.contains(&tmp),
        "configured module path missing from {paths:?}",
    );

    let _ = std::fs::remove_dir(&tmp);
}

/// Non-existent entries are filtered out (pre-existing `is_dir()` policy).
#[test]
fn resolve_module_paths_skips_missing_dirs() {
    let bogus = PathBuf::from("/definitely/not/a/real/dir/z42-modules");
    let cfg = RuntimeConfig { module_path: vec![bogus.clone()], ..Default::default() };
    assert!(!resolve_module_paths(&cfg).contains(&bogus));
}
