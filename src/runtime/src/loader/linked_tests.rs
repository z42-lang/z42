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
