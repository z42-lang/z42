use super::*;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use z42_rs::module::{self as sdk, Value as NativeValue};

#[test]
fn competing_state_destructors_run_outside_the_instance_registry_lock() {
    use std::sync::{Barrier, OnceLock, atomic::AtomicUsize};
    static START: OnceLock<Barrier> = OnceLock::new();
    static WATCH: AtomicUsize = AtomicUsize::new(0);
    static DROPS: AtomicUsize = AtomicUsize::new(0);
    static LOCKED: AtomicUsize = AtomicUsize::new(0);
    unsafe extern "C" fn create() -> *mut c_void {
        START.get_or_init(|| Barrier::new(2)).wait();
        Box::into_raw(Box::new(0_u8)).cast()
    }
    unsafe extern "C" fn destroy(value: *mut c_void) {
        let pointer = WATCH.load(Ordering::SeqCst) as *const crate::interop::state::State;
        if let Some(state) = unsafe { pointer.as_ref() } {
            if state.instances.try_read().is_none() { LOCKED.fetch_add(1, Ordering::SeqCst); }
        }
        drop(unsafe { Box::from_raw(value.cast::<u8>()) });
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
    static ENTRIES: &[abi::Function] = &[function("__race", &[], abi::VOID, 0, void)];
    static RACE: abi::Module = abi::Module { name: abi::Text::new("native.race"), functions: ENTRIES.as_ptr(),
        function_count: 1, create_state: Some(create), destroy_state: Some(destroy), ..MODULE };
    let ctx = VmContext::new();
    unsafe { crate::interop::register_native_module(&ctx, &RACE) }.unwrap();
    WATCH.store(crate::interop::state::state(&ctx).unwrap() as *const _ as usize, Ordering::SeqCst);
    let workers: Vec<_> = (0..2).map(|_| {
        let core = ctx.core_arc();
        std::thread::spawn(move || {
            let worker = VmContext::new_with_core(core);
            assert_eq!(invoke_export(&worker, Some("native.race"), "__race", &[]).unwrap().unwrap(), None);
        })
    }).collect();
    for worker in workers { worker.join().unwrap(); }
    WATCH.store(0, Ordering::SeqCst);
    assert_eq!(DROPS.load(Ordering::SeqCst), 1);
    assert_eq!(LOCKED.load(Ordering::SeqCst), 0);
    drop(ctx);
    assert_eq!(DROPS.load(Ordering::SeqCst), 2);
}

static DROPPED: AtomicU64 = AtomicU64::new(0);
unsafe extern "C" fn create() -> *mut c_void {
    Box::into_raw(Box::new(AtomicI64::new(0))).cast()
}
unsafe extern "C" fn destroy(state: *mut c_void) {
    drop(unsafe { Box::from_raw(state.cast::<AtomicI64>()) });
    DROPPED.fetch_add(1, Ordering::SeqCst);
}
unsafe extern "C" fn count(raw: *mut abi::Call) -> u32 {
    unsafe {
        sdk::dispatch(raw, |call| {
            Ok(NativeValue::I64(
                call.state::<AtomicI64>()?.fetch_add(1, Ordering::SeqCst) + 1,
            ))
        })
    }
}
unsafe extern "C" fn echo(raw: *mut abi::Call) -> u32 {
    unsafe { sdk::dispatch(raw, |call| Ok(NativeValue::Str(call.text(0)?))) }
}
unsafe extern "C" fn void(raw: *mut abi::Call) -> u32 {
    unsafe { sdk::dispatch(raw, |_| Ok(NativeValue::Void)) }
}
unsafe extern "C" fn null(raw: *mut abi::Call) -> u32 {
    unsafe { sdk::dispatch(raw, |_| Ok(NativeValue::Null)) }
}
unsafe extern "C" fn error(raw: *mut abi::Call) -> u32 {
    unsafe {
        sdk::dispatch(raw, |_| {
            Err(sdk::Error::new("Std.IO.IOException", "owned native error"))
        })
    }
}
unsafe extern "C" fn parked(raw: *mut abi::Call) -> u32 {
    let Some(frame) = (unsafe { frame(raw) }) else {
        return abi::ERROR;
    };
    let parked = frame.ctx.core.threads.parked_count.load(Ordering::Acquire);
    unsafe { sdk::dispatch(raw, |_| Ok(NativeValue::I64(parked as i64))) }
}

const fn function(
    name: &'static str,
    parameters: &'static [u32],
    result: u32,
    flags: u32,
    invoke: abi::Invoke,
) -> abi::Function {
    abi::Function {
        name: abi::Text::new(name),
        parameters: abi::Parameters::new(parameters),
        result,
        flags,
        invoke: Some(invoke),
    }
}
static COUNT_FUNCTIONS: &[abi::Function] = &[function("__native_count", &[], abi::I64, 0, count)];
static COUNT: abi::Module = abi::Module {
    version: 1,
    size: std::mem::size_of::<abi::Module>() as u32,
    name: abi::Text::new("native.count"),
    functions: COUNT_FUNCTIONS.as_ptr(),
    function_count: COUNT_FUNCTIONS.len(),
    create_state: Some(create),
    destroy_state: Some(destroy),
};
static FUNCTIONS: &[abi::Function] = &[
    function("__native_echo", &[abi::STR], abi::STR, 0, echo),
    function("__native_void", &[], abi::VOID, 0, void),
    function("__native_null", &[], abi::OPT_STR, 0, null),
    function("__native_error", &[], abi::VOID, 0, error),
    function("__native_parked", &[], abi::I64, abi::BLOCKING, parked),
    function("__native_unparked", &[], abi::I64, 0, parked),
];
static MODULE: abi::Module = abi::Module {
    version: 1,
    size: std::mem::size_of::<abi::Module>() as u32,
    name: abi::Text::new("native.test"),
    functions: FUNCTIONS.as_ptr(),
    function_count: FUNCTIONS.len(),
    create_state: None,
    destroy_state: None,
};

#[test]
fn crate_module_registers_and_marshals_without_a_runtime_wrapper() {
    let ctx = VmContext::new();
    unsafe { crate::interop::register_native_module(&ctx, &MODULE) }.unwrap();
    let result = invoke_export(
        &ctx,
        Some("native.test"),
        "__native_echo",
        &[Value::Str("你好 native".into())],
    )
    .unwrap()
    .unwrap();
    assert_eq!(result, Some(Value::Str("你好 native".into())));
    assert_eq!(
        invoke_export(&ctx, None, "__native_void", &[])
            .unwrap()
            .unwrap(),
        None
    );
    assert_eq!(
        invoke_export(&ctx, None, "__native_null", &[])
            .unwrap()
            .unwrap(),
        Some(Value::Null)
    );
    assert!(
        invoke_export(&ctx, None, "__native_echo", &[])
            .unwrap()
            .is_err()
    );
    assert!(invoke_export(&ctx, Some("other"), "__native_echo", &[]).is_none());
    assert!(!crate::interop::state::state(&ctx).unwrap().instances.read().contains_key("native.test"));
    let error = invoke_export(&ctx, None, "__native_error", &[])
        .unwrap()
        .unwrap_err();
    assert!(
        matches!(*error, BuiltinError::ExceptionOwned { ref class, ref message }
        if class == "Std.IO.IOException" && message == "owned native error")
    );
}

#[test]
fn module_state_is_per_vm_shared_with_workers_and_destroyed_once() {
    let dropped = DROPPED.load(Ordering::SeqCst);
    let first = VmContext::new();
    let second = VmContext::new();
    for ctx in [&first, &second] {
        unsafe { crate::interop::register_native_module(ctx, &COUNT) }.unwrap();
    }
    let call = |ctx: &VmContext| {
        invoke_export(ctx, Some("native.count"), "__native_count", &[])
            .unwrap()
            .unwrap()
    };
    assert_eq!(call(&first), Some(Value::I64(1)));
    let worker = VmContext::new_with_core(first.core_arc());
    assert_eq!(call(&worker), Some(Value::I64(2)));
    assert_eq!(call(&second), Some(Value::I64(1)));
    unsafe { crate::interop::register_native_module(&first, &COUNT) }.unwrap();
    assert_eq!(call(&first), Some(Value::I64(3)));
    drop(first);
    assert_eq!(DROPPED.load(Ordering::SeqCst), dropped);
    drop(worker);
    drop(second);
    assert_eq!(DROPPED.load(Ordering::SeqCst), dropped + 2);
}

#[test]
fn malformed_registration_does_not_publish_partial_exports() {
    let ctx = VmContext::new();
    let bad = abi::Module {
        version: 999,
        size: std::mem::size_of::<abi::Module>() as u32,
        name: abi::Text::new("native.test"),
        functions: FUNCTIONS.as_ptr(),
        function_count: FUNCTIONS.len(),
        create_state: None,
        destroy_state: None,
    };
    assert!(unsafe { crate::interop::register_native_module(&ctx, &bad) }.is_err());
    assert!(
        crate::interop::resolve_native_module(&ctx, "native.test")
            .unwrap()
            .is_none()
    );
    assert!(unsafe { crate::interop::module::NativeModule::parse(std::ptr::null()) }.is_err());
    unsafe { crate::interop::register_native_module(&ctx, &MODULE) }.unwrap();
    assert!(
        crate::interop::resolve_native_module(&ctx, "native.test")
            .unwrap()
            .is_some()
    );
}

#[test]
fn blocking_native_calls_park_and_restore_the_mutator() {
    let ctx = VmContext::new();
    unsafe { crate::interop::register_native_module(&ctx, &MODULE) }.unwrap();
    assert_eq!(
        invoke_export(&ctx, None, "__native_unparked", &[])
            .unwrap()
            .unwrap(),
        Some(Value::I64(0))
    );
    assert_eq!(
        invoke_export(&ctx, None, "__native_parked", &[])
            .unwrap()
            .unwrap(),
        Some(Value::I64(1))
    );
    assert_eq!(ctx.core.threads.parked_count.load(Ordering::Acquire), 0);
}

#[test]
fn invalid_signatures_and_interface_conflicts_leave_registration_unchanged() {
    let ctx = VmContext::new();
    let bad_signatures = [
        function("__invalid", &[abi::VOID], abi::VOID, 0, void),
        function("__invalid", &[], abi::NULL, 0, void),
        function("__invalid", &[], abi::VOID, 128, void),
        function("__str_length", &[], abi::VOID, 0, void),
    ];
    for entry in &bad_signatures {
        let descriptor = abi::Module {
            functions: entry,
            function_count: 1,
            ..MODULE
        };
        assert!(unsafe { crate::interop::register_native_module(&ctx, &descriptor) }.is_err());
        assert!(
            crate::interop::resolve_native_module(&ctx, "native.test")
                .unwrap()
                .is_none()
        );
    }
    unsafe { crate::interop::register_native_module(&ctx, &MODULE) }.unwrap();
    let incompatible = abi::Module {
        functions: std::ptr::null(),
        function_count: 0,
        ..MODULE
    };
    assert!(unsafe { crate::interop::register_native_module(&ctx, &incompatible) }.is_err());
    assert_eq!(
        crate::interop::resolve_native_module(&ctx, "native.test")
            .unwrap()
            .unwrap()
            .function_count(),
        FUNCTIONS.len()
    );
}

#[test]
fn declaration_loads_once_on_first_call_and_rejects_mismatched_implementations_atomically() {
    use crate::interop::providers::RegistryBuilder;
    use std::sync::atomic::AtomicUsize;
    static LOOKUPS: AtomicUsize = AtomicUsize::new(0);
    static READY: AtomicUsize = AtomicUsize::new(0);
    static IMPLEMENTATIONS: &[abi::Function] =
        &[function("__lazy_echo", &[abi::STR], abi::STR, 0, echo)];
    static DECLARATIONS: &[abi::Function] = &[abi::Function {
        invoke: None,
        ..function("__lazy_echo", &[abi::STR], abi::STR, 0, echo)
    }];
    static DECLARATION: abi::Module = abi::Module {
        name: abi::Text::new("native.lazy.test"),
        functions: DECLARATIONS.as_ptr(),
        function_count: 1,
        ..MODULE
    };
    static GOOD: abi::Module = abi::Module {
        name: abi::Text::new("native.lazy.test"),
        functions: IMPLEMENTATIONS.as_ptr(),
        function_count: 1,
        ..MODULE
    };
    static BAD: abi::Module = abi::Module {
        function_count: 0,
        ..GOOD
    };
    unsafe extern "C" fn entry() -> *const abi::Module {
        if READY.load(Ordering::SeqCst) == 0 {
            &BAD
        } else {
            &GOOD
        }
    }
    unsafe extern "C" fn resolver(name: *const std::ffi::c_char) -> *const c_void {
        LOOKUPS.fetch_add(1, Ordering::SeqCst);
        assert_eq!(
            unsafe { std::ffi::CStr::from_ptr(name) }.to_bytes(),
            b"native.lazy.test_module_v1"
        );
        entry as *const () as *const c_void
    }
    unsafe { crate::host::register_static_native_library("native.lazy.test", resolver) }.unwrap();
    let mut builder = RegistryBuilder::default();
    unsafe { builder.register_declaration(&DECLARATION) }.unwrap();
    let registry = Box::leak(Box::new(builder.freeze()));
    let mut extensions = crate::context::extensions::VmExtensions::new();
    extensions
        .register(crate::interop::state_for_vm(registry))
        .unwrap();
    let ctx = crate::host::vm_defaults::with_config(
        None,
        crate::pal::fs::default_vfs(),
        extensions.freeze(),
        crate::config::runtime_config().vm_config(),
        crate::corelib::object_protocol(),
        crate::exec::engine::EngineRegistry::new(None),
    );
    assert!(
        crate::interop::resolve_native_module(&ctx, "native.lazy.test")
            .unwrap()
            .unwrap()
            .deferred
    );
    assert!(invoke_export(&ctx, Some("native.lazy.test"), "missing", &[]).is_none());
    assert!(
        invoke_export(&ctx, Some("native.lazy.test"), "__lazy_echo", &[])
            .unwrap()
            .is_err()
    );
    assert!(
        invoke_export(
            &ctx,
            Some("native.lazy.test"),
            "__lazy_echo",
            &[Value::I64(1)]
        )
        .unwrap()
        .is_err()
    );
    assert_eq!(LOOKUPS.load(Ordering::SeqCst), 0);
    let arguments = [Value::Str("lazy module".into())];
    assert!(
        invoke_export(&ctx, Some("native.lazy.test"), "__lazy_echo", &arguments)
            .unwrap()
            .is_err()
    );
    assert!(
        crate::interop::resolve_native_module(&ctx, "native.lazy.test")
            .unwrap()
            .unwrap()
            .deferred
    );
    READY.store(1, Ordering::SeqCst);
    assert_eq!(
        invoke_export(&ctx, Some("native.lazy.test"), "__lazy_echo", &arguments)
            .unwrap()
            .unwrap(),
        Some(arguments[0])
    );
    assert_eq!(
        invoke_export(&ctx, None, "__lazy_echo", &arguments)
            .unwrap()
            .unwrap(),
        Some(arguments[0])
    );
    assert_eq!(
        LOOKUPS.load(Ordering::SeqCst),
        2,
        "successful materialization must replace the declaration"
    );
    assert!(
        !crate::interop::resolve_native_module(&ctx, "native.lazy.test")
            .unwrap()
            .unwrap()
            .deferred
    );
}

#[cfg(feature = "interp")]
#[test]
fn blocking_provider_reenters_the_interpreter_and_copies_callback_results() {
    use crate::metadata::{Function, Module};
    use crate::zpkg::ir::{BasicBlock, ExecMode, Terminator};
    let script_function = Function::new(crate::zpkg::ir::Function {
        name: "Test.Callback".into(),
        param_count: 1,
        ret_type: "string[]".into(),
        exec_mode: ExecMode::Interp,
        blocks: vec![BasicBlock {
            label: "entry".into(),
            instructions: vec![],
            terminator: Terminator::Ret { reg: Some(0) },
        }],
        is_static: true,
        visibility: 0,
        method_flags: 0,
        min_arg: 0,
        params_from: 0xff,
        max_reg: 2,
        cold: None,
        reg_types: Box::new([]),
        block_index: Default::default(),
        branch_targets: vec![],
        frame_meta: None,
    });
    let mut void_function = script_function.clone();
    void_function.name = "Test.Void".into();
    void_function.param_count = 0;
    void_function.ret_type = "void".into();
    void_function.blocks[0].terminator = Terminator::Ret { reg: None };
    let ctx = VmContext::with_module(Module::new(
        crate::zpkg::image::ModuleData {
            name: "Test".into(),
            string_pool: vec![],
            classes: vec![],
            func_index: [("Test.Callback".into(), 0), ("Test.Void".into(), 1)]
                .into_iter()
                .collect(),
        },
        vec![script_function, Function::new(void_function)],
        Default::default(),
    ));
    ctx.install_lazy_loader(None, 0);
    unsafe extern "C" fn callback(raw: *mut abi::Call) -> u32 {
        let ctx = unsafe { frame(raw) }.unwrap().ctx;
        assert_eq!(ctx.core.threads.parked_count.load(Ordering::Acquire), 1);
        unsafe {
            sdk::dispatch(raw, |call| {
                if !matches!(call.invoke("Test.Void", &[])?, NativeValue::Void) {
                    return Err(sdk::Error::new(
                        "Std.InvalidCastException",
                        "void callback became null",
                    ));
                }
                call.invoke(
                    "Test.Callback",
                    &[NativeValue::Strings(vec!["中文".into(), "".into()])],
                )
            })
        }
    }
    static CALLBACKS: &[abi::Function] = &[function(
        "__callback",
        &[],
        abi::STR_ARRAY,
        abi::BLOCKING | abi::REENTERS,
        callback,
    )];
    static CALLBACK: abi::Module = abi::Module {
        name: abi::Text::new("native.callback"),
        functions: CALLBACKS.as_ptr(),
        function_count: 1,
        ..MODULE
    };
    unsafe { crate::interop::register_native_module(&ctx, &CALLBACK) }.unwrap();
    let returned = invoke_export(&ctx, Some("native.callback"), "__callback", &[])
        .unwrap()
        .unwrap()
        .unwrap();
    let copied = native_value::from_vm(&returned, abi::STR_ARRAY, "callback", 0).unwrap();
    assert!(matches!(copied, NativeValue::Strings(values) if values == ["中文", ""]));
    assert_eq!(ctx.core.threads.parked_count.load(Ordering::Acquire), 0);
}
