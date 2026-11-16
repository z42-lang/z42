//! Static composition exercises real interpreter and JIT through the host.
use crate::components::bridge::VmContext;
use z42::host::{ExecMode, LoadOptions, LoadedProgram, Value};
use z42::metadata::Module;
use z42::zpkg::{self, BasicBlock, Instruction, Terminator};

fn register_components() {
    use z42::components::abi::{register_static_component, INTERP_COMPONENT, JIT_COMPONENT};
    unsafe {
        register_static_component(INTERP_COMPONENT, z42_interp::component::z42_interp_init).unwrap();
        register_static_component(JIT_COMPONENT, crate::jit::component::z42_jit_init).unwrap();
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
    let ctx = program.context();
    assert!(ctx.core().exec().engines.has_baseline());
    assert!(ctx.core().exec().engines.has_jit());
    assert!(ctx.core().exec().engines.tier().is_none());
    assert_eq!(ctx.core().diagnostics().snapshot(ctx.core().threads()).jit_methods_compiled, 0);
}
fn program(throws: bool) -> LoadedProgram {
    let func = zpkg::ir::Function {
        name: "Demo.Main".into(), param_count: 0, ret_type: "long".into(), exec_mode: ExecMode::Jit,
        blocks: vec![BasicBlock { label: "entry".into(), instructions: vec![Instruction::ConstI64 { dst: 0, val: 42 }],
            terminator: if throws { Terminator::Throw { reg: 0 } } else { Terminator::Ret { reg: Some(0) } } }],
        is_static: true, visibility: 0, method_flags: 0, min_arg: 0, params_from: 0xff, max_reg: 0,
        cold: None, reg_types: Box::new([]), block_index: Default::default(), branch_targets: Vec::new(), frame_meta: None,
    };
    let module = Module::new(zpkg::image::ModuleData { name: "Demo".into(), string_pool: Vec::new(), classes: Vec::new(),
        func_index: [("Demo.Main".into(), 0)].into_iter().collect() }, vec![z42::metadata::Function::new(func)], Default::default());
    let ctx = VmContext::with_module(module);
    ctx.core().exec().engines.register_baseline(&z42_interp::interp::engine::ENGINE).unwrap();
    ctx.core().exec().engines.register_jit(crate::jit::execution_engine(), z42::config::runtime_config().jit_config()).unwrap();
    z42::components::bridge::test_loaded_program(ctx, ExecMode::Jit, Some("Demo.Main".into())).unwrap()
}
#[test]
fn jit_host_calls_return_values_compile_and_preserve_static_state() {
    let program = program(false);
    assert_eq!(program.invoke("Demo.Main", &[]).unwrap(), Some(Value::I64(42)));
    assert!(program.context().core().diagnostics().snapshot(program.context().core().threads()).jit_methods_compiled > 0);
    program.context().statics().set_named("Demo.sentinel", Value::I64(91)).unwrap();
    assert_eq!(program.invoke("Demo.Main", &[]).unwrap(), Some(Value::I64(42)));
    assert_eq!(program.context().statics().get_named("Demo.sentinel"), Value::I64(91));
    assert!(self::program(true).invoke("Demo.Main", &[]).unwrap_err().to_string().contains("uncaught exception"));
}

#[test]
fn c_host_jit_mode_uses_the_shared_engine_and_returns_values() {
    use std::{ffi::{c_char, c_void, CString}, ptr, sync::Mutex};
    use z42::host::{self, config::{Z42HostConfig, Z42ExecMode, Z42_HOST_ABI_VERSION}, error::Z42HostStatus, Z42Host, Z42Module, Z42Entry, Z42Value, Z42_VALUE_TAG_I64};
    unsafe extern "C" fn sink(bytes: *const c_char, len: usize, data: *mut c_void) {
        let output = unsafe { &*(data as *const Mutex<Vec<u8>>) };
        output.lock().unwrap().extend_from_slice(unsafe { std::slice::from_raw_parts(bytes.cast(), len) });
    }
    let Some((bytes, libs)) = z42::components::bridge::test_embedding_hello() else { return };
    register_components();
    let output = Mutex::new(Vec::new());
    let libs = CString::new(libs).unwrap();
    let paths = [libs.as_ptr(), ptr::null()];
    let cfg = Z42HostConfig { abi_version: Z42_HOST_ABI_VERSION, struct_size: std::mem::size_of::<Z42HostConfig>() as u32,
        exec_mode: Z42ExecMode::Jit as i32, heap_initial_bytes: 0, heap_max_bytes: 0, stdout_sink: Some(sink), stderr_sink: None,
        sink_user_data: (&output as *const Mutex<Vec<u8>>).cast_mut().cast(), search_paths: paths.as_ptr(),
        zpkg_resolver: None, zpkg_resolver_user_data: ptr::null_mut() };
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
