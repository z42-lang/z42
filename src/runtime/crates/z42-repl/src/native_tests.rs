use super::*;

#[derive(Default)]
struct Mock {
    argument: &'static str,
    result_kind: u32,
    result_bits: u64,
    invokes: usize,
    resolves: usize,
}
unsafe fn mock<'a>(call: *mut abi::Call) -> &'a mut Mock {
    unsafe { &mut *(*call).context.cast::<Mock>() }
}
unsafe extern "C" fn argument(call: *mut abi::Call, _: usize, output: *mut abi::Value) -> u32 {
    let mock = unsafe { mock(call) };
    unsafe {
        *output = abi::Value {
            kind: abi::STR,
            reserved: 0,
            bits: 0,
            data: mock.argument.as_ptr(),
            len: mock.argument.len(),
        };
    }
    abi::OK
}
unsafe extern "C" fn returned(call: *mut abi::Call, value: *const abi::Value) -> u32 {
    let mock = unsafe { mock(call) };
    mock.result_kind = unsafe { (*value).kind };
    mock.result_bits = unsafe { (*value).bits };
    abi::OK
}
unsafe extern "C" fn raised(_: *mut abi::Call, _: abi::Text, _: abi::Text) -> u32 {
    abi::ERROR
}
unsafe extern "C" fn invoke(
    call: *mut abi::Call,
    _: abi::Text,
    _: *const abi::Value,
    _: usize,
    _: *mut abi::Value,
) -> u32 {
    unsafe { mock(call) }.invokes += 1;
    abi::ERROR
}
unsafe extern "C" fn resolve(
    call: *mut abi::Call,
    _: abi::Text,
    _: abi::Text,
    _: *mut *mut c_void,
) -> u32 {
    unsafe { mock(call) }.resolves += 1;
    abi::ERROR
}
const fn host(capabilities: u64) -> abi::HostApi {
    abi::HostApi {
        version: 1,
        size: std::mem::size_of::<abi::HostApi>() as u32,
        capabilities,
        argument,
        return_value: returned,
        raise: raised,
        invoke_script: invoke,
        resolve_symbol: resolve,
    }
}
static NONE: abi::HostApi = host(0);
static TERMINAL: abi::HostApi =
    host(abi::CAP_TERMINAL | abi::CAP_NATIVE_LOADING | abi::CAP_FILESYSTEM);
static NO_FILES: abi::HostApi = host(abi::CAP_TERMINAL | abi::CAP_NATIVE_LOADING);

struct State(*mut c_void);
impl State {
    fn new() -> Self {
        Self(unsafe { create() })
    }
    fn call(&self, host: &'static abi::HostApi, mock: &mut Mock) -> abi::Call {
        abi::Call {
            host,
            context: (mock as *mut Mock).cast(),
            state: self.0,
        }
    }
    fn session(&self) -> &Session {
        unsafe { &*self.0.cast::<Session>() }
    }
}
impl Drop for State {
    fn drop(&mut self) {
        unsafe { destroy(self.0) };
    }
}

#[test]
fn callback_null_arguments_are_rejected() {
    let line = CString::new("Con").unwrap();
    let key = CString::new("tab").unwrap();
    assert!(complete(std::ptr::null_mut(), line.as_ptr(), 3).is_null());
    assert!(key_edit(std::ptr::null_mut(), key.as_ptr(), line.as_ptr(), 3).is_null());
    let state = State::new();
    let mut mock = Mock::default();
    let mut call = state.call(&TERMINAL, &mut mock);
    assert!(complete((&mut call as *mut abi::Call).cast(), std::ptr::null(), 0).is_null());
    assert!(
        key_edit(
            (&mut call as *mut abi::Call).cast(),
            std::ptr::null(),
            line.as_ptr(),
            0
        )
        .is_null()
    );
    assert!(
        key_edit(
            (&mut call as *mut abi::Call).cast(),
            key.as_ptr(),
            std::ptr::null(),
            0
        )
        .is_null()
    );
    assert_eq!(mock.invokes, 0);
}

#[test]
fn unconfigured_callbacks_do_not_invoke_and_errors_remain_no_action() {
    let state = State::new();
    let mut mock = Mock {
        argument: "Missing.Callback",
        ..Mock::default()
    };
    let mut call = state.call(&TERMINAL, &mut mock);
    let line = CString::new("Con").unwrap();
    let key = CString::new("tab").unwrap();
    let context = (&mut call as *mut abi::Call).cast();
    assert!(complete(context, line.as_ptr(), 3).is_null());
    assert!(key_edit(context, key.as_ptr(), line.as_ptr(), 3).is_null());
    assert_eq!(mock.invokes, 0);
    assert_eq!(unsafe { set_completer(&mut call) }, abi::OK);
    assert_eq!(unsafe { set_key_editor(&mut call) }, abi::OK);
    assert!(complete(context, line.as_ptr(), 3).is_null());
    assert!(key_edit(context, key.as_ptr(), line.as_ptr(), 3).is_null());
    assert_eq!(mock.invokes, 2);
}

#[test]
fn registration_state_keeps_session_configuration_separate() {
    let first = State::new();
    let second = State::new();
    let mut mock = Mock {
        argument: "One.Complete",
        ..Mock::default()
    };
    assert_eq!(
        unsafe { set_completer(&mut first.call(&NONE, &mut mock)) },
        abi::OK
    );
    assert_eq!(
        lock(&first.session().completer).as_deref(),
        Some("One.Complete")
    );
    assert!(lock(&second.session().completer).is_none());
    mock.argument = "One.Edit";
    assert_eq!(
        unsafe { set_key_editor(&mut first.call(&NONE, &mut mock)) },
        abi::OK
    );
    assert!(lock(&second.session().key_editor).is_none());
    mock.argument = "";
    assert_eq!(
        unsafe { set_completer(&mut first.call(&NONE, &mut mock)) },
        abi::OK
    );
    assert!(lock(&first.session().completer).is_none());
}

#[test]
fn portable_configuration_does_not_load_a_terminal_and_history_requires_files() {
    let state = State::new();
    let mut mock = Mock {
        argument: "if\nwhile",
        ..Mock::default()
    };
    assert_eq!(
        unsafe { set_keywords(&mut state.call(&NONE, &mut mock)) },
        abi::OK
    );
    assert_eq!(mock.resolves, 0);
    assert_eq!(
        unsafe { supports_terminal(&mut state.call(&NONE, &mut mock)) },
        abi::OK
    );
    assert_eq!(mock.result_bits, 0);
    assert_eq!(
        unsafe { supports_history(&mut state.call(&NO_FILES, &mut mock)) },
        abi::OK
    );
    assert_eq!(mock.result_bits, 0);
    assert_eq!(
        unsafe { supports_history(&mut state.call(&TERMINAL, &mut mock)) },
        abi::OK
    );
    assert_eq!(mock.result_bits, 1);
}

#[test]
fn callback_strings_are_freed_by_their_owner_and_null_is_ignored() {
    free_str(CString::new("candidate").unwrap().into_raw());
    free_str(std::ptr::null_mut());
}
