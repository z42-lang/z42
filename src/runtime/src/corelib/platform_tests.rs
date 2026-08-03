//! `Std.Platform` builtin unit tests.

use super::*;
use crate::metadata::Value;
use crate::vm_context::VmContext;

#[test]
fn os_returns_consts_value() {
    let ctx = VmContext::new();
    let Value::Str(os) = builtin_platform_os(&ctx, &[]).unwrap()
        else { panic!("expected Str"); };
    assert_eq!(os, std::env::consts::OS.into());
    assert!(!os.is_empty(), "os string must not be empty");
}

#[test]
fn arch_returns_consts_value() {
    let ctx = VmContext::new();
    let Value::Str(arch) = builtin_platform_arch(&ctx, &[]).unwrap()
        else { panic!("expected Str"); };
    assert_eq!(arch, std::env::consts::ARCH.into());
    assert!(!arch.is_empty());
}

#[test]
fn family_returns_consts_value() {
    let ctx = VmContext::new();
    let Value::Str(family) = builtin_platform_family(&ctx, &[]).unwrap()
        else { panic!("expected Str"); };
    assert_eq!(family, std::env::consts::FAMILY.into());
}

#[test]
fn os_kind_matches_current_os() {
    let ctx = VmContext::new();
    let Value::I64(kind) = builtin_platform_os_kind(&ctx, &[]).unwrap()
        else { panic!("expected I64"); };
    // Verify the value matches the known mapping for this build target.
    #[cfg(target_os = "linux")]
    assert_eq!(kind, 1, "linux → 1");
    #[cfg(target_os = "macos")]
    assert_eq!(kind, 2, "macos → 2");
    #[cfg(target_os = "windows")]
    assert_eq!(kind, 3, "windows → 3");
    #[cfg(target_os = "android")]
    assert_eq!(kind, 4, "android → 4");
    #[cfg(target_os = "ios")]
    assert_eq!(kind, 5, "ios → 5");
    #[cfg(target_arch = "wasm32")]
    assert_eq!(kind, 6, "wasm → 6");
    #[cfg(target_os = "freebsd")]
    assert_eq!(kind, 7, "freebsd → 7");

    // Whatever the host is, kind should be one of the known values.
    assert!((0..=7).contains(&kind), "kind {} out of expected range", kind);
}

#[test]
fn arch_kind_matches_current_arch() {
    let ctx = VmContext::new();
    let Value::I64(kind) = builtin_platform_arch_kind(&ctx, &[]).unwrap()
        else { panic!("expected I64"); };
    #[cfg(target_arch = "x86_64")]
    assert_eq!(kind, 1);
    #[cfg(target_arch = "aarch64")]
    assert_eq!(kind, 2);
    #[cfg(target_arch = "wasm32")]
    assert_eq!(kind, 3);
    #[cfg(target_arch = "x86")]
    assert_eq!(kind, 4);
}

/// Extract a `Vec<String>` from a builtin that returns `string[]`.
fn str_array(v: Value) -> Vec<String> {
    let Value::Array(rc) = v else { panic!("expected Array"); };
    let arr = rc.borrow();
    arr.to_boxed_vec()
        .iter()
        .map(|e| match e {
            Value::Str(s) => s.to_string(),
            other => panic!("expected Str element, got {other:?}"),
        })
        .collect()
}

#[test]
fn caps_reports_threads_off_wasm() {
    let ctx = VmContext::new();
    let caps = str_array(builtin_platform_caps(&ctx, &[]).unwrap());
    // threads = real OS-thread support, compiled in everywhere except wasm.
    #[cfg(not(target_arch = "wasm32"))]
    assert!(caps.contains(&"threads".to_string()), "non-wasm build must report threads: {caps:?}");
    #[cfg(target_arch = "wasm32")]
    assert!(!caps.contains(&"threads".to_string()), "wasm build must not report threads: {caps:?}");
}

#[test]
fn caps_reports_jit_when_feature_enabled() {
    let ctx = VmContext::new();
    let caps = str_array(builtin_platform_caps(&ctx, &[]).unwrap());
    #[cfg(feature = "jit")]
    assert!(caps.contains(&"jit".to_string()), "jit build must report jit cap: {caps:?}");
    #[cfg(not(feature = "jit"))]
    assert!(!caps.contains(&"jit".to_string()), "non-jit build must not report jit cap: {caps:?}");
}

#[test]
fn exec_modes_always_has_interp_and_matches_features() {
    let ctx = VmContext::new();
    let modes = str_array(builtin_platform_exec_modes(&ctx, &[]).unwrap());
    assert!(modes.contains(&"interp".to_string()), "interp is always available: {modes:?}");
    #[cfg(feature = "jit")]
    assert!(modes.contains(&"jit".to_string()), "jit build lists jit mode: {modes:?}");
    #[cfg(not(feature = "jit"))]
    assert!(!modes.contains(&"jit".to_string()), "non-jit build omits jit mode: {modes:?}");
}
