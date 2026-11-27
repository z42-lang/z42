//! Static composition exercises real interpreter and JIT through the host.
#![cfg(all(feature = "jit", feature = "jit-test-support", z42_jit_supported))]

use z42::components::bridge::{
    installed_tier, register_baseline_engine, test_engine_registered, verification_context, VmContext,
};
use z42::host::{ExecMode, LoadOptions, LoadedProgram, Value};
use z42::metadata::Module;
use z42::zpkg::{self, BasicBlock, Instruction, Terminator};

fn register_components() {
    use z42::components::abi::{register_static_component, INTERP_COMPONENT, JIT_COMPONENT};
    unsafe {
        register_static_component(INTERP_COMPONENT, z42_interp::component::z42_interp_init).unwrap();
        register_static_component(JIT_COMPONENT, z42_jit::jit::component::z42_jit_init).unwrap();
    }
}
fn empty_zbc() -> Vec<u8> {
    use zpkg::format::*;
    let mut bytes = vec![0; 16];
    bytes[..4].copy_from_slice(&ZBC_MAGIC);
    bytes[4..6].copy_from_slice(&ZBC_VERSION_MAJOR.to_le_bytes());
    bytes[6..8].copy_from_slice(&ZBC_VERSION_MINOR.to_le_bytes());
    bytes
}
#[test]
fn jit_mode_registers_both_engines_without_eager_compilation() {
    register_components();
    let program = LoadedProgram::load_bytes(&empty_zbc(), LoadOptions { mode: ExecMode::Jit, ..Default::default() }).unwrap();
    let ctx = verification_context(&program);
    assert!(test_engine_registered(ctx, ExecMode::Interp));
    assert!(test_engine_registered(ctx, ExecMode::Jit));
    assert!(installed_tier(ctx).is_none());
    assert_eq!(ctx.core().diagnostics().snapshot(ctx.core().threads()).jit_methods_compiled, 0);
}
fn demo_function(throws: bool) -> z42::metadata::Function {
    z42::metadata::Function::new(zpkg::ir::Function {
        name: "Demo.Main".into(), param_count: 0, ret_type: "long".into(), exec_mode: ExecMode::Jit,
        blocks: vec![BasicBlock { label: "entry".into(), instructions: vec![Instruction::ConstI64 { dst: 0, val: 42 }],
            terminator: if throws { Terminator::Throw { reg: 0 } } else { Terminator::Ret { reg: Some(0) } } }],
        is_static: true, visibility: 0, method_flags: 0, min_arg: 0, params_from: 0xff, max_reg: 0,
        cold: None, reg_types: Box::new([]), block_index: Default::default(), branch_targets: Vec::new(), frame_meta: None,
    })
}

fn program(throws: bool) -> LoadedProgram {
    let module = Module::new(zpkg::image::ModuleData { name: "Demo".into(), string_pool: Vec::new(), classes: Vec::new(),
        func_index: [("Demo.Main".into(), 0)].into_iter().collect() }, vec![demo_function(throws)], Default::default());
    let ctx = VmContext::with_module(module);
    use z42::components::{
        abi::{INTERP_COMPONENT, JIT_COMPONENT},
        bridge::test_bind_execution,
    };
    // Both factories are linked into this integration test's Core.
    unsafe {
        test_bind_execution(
            &ctx,
            (INTERP_COMPONENT, z42_interp::component::z42_interp_init),
            Some((JIT_COMPONENT, z42_jit::jit::component::z42_jit_init)),
        ).unwrap();
    }
    z42::components::bridge::test_loaded_program(ctx, ExecMode::Jit, Some("Demo.Main".into())).unwrap()
}
#[test]
fn jit_host_calls_return_values_compile_and_preserve_static_state() {
    let program = program(false);
    assert_eq!(program.invoke("Demo.Main", &[]).unwrap(), Some(Value::I64(42)));
    let ctx = verification_context(&program);
    assert!(ctx.core().diagnostics().snapshot(ctx.core().threads()).jit_methods_compiled > 0);
    ctx.statics().set_named("Demo.sentinel", Value::I64(91)).unwrap();
    assert_eq!(program.invoke("Demo.Main", &[]).unwrap(), Some(Value::I64(42)));
    assert_eq!(ctx.statics().get_named("Demo.sentinel"), Value::I64(91));
    assert!(self::program(true).invoke("Demo.Main", &[]).unwrap_err().to_string().contains("uncaught exception"));
}

#[test]
fn jit_fallback_uses_the_bound_baseline_provider() {
    use core::ffi::c_void;
    use z42::components::{
        abi::*,
        bridge::{core_identity, test_bind_execution, CORE_ABI},
    };
    use z42::exec::{args::CallArgs, outcome::ExecOutcome, Engine};

    struct Baseline;
    impl Engine for Baseline {
        fn invoke(&self, _: &VmContext, _: &Module, func: &z42::metadata::Function,
            args: CallArgs<'_>, _: &[String]) -> anyhow::Result<ExecOutcome> {
            anyhow::ensure!(func.name == "Demo.Main" && matches!(args, CallArgs::Values([])));
            Ok(ExecOutcome::Returned(Some(Value::I64(17))))
        }
    }

    static BASELINE: Baseline = Baseline;
    const ALTERNATE: ComponentId = ComponentId::new(0x414c545f42415345);

    unsafe extern "C" fn identity() -> ExecutionIdentity {
        ExecutionIdentity { image: core_identity(), abi: CORE_ABI }
    }
    unsafe extern "C" fn attach(vm: *const c_void, _: *const ExecutionConfig) -> ComponentStatus {
        // SAFETY: Core already validated its identity and retains this linked engine.
        let ctx = unsafe { &*vm.cast::<VmContext>() };
        match unsafe { register_baseline_engine(ctx, &BASELINE) } {
            Ok(()) => ComponentStatus::OK,
            Err(_) => ComponentStatus::ERROR,
        }
    }
    static API: ExecutionApi = ExecutionApi {
        header: ApiHeader::of::<ExecutionApi>(1), core_identity: identity, attach,
    };
    unsafe extern "C" fn create(_: *const RuntimeApi, _: VmHandle, out: *mut InstanceHandle) -> ComponentStatus {
        unsafe { *out = core::ptr::null_mut(); }
        ComponentStatus::OK
    }
    unsafe extern "C" fn destroy(_: InstanceHandle) {}
    unsafe extern "C" fn query(_: InstanceHandle, id: InterfaceId, version: u32, out: *mut InterfaceRef) -> ComponentStatus {
        if id != ExecutionApi::ID || version != ExecutionApi::VERSION { return ComponentStatus::UNAVAILABLE; }
        unsafe { *out = InterfaceRef { api: &API.header, context: core::ptr::null_mut() }; }
        ComponentStatus::OK
    }
    static SPECS: [InterfaceSpec; 1] = [InterfaceSpec::of::<ExecutionApi>()];
    static DESCRIPTOR: ComponentDescriptor = ComponentDescriptor {
        header: ApiHeader::of::<ComponentDescriptor>(COMPONENT_ABI_VERSION), build_id: COMPONENT_BUILD_ID,
        id: ALTERNATE, dependencies: IdSpan::EMPTY,
        interfaces: InterfaceSpan { data: SPECS.as_ptr(), len: SPECS.len() },
        create_vm: Some(create), destroy_vm: Some(destroy), query_interface: Some(query),
    };
    unsafe extern "C" fn init() -> *const ComponentDescriptor { &DESCRIPTOR }

    let ctx = VmContext::new();
    unsafe {
        test_bind_execution(&ctx, (ALTERNATE, init), Some((JIT_COMPONENT, z42_jit::jit::component::z42_jit_init))).unwrap();
    }
    let module = Module::new(zpkg::image::ModuleData {
        name: "Demo".into(), string_pool: Vec::new(), classes: Vec::new(), func_index: Default::default(),
    }, Vec::new(), Default::default());
    // The function is absent from the JIT module, so the real JIT entry falls back.
    let outcome = z42::exec::invoke_mode(&ctx, ExecMode::Jit, &module, &demo_function(false), &[]).unwrap();
    assert!(matches!(outcome, ExecOutcome::Returned(Some(Value::I64(17)))));
}

#[test]
fn c_host_jit_mode_uses_the_shared_engine_and_returns_values() {
    use std::{ffi::{c_char, c_void, CString}, ptr, sync::Mutex};
    use z42::host::{self, Z42HostConfig, Z42ExecMode, Z42_HOST_ABI_VERSION, error::Z42HostStatus, Z42Host, Z42Module, Z42Entry, Z42Value, Z42_VALUE_TAG_I64};
    unsafe extern "C" fn sink(bytes: *const c_char, len: usize, data: *mut c_void) {
        if data.is_null() || bytes.is_null() || len == 0 { return; }
        let output = unsafe { &*(data as *const Mutex<Vec<u8>>) };
        output.lock().unwrap().extend_from_slice(unsafe { std::slice::from_raw_parts(bytes.cast(), len) });
    }
    let Some((bytes, libs)) = z42::components::bridge::test_embedding_hello() else { return };
    register_components();
    let output = Mutex::new(Vec::new());
    let libs = CString::new(libs).unwrap();
    let paths = [libs.as_ptr(), ptr::null()];
    // Match a C caller's zero-initialized config. All fields are scalar values,
    // raw pointers, or nullable callback pointers.
    let mut cfg: Z42HostConfig = unsafe { std::mem::zeroed() };
    cfg.abi_version = Z42_HOST_ABI_VERSION;
    cfg.struct_size = std::mem::size_of::<Z42HostConfig>() as u32;
    cfg.exec_mode = Z42ExecMode::Jit as i32;
    cfg.stdout_sink = Some(sink);
    cfg.sink_user_data = (&output as *const Mutex<Vec<u8>>).cast_mut().cast();
    cfg.search_paths = paths.as_ptr();
    unsafe {
        let mut host_ptr: *mut Z42Host = ptr::null_mut();
        assert_eq!(host::z42_host_initialize(&cfg, &mut host_ptr), Z42HostStatus::Ok);
        let mut module: *mut Z42Module = ptr::null_mut();
        assert_eq!(host::z42_host_load_zbc(host_ptr, bytes.as_ptr(), bytes.len(), &mut module), Z42HostStatus::Ok);
        for name in ["UserStatic", "UserStatic", "Main"] {
            let is_static = name == "UserStatic";
            let name = CString::new(format!("Embedding.Hello.{name}")).unwrap();
            let mut entry: *mut Z42Entry = ptr::null_mut();
            assert_eq!(host::z42_host_resolve_entry(host_ptr, module, name.as_ptr(), &mut entry), Z42HostStatus::Ok);
            let mut result = Z42Value { tag: 0, reserved: 0, payload: 0 };
            assert_eq!(host::z42_host_invoke(entry, ptr::null(), 0, &mut result), Z42HostStatus::Ok);
            if is_static { assert_eq!((result.tag, result.payload), (Z42_VALUE_TAG_I64, 42)); }
        }
        assert!(z42::components::bridge::test_host_jit_compiled());
        assert_eq!(host::z42_host_shutdown(host_ptr), Z42HostStatus::Ok);
    }
    assert!(String::from_utf8(output.into_inner().unwrap()).unwrap().contains("Hello, World!"));
}
