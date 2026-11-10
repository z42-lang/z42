use super::*;

#[derive(Default)]
struct Host {
    result: Option<Value>,
    error: Option<(String, String)>,
}
unsafe fn host<'a>(call: *mut abi::Call) -> &'a mut Host {
    unsafe { &mut *(*call).context.cast() }
}
unsafe extern "C" fn argument(_: *mut abi::Call, _: usize, _: *mut abi::Value) -> u32 {
    abi::ERROR
}
unsafe extern "C" fn returned(call: *mut abi::Call, value: *const abi::Value) -> u32 {
    unsafe { host(call) }.result = Some(unsafe { decode(&*value) }.unwrap());
    abi::OK
}
unsafe extern "C" fn raised(call: *mut abi::Call, class: abi::Text, message: abi::Text) -> u32 {
    let text = |value: abi::Text| {
        String::from_utf8(unsafe { span(value.data, value.len) }.unwrap().to_vec()).unwrap()
    };
    unsafe { host(call) }.error = Some((text(class), text(message)));
    abi::ERROR
}
unsafe extern "C" fn invoke(
    _: *mut abi::Call,
    _: abi::Text,
    _: *const abi::Value,
    _: usize,
    _: *mut abi::Value,
) -> u32 {
    abi::THROWN
}
unsafe extern "C" fn resolve(
    _: *mut abi::Call,
    _: abi::Text,
    _: abi::Text,
    _: *mut *mut c_void,
) -> u32 {
    abi::ERROR
}
const fn api() -> abi::HostApi {
    abi::HostApi {
        version: 1,
        size: std::mem::size_of::<abi::HostApi>() as u32,
        capabilities: 0,
        argument,
        return_value: returned,
        raise: raised,
        invoke_script: invoke,
        resolve_symbol: resolve,
    }
}
static API: abi::HostApi = api();
fn call(host: &mut Host) -> abi::Call {
    abi::Call {
        host: &API,
        context: (host as *mut Host).cast(),
        state: std::ptr::null_mut(),
    }
}

#[test]
fn dispatch_copies_owned_results_and_preserves_unicode_and_scalar_bits() {
    let mut host = Host::default();
    let mut raw = call(&mut host);
    let status = unsafe {
        dispatch(&mut raw, |_| {
            Ok(Value::Strings(vec!["中文".into(), "".into()]))
        })
    };
    assert_eq!(status, abi::OK);
    assert!(matches!(host.result.take(), Some(Value::Strings(values)) if values == ["中文", ""]));
    for value in [
        Value::Void,
        Value::Null,
        Value::I64(i64::MIN),
        Value::F64(-0.0),
        Value::Bool(true),
        Value::Bytes(vec![0, 255]),
    ] {
        let encoded = Encoded::new(&value);
        let decoded = unsafe { decode(&encoded.raw) }.unwrap();
        match (value, decoded) {
            (Value::Void, Value::Void) | (Value::Null, Value::Null) => {}
            (Value::I64(left), Value::I64(right)) => assert_eq!(left, right),
            (Value::F64(left), Value::F64(right)) => assert_eq!(left.to_bits(), right.to_bits()),
            (Value::Bool(left), Value::Bool(right)) => assert_eq!(left, right),
            (Value::Bytes(left), Value::Bytes(right)) => assert_eq!(left, right),
            other => panic!("roundtrip changed value: {other:?}"),
        }
    }
}

#[test]
fn malformed_values_are_rejected_without_reading_bad_spans() {
    let mut raw = abi::Value::scalar(abi::STR, 0);
    raw.len = 1;
    assert!(unsafe { decode(&raw) }.is_err());
    raw.len = usize::MAX;
    assert!(unsafe { decode(&raw) }.is_err());
    raw.data = [0xff].as_ptr();
    raw.len = 1;
    assert!(unsafe { decode(&raw) }.is_err());
    let nested = abi::Value::scalar(abi::STR_ARRAY, 0);
    raw.kind = abi::STR_ARRAY;
    raw.data = (&nested as *const abi::Value).cast();
    assert!(
        unsafe { decode(&raw) }.is_err(),
        "nested arrays must be rejected before recursion"
    );
    raw.kind = 999;
    assert!(unsafe { decode(&raw) }.is_err());
}

#[test]
fn dispatch_contains_panics_preserves_script_exceptions_and_validates_host_abi() {
    let mut host = Host::default();
    let mut raw = call(&mut host);
    assert_eq!(
        unsafe { dispatch(&mut raw, |_| panic!("provider panic")) },
        abi::ERROR
    );
    assert_eq!(host.error.take().unwrap().1, "panic in native module");
    assert_eq!(
        unsafe {
            dispatch(&mut raw, |_| {
                Err(Error::new("Std.IO.IOException", "native error"))
            })
        },
        abi::ERROR
    );
    assert_eq!(
        host.error.take().unwrap(),
        ("Std.IO.IOException".into(), "native error".into())
    );
    assert_eq!(
        unsafe { dispatch(&mut raw, |call| call.invoke("Script.Throw", &[])) },
        abi::THROWN
    );
    let mut invalid = api();
    invalid.version = 999;
    raw.host = &invalid;
    assert_eq!(
        unsafe { dispatch(&mut raw, |_| panic!("must not invoke")) },
        abi::ERROR
    );
    let prefix = [999_u32, 8];
    raw.host = prefix.as_ptr().cast();
    assert_eq!(
        unsafe { dispatch(&mut raw, |_| panic!("truncated host must be rejected")) },
        abi::ERROR
    );
}
