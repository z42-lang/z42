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
    assert!(providers.resolve(KNOWN_STATIC_LIBRARIES[0], "entry").is_none());
    providers.register(KNOWN_STATIC_LIBRARIES[0], first).unwrap();
    providers.register(KNOWN_STATIC_LIBRARIES[0], first).unwrap();
    assert!(providers.register(KNOWN_STATIC_LIBRARIES[0], missing).is_err());
    assert_eq!(
        providers.resolve(KNOWN_STATIC_LIBRARIES[0], "entry").unwrap().unwrap(),
        std::ptr::dangling::<u8>().cast_mut().cast()
    );
    assert!(providers.resolve("z42_other", "entry").is_none());
    let null_resolver = Providers::default();
    null_resolver.register(KNOWN_STATIC_LIBRARIES[0], missing).unwrap();
    assert!(null_resolver.resolve(KNOWN_STATIC_LIBRARIES[0], "entry").unwrap().is_err());
    for name in ["", "../example", "example\\entry", "nul\0name", "z42_unknown_library"] {
        assert!(providers.register(name, first).is_err());
    }
}
