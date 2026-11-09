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
    assert!(providers.resolve("example", "entry").is_none());
    providers.register("example", first).unwrap();
    providers.register("example", first).unwrap();
    assert!(providers.register("example", missing).is_err());
    assert_eq!(
        providers.resolve("example", "entry").unwrap().unwrap(),
        std::ptr::dangling::<u8>().cast_mut().cast()
    );
    providers.register("missing", missing).unwrap();
    assert!(providers.resolve("missing", "entry").unwrap().is_err());
    assert!(providers.resolve("example", "nul\0entry").unwrap().is_err());
    for name in ["", "../example", "example\\entry", "nul\0name"] {
        assert!(providers.register(name, first).is_err());
    }
}
