use super::*;
use std::ffi::c_void;

#[derive(Default)]
struct Calls {
    arguments: usize,
    resolves: usize,
}
unsafe fn calls<'a>(raw: *mut abi::Call) -> &'a mut Calls {
    unsafe { &mut *(*raw).context.cast() }
}
unsafe extern "C" fn argument(raw: *mut abi::Call, _: usize, _: *mut abi::Value) -> u32 {
    unsafe { calls(raw) }.arguments += 1;
    abi::ERROR
}
unsafe extern "C" fn returned(_: *mut abi::Call, _: *const abi::Value) -> u32 {
    abi::OK
}
unsafe extern "C" fn raised(_: *mut abi::Call, _: abi::Text, _: abi::Text) -> u32 {
    abi::ERROR
}
unsafe extern "C" fn invoke(
    _: *mut abi::Call,
    _: abi::Text,
    _: *const abi::Value,
    _: usize,
    _: *mut abi::Value,
) -> u32 {
    abi::ERROR
}
unsafe extern "C" fn resolve(
    raw: *mut abi::Call,
    _: abi::Text,
    _: abi::Text,
    _: *mut *mut c_void,
) -> u32 {
    unsafe { calls(raw) }.resolves += 1;
    abi::ERROR
}
static HOST: abi::HostApi = abi::HostApi {
    version: 1,
    size: std::mem::size_of::<abi::HostApi>() as u32,
    capabilities: 0,
    argument,
    return_value: returned,
    raise: raised,
    invoke_script: invoke,
    resolve_symbol: resolve,
};

#[test]
fn linked_adapter_preserves_interface_and_stays_lazy_when_backend_features_are_unified() {
    assert_eq!(MODULE.function_count, LINKED_MODULE.function_count);
    let mut calls = Calls::default();
    let mut raw = abi::Call {
        host: &HOST,
        context: (&mut calls as *mut Calls).cast(),
        state: std::ptr::null_mut(),
    };
    assert_eq!(
        unsafe { LINKED_FUNCTIONS[0].invoke.unwrap()(&mut raw) },
        abi::ERROR
    );
    assert_eq!(
        (calls.arguments, calls.resolves),
        (0, 1),
        "adapter must ask the host for its implementation"
    );
    #[cfg(feature = "backend")]
    {
        assert_eq!(
            unsafe { FUNCTIONS[0].invoke.unwrap()(&mut raw) },
            abi::ERROR
        );
        assert_eq!(
            (calls.arguments, calls.resolves),
            (1, 1),
            "full implementation must consume its own arguments"
        );
    }
    for (full, linked) in FUNCTIONS.iter().zip(LINKED_FUNCTIONS) {
        assert_eq!(full.result, linked.result);
        assert_eq!(full.flags, linked.flags);
        assert_eq!(full.parameters.len, linked.parameters.len);
        assert_eq!(
            unsafe { std::slice::from_raw_parts(full.name.data, full.name.len) },
            unsafe { std::slice::from_raw_parts(linked.name.data, linked.name.len) }
        );
    }
}
