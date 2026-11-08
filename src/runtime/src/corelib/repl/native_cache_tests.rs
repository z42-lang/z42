use super::*;

#[test]
fn free_str_frees_owned_and_ignores_null() {
    // A pointer from `CString::into_raw` (as the trampolines produce) is
    // reclaimed without leak/double-free; null is a no-op.
    let owned = CString::new("candidate").unwrap().into_raw();
    z42vm_free_str(owned);
    z42vm_free_str(std::ptr::null_mut());
}

#[test]
fn lib_filename_is_platform_shaped() {
    let f = crate::pal::dynlib::file_name("z42_repl");
    assert!(f.contains("z42_repl"));
    // libz42_repl.dylib / .so, or z42_repl.dll on Windows.
    assert!(f.ends_with(std::env::consts::DLL_SUFFIX));
}

use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn missing_editor_is_cached_without_repeating_path_policy() {
    let cache = OnceLock::new();
    let calls = AtomicUsize::new(0);
    for _ in 0..2 {
        assert!(loaded_from(&cache, || {
            calls.fetch_add(1, Ordering::Relaxed);
            Vec::new()
        })
        .is_none());
    }
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}
