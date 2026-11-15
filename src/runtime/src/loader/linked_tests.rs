use super::*;
unsafe extern "C" fn first(_: *const c_char) -> *const c_void {
    std::ptr::dangling::<u8>().cast()
}
unsafe extern "C" fn missing(_: *const c_char) -> *const c_void {
    std::ptr::null()
}

#[test]
fn static_resolution_is_lazy_idempotent_and_does_not_fall_back_on_missing_exports() {
    let providers = Providers::default();
    assert!(providers.resolve("independent.example", "entry").is_none());
    providers.register("independent.example", first).unwrap();
    providers.register("independent.example", first).unwrap();
    assert!(providers.register("independent.example", missing).is_err());
    assert_eq!(
        providers.resolve("independent.example", "entry").unwrap().unwrap(),
        std::ptr::dangling::<u8>().cast_mut().cast()
    );
    assert!(providers.resolve("z42_other", "entry").is_none());
    let null_resolver = Providers::default();
    null_resolver.register("independent.example", missing).unwrap();
    assert!(null_resolver.resolve("independent.example", "entry").unwrap().is_err());
    providers.register("z42_unknown_library", first).unwrap();
    for name in ["", "../example", "example\\entry", "nul\0name"] {
        assert!(providers.register(name, first).is_err());
    }
}

#[test]
fn linked_source_policy_is_checked_before_resolving_symbols() {
    use super::super::{NativeLoading, NativeOrigin};
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    unsafe extern "C" fn counted(_: *const c_char) -> *const c_void {
        CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::ptr::dangling::<u8>().cast()
    }
    let providers = Providers::default();
    providers.register_source("static", counted, NativeOrigin::StaticLink).unwrap();
    providers.register_source("shared", counted, NativeOrigin::SharedLink).unwrap();
    assert!(providers.register_source("static", counted, NativeOrigin::SharedLink).is_err());
    let mut policy = NativeLoading { allow_static: false, allow_shared: false, ..Default::default() };
    for name in ["static", "shared"] {
        assert!(providers.resolve_with_policy(name, "entry", &policy).unwrap().is_err());
    }
    assert_eq!(CALLS.load(std::sync::atomic::Ordering::Relaxed), 0);
    policy.allow_shared = true;
    assert!(providers.resolve_with_policy("shared", "entry", &policy).unwrap().is_ok());
    assert!(providers.resolve_with_policy("static", "entry", &policy).unwrap().is_err());
    assert_eq!(CALLS.load(std::sync::atomic::Ordering::Relaxed), 1);
}
