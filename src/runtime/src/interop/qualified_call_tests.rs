//! Qualified dispatch across independent modules and real interpreter/JIT frames.
use crate::{context::VmContext, metadata::{Function, Module}, object::Value};
use crate::exec::call::{builtins::invoke_module, builtin_abi::BuiltinError};
use z42_abi::module as abi;
use z42_rs::module::{self as sdk, Value as NativeValue};

unsafe extern "C" fn first(raw: *mut abi::Call) -> u32 {
    unsafe { sdk::dispatch(raw, |_| Ok(NativeValue::I64(11))) }
}
unsafe extern "C" fn second(raw: *mut abi::Call) -> u32 {
    unsafe { sdk::dispatch(raw, |_| Ok(NativeValue::I64(22))) }
}
unsafe extern "C" fn void(raw: *mut abi::Call) -> u32 {
    unsafe { sdk::dispatch(raw, |_| Ok(NativeValue::Void)) }
}
unsafe extern "C" fn null(raw: *mut abi::Call) -> u32 {
    unsafe { sdk::dispatch(raw, |_| Ok(NativeValue::Null)) }
}
const fn function(name: &'static str, result: u32, invoke: abi::Invoke) -> abi::Function {
    abi::Function { name: abi::Text::new(name), parameters: abi::Parameters::new(&[]),
        result, flags: 0, invoke: Some(invoke) }
}
static FIRST_FUNCTIONS: &[abi::Function] = &[
    function("__qualified_same", abi::I64, first),
    function("__qualified_void", abi::VOID, void),
    function("__qualified_null", abi::OPT_STR, null),
];
static SECOND_FUNCTIONS: &[abi::Function] = &[function("__qualified_same", abi::I64, second)];
static FIRST: abi::Module = abi::Module {
    version: abi::MODULE_ABI_VERSION, size: std::mem::size_of::<abi::Module>() as u32,
    name: abi::Text::new("qualified.first"), functions: FIRST_FUNCTIONS.as_ptr(),
    function_count: FIRST_FUNCTIONS.len(), create_state: None, destroy_state: None,
};
static SECOND: abi::Module = abi::Module {
    name: abi::Text::new("qualified.second"), functions: SECOND_FUNCTIONS.as_ptr(),
    function_count: SECOND_FUNCTIONS.len(), ..FIRST
};
fn register(ctx: &VmContext) {
    unsafe {
        super::register_native_module(ctx, &FIRST).unwrap();
        super::register_native_module(ctx, &SECOND).unwrap();
    }
}

#[test]
fn qualified_dispatch_never_uses_a_bare_name_winner() {
    let ctx = VmContext::new(); register(&ctx);
    for (provider, expected) in [("qualified.first", 11), ("qualified.second", 22)] {
        assert_eq!(invoke_module(&ctx, provider, "__qualified_same", &[], &[]).unwrap(), Some(Value::I64(expected)));
    }
    assert!(super::invoke_export(&ctx, None, "__qualified_same", &[]).is_none());
    for (provider, name) in [("missing.provider", "__qualified_same"),
        ("qualified.second", "__qualified_void"), ("z42_repl", "__println")] {
        let error = invoke_module(&ctx, provider, name, &[], &[]).unwrap_err();
        assert!(matches!(*error, BuiltinError::Exception { class: "Std.MissingSymbolException", ref message }
            if message.contains(provider) && message.contains(name)), "{error:?}");
    }
    assert!(matches!(*invoke_module(&ctx, "qualified.first", "__qualified_same", &[], &[9]).unwrap_err(),
        BuiltinError::Internal(_)));
}

#[test]
fn internal_descriptors_follow_their_declared_provider() {
    let registry = crate::host::builtins::registry();
    for (provider, name) in [("z42_diagnostics", "__diag_counters"),
        ("z42_harness", "__test_io_take_stdout_buffer"), ("z42.core", "__println")] {
        assert!(registry.resolve(provider, name).is_some());
    }
    assert!(registry.resolve("z42.core", "__diag_counters").is_none());
    let ctx = VmContext::new();
    assert_eq!(invoke_module(&ctx, "z42_harness", "__test_io_install_stdout_sink", &[], &[]).unwrap(), None);
    assert_eq!(invoke_module(&ctx, "z42_harness", "__test_io_take_stdout_buffer", &[], &[]).unwrap(),
        Some(Value::Str("".into())));
}

fn script(provider: &str, symbol: &str) -> Module {
    use crate::zpkg::{BasicBlock, CallNativeInsn, ExecMode, Instruction, Terminator};
    let function = Function::new(crate::zpkg::ir::Function {
        name: "Test.Main".into(), param_count: 0, ret_type: "object".into(), exec_mode: ExecMode::Interp,
        blocks: vec![BasicBlock { label: "entry".into(), instructions: vec![
            Instruction::ConstI64 { dst: 0, val: 99 },
            Instruction::CallNative(Box::new(CallNativeInsn { dst: 0, module: provider.into(),
                type_name: "".into(), symbol: symbol.into(), args: Box::new([]) })),
        ], terminator: Terminator::Ret { reg: Some(0) } }],
        is_static: true, visibility: 0, method_flags: 0, min_arg: 0, params_from: 0xff,
        max_reg: 1, cold: None, reg_types: Box::new([]), block_index: Default::default(),
        branch_targets: vec![], frame_meta: None,
    });
    Module::new(crate::zpkg::image::ModuleData { name: "Test".into(), string_pool: vec![],
        classes: vec![], func_index: [("Test.Main".into(), 0)].into_iter().collect() },
        vec![function], Default::default())
}

#[test]
fn interpreter_module_calls_preserve_provider_void_and_null_without_typed_ffi() {
    for (provider, name, expected) in [
        ("qualified.first", "__qualified_same", Value::I64(11)),
        ("qualified.second", "__qualified_same", Value::I64(22)),
        ("qualified.first", "__qualified_void", Value::I64(99)),
        ("qualified.first", "__qualified_null", Value::Null),
    ] {
        let module = script(provider, name);
        let ctx = VmContext::new(); register(&ctx);
        assert_eq!(crate::interp::run_returning(&ctx, &module, &module.functions[0], &[]).unwrap(), Some(expected));
        assert_eq!(ctx.counters().native_calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    }
}

#[test]
#[cfg(all(feature = "jit", z42_jit_supported))]
fn jit_module_calls_compile_and_preserve_provider_void_and_null() {
    use crate::jit::runtime::{frame::JitFrame, invoke::{call_entry, NativeOutcome}};
    for (provider, name, expected) in [
        ("qualified.first", "__qualified_same", Value::I64(11)),
        ("qualified.second", "__qualified_same", Value::I64(22)),
        ("qualified.first", "__qualified_void", Value::I64(99)),
        ("qualified.first", "__qualified_null", Value::Null),
    ] {
        let ctx = VmContext::with_module(script(provider, name)); register(&ctx);
        let rt = crate::jit::compile::state::install(&ctx, ctx.module().unwrap(),
            crate::config::runtime_config().jit_config()).unwrap();
        let entry = rt.resolve_fn_by_id(&ctx, 0).expect("module call must compile, not fall back");
        match unsafe { call_entry(&ctx, entry, JitFrame::new(&ctx, entry.max_reg, &[])) } {
            NativeOutcome::Returned(value) => assert_eq!(value, Some(expected)),
            NativeOutcome::Threw => panic!("unexpected native throw: {:?}", ctx.take_exception()),
        }
    }
}
