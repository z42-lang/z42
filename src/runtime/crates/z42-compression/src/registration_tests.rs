use super::*;

#[test]
fn declaration_has_signatures_without_call_wrappers_or_state() {
    assert_eq!(DECLARATION.function_count, 12);
    assert!(
        DECLARATIONS
            .iter()
            .all(|function| function.invoke.is_none())
    );
    assert!(DECLARATION.create_state.is_none() && DECLARATION.destroy_state.is_none());
    #[cfg(feature = "backend")]
    for (implementation, declaration) in FUNCTIONS.iter().zip(DECLARATIONS) {
        assert!(implementation.invoke.is_some());
        assert_eq!(implementation.result, declaration.result);
        assert_eq!(implementation.flags, declaration.flags);
        assert_eq!(implementation.parameters.len, declaration.parameters.len);
        assert_eq!(
            unsafe {
                std::slice::from_raw_parts(implementation.name.data, implementation.name.len)
            },
            unsafe { std::slice::from_raw_parts(declaration.name.data, declaration.name.len) }
        );
    }
}
