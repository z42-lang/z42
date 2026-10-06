//! Unit tests for `native::ext` — search path resolution + lazy library loading.

use super::ext::*;

#[test]
fn native_search_paths_includes_default_sdk_layout() {
    // Always returns *something* because we always probe alongside the
    // current_exe; even if the env var is unset, the SDK layout fallbacks
    // are appended.
    let paths = native_search_paths();
    assert!(!paths.is_empty(), "expected at least one search path");
}

#[test]
fn native_search_paths_includes_exec_dir_for_cargo_target_layout() {
    // 2026-05-24 dev-infra: <exec_dir> itself is now a search path so cargo
    // `target/<profile>/libz42_*.dylib` is discovered without manual
    // `ln -sf ../libz42_compression.dylib release/native/libz42_compression.dylib`.
    let exe = std::env::current_exe().expect("current_exe");
    let exec_dir = exe.parent().expect("exec dir").to_path_buf();
    let paths = native_search_paths();
    assert!(
        paths.iter().any(|p| p == &exec_dir),
        "expected <exec_dir> ({}) among search paths, got {:?}",
        exec_dir.display(),
        paths,
    );
}

/// runtime-config-phase2 (2026-06-03): `Z42_NATIVE_PATH` parsing moved
/// to `crate::config::parse_native_search_paths`, covered by
/// `config::tests::from_getter_native_path_splits_on_platform_separator`.
/// `native_search_paths()` now concatenates the parsed list with SDK-
/// relative fallbacks; smoke-test that it returns a non-empty list
/// containing the running binary's exec_dir (the fallback) so the
/// delegator + fallback wiring still works end-to-end.
#[test]
fn native_search_paths_includes_exec_dir_fallback() {
    let paths = native_search_paths();
    let exec_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .expect("test binary must have an exec_dir");
    assert!(
        paths.iter().any(|p| p == &exec_dir),
        "expected exec_dir fallback ({}) among search paths, got {paths:?}",
        exec_dir.display(),
    );
}

#[test]
fn ext_builtin_table_register_and_lookup() {
    use crate::corelib::NativeFn;
    use crate::metadata::Value;
    use crate::vm_context::VmContext;
    use anyhow::Result;

    fn dummy_fn(_ctx: &VmContext, _args: &[Value]) -> Result<Value> {
        Ok(Value::I64(42))
    }
    let f: NativeFn = dummy_fn;

    let mut table = ExtBuiltinTable::default();
    let id0 = table.register("__test_foo", f);
    let id1 = table.register("__test_bar", f);
    assert_ne!(id0, id1);

    assert_eq!(table.lookup_id("__test_foo"), Some(id0));
    assert_eq!(table.lookup_id("__test_bar"), Some(id1));
    assert_eq!(table.lookup_id("__nonexistent"), None);

    assert!(table.dispatch(id0).is_some());
    assert!(table.dispatch(id1).is_some());
    assert!(table.dispatch(99).is_none());

    // Idempotent: re-register same name returns existing id, doesn't swap.
    let id0_again = table.register("__test_foo", f);
    assert_eq!(id0_again, id0);
}

#[cfg(not(feature = "bundled-compression"))]
#[test]
fn unknown_builtin_names_never_trigger_a_library_load() {
    let ctx = crate::vm_context::VmContext::new();
    assert!(!ensure_lib_for(&ctx, "__no_such_builtin"));
}

#[cfg(not(feature = "bundled-compression"))]
#[test]
fn a_library_is_looked_for_once_per_vm() {
    // The first miss looks for libz42_compression (it may or may not sit next
    // to the test binary); a later miss for another of its builtins does not
    // look again.
    let ctx = crate::vm_context::VmContext::new();
    if ensure_lib_for(&ctx, "__deflate_compress") {
        assert!(ctx.core.ext_builtins.lock().lookup_id("__deflate_compress").is_some());
    }
    assert!(!ensure_lib_for(&ctx, "__zstd_compress"), "second miss for the same library must not retry");
}
