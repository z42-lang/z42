//! Production-graph component tests. Run by runtime --linkage dynamic --verify
//! after relocation, using only the packaged core/std/backend library closure.
#![cfg(feature = "shared-core")]
use std::{path::PathBuf, sync::Arc};
use z42::host::{
    ExecMode,
    LoadOptions, LoadedProgram, Value, VmContext,
};

fn input(name: &str) -> PathBuf {
    std::env::var_os(name).map(PathBuf::from).expect("component verification must supply packaged fixture paths")
}
fn native_loading() -> z42::loader::NativeLoading {
    z42::loader::NativeLoading {
        search_dirs: vec![input("Z42_TEST_ENGINE_COMPONENT_DIR")],
        ..Default::default()
    }
}

fn load(mode: ExecMode, native: z42::loader::NativeLoading) -> anyhow::Result<LoadedProgram> {
    LoadedProgram::load_file(input("Z42_TEST_ENGINE_BYTECODE").to_str().unwrap(), LoadOptions {
        mode, native, search_dirs: vec![input("Z42_TEST_ENGINE_LIBS")], ..Default::default()
    })
}
fn echo(program: &LoadedProgram, value: i64) -> i64 {
    match program.invoke("EngineComponentProbe.Echo", &[Value::I64(value)]).unwrap() {
        Some(Value::I64(n)) => n,
        other => panic!("unexpected return: {other:?}"),
    }
}
#[test]
fn actual_dynamic_object_protocol_callbacks_and_execution_bridges_agree() {
    for mode in [ExecMode::Interp, ExecMode::Jit] {
        let program = load(mode, native_loading()).unwrap();
        for _ in 0..2 {
            assert!(matches!(program.invoke("EngineComponentProbe.ObjectProtocolRoundtrip", &[]).unwrap(), Some(Value::I64(42))));
        }
        if mode == ExecMode::Jit {
            assert!(program.context().core().diagnostics().snapshot(program.context().core().threads()).jit_methods_compiled > 0);
        }
    }
}
#[test]
fn actual_dynamic_engines_preserve_vm_isolation_worker_gc_and_exceptions() {
    // This test thread exits after both VMs/options drop. Backend/dependency TLS
    // destructors must still point into mapped native images at thread exit.
    for mode in [ExecMode::Interp, ExecMode::Jit] {
        let first = load(mode, native_loading()).unwrap();
        let second = load(mode, native_loading()).unwrap();
        assert_eq!(echo(&first, 40), 41);
        assert_eq!(echo(&first, 40), 42);
        assert_eq!(echo(&second, 40), 41);
        assert!(matches!(first.invoke("EngineComponentProbe.Worker", &[]).unwrap(), Some(Value::I64(42))));
        assert!(first.invoke("EngineComponentProbe.Boom", &[]).unwrap_err().to_string().contains("component-exception"));
        assert_eq!(echo(&first, 40), 43, "a failed invocation must not poison subsequent calls");
        if mode == ExecMode::Jit {
            let ctx = first.context();
            assert!(ctx.core().diagnostics().snapshot(ctx.core().threads()).jit_methods_compiled > 0);
            let before = ctx.core().diagnostics().snapshot(ctx.core().threads()).jit_native_from_interp;
            let module = ctx.module().unwrap();
            let hot = module.functions.iter().find(|f| f.name == "EngineComponentProbe.HotLoop").unwrap();
            let result = ctx.core().exec().engines.invoke_mode(ExecMode::Interp, ctx, module, hot, &[Value::I64(20_000)]).unwrap();
            assert!(matches!(result, z42::exec::outcome::ExecOutcome::Returned(Some(Value::I64(199_990_000)))));
            assert!(ctx.core().diagnostics().snapshot(ctx.core().threads()).jit_native_from_interp > before, "dynamic interpreter must hand its hot loop to the dynamic JIT");
        } else {
            assert_eq!(first.context().core().diagnostics().snapshot(first.context().core().threads()).jit_methods_compiled, 0);
        }
        drop(first);
        assert_eq!(echo(&second, 40), 42);
    }
}
#[test]
fn worker_retains_dynamic_execution_after_host_release() {
    let program = load(ExecMode::Jit, native_loading()).unwrap();
    assert_eq!(echo(&program, 5), 6);
    let weak = Arc::downgrade(program.context().core());
    let worker = VmContext::new_with_core(program.context().core_arc());
    let module = Arc::clone(program.context().module().unwrap());
    drop(program);
    assert!(weak.upgrade().is_some());
    let function = module.functions.iter().find(|f| f.name == "EngineComponentProbe.Echo").unwrap();
    let result = worker.core().exec().engines.invoke_mode(ExecMode::Jit, &worker, &module, function, &[Value::I64(5)]).unwrap();
    assert!(matches!(result, z42::exec::outcome::ExecOutcome::Returned(Some(Value::I64(7)))));
    drop(worker);
    assert!(weak.upgrade().is_none(), "backend state must not form a Core cycle");
}
#[test]
fn missing_and_disallowed_native_sources_are_errors() {
    let mut native = native_loading();
    native.search_dirs = vec![input("Z42_TEST_ENGINE_COMPONENT_DIR").join("missing-components")];
    let error = load(ExecMode::Jit, native).err().unwrap().to_string();
    assert!(error.contains("was not found"), "{error}");
    let mut native = native_loading();
    native.allow_runtime = false;
    let error = load(ExecMode::Jit, native).err().unwrap().to_string();
    assert!(error.contains("disabled"), "{error}");
    // The rejected VM must not poison process-wide native code caches.
    assert_eq!(echo(&load(ExecMode::Interp, native_loading()).unwrap(), 40), 41);
}
#[test]
fn packaged_c_embedding_host_loads_components_and_shuts_down() {
    use std::{ffi::{c_char, CString}, ptr};
    use z42::host::{
        Z42HostConfig, Z42ExecMode, Z42_HOST_ABI_VERSION,
        Z42Entry, Z42Module, error::Z42HostStatus,
        Z42Host, Z42Value, Z42_VALUE_TAG_I64,
    };
    type Initialize = unsafe extern "C" fn(*const Z42HostConfig, *mut *mut Z42Host) -> Z42HostStatus;
    type Load = unsafe extern "C" fn(*mut Z42Host, *const u8, usize, *mut *mut Z42Module) -> Z42HostStatus;
    type Resolve = unsafe extern "C" fn(*mut Z42Host, *mut Z42Module, *const c_char, *mut *mut Z42Entry) -> Z42HostStatus;
    type Invoke = unsafe extern "C" fn(*mut Z42Entry, *const Z42Value, usize, *mut Z42Value) -> Z42HostStatus;
    type Shutdown = unsafe extern "C" fn(*mut Z42Host) -> Z42HostStatus;
    let path = input("Z42_TEST_ENGINE_COMPONENT_DIR").join(format!("{}z42{}", std::env::consts::DLL_PREFIX, std::env::consts::DLL_SUFFIX));
    let libs = CString::new(input("Z42_TEST_ENGINE_LIBS").to_str().unwrap()).unwrap();
    let search = [libs.as_ptr(), ptr::null()];
    let cfg = Z42HostConfig {
        abi_version: Z42_HOST_ABI_VERSION, struct_size: std::mem::size_of::<Z42HostConfig>() as u32,
        exec_mode: Z42ExecMode::Jit as i32, heap_initial_bytes: 0, heap_max_bytes: 0,
        stdout_sink: None, stderr_sink: None, sink_user_data: ptr::null_mut(),
        search_paths: search.as_ptr(), zpkg_resolver: None, zpkg_resolver_user_data: ptr::null_mut(),
    };
    let bytes = std::fs::read(input("Z42_TEST_ENGINE_BYTECODE")).unwrap();
    let fqn = CString::new("EngineComponentProbe.Echo").unwrap();
    // Keep the C host image through thread exit, including its TLS destructors.
    unsafe {
        let library = std::mem::ManuallyDrop::new(z42::pal::dynlib::Library::open(&path).unwrap());
        assert!(library.symbol::<extern "C" fn()>("z42_runtime_entry_v1").is_err(), "CLI must not be exported by the runtime library");
        let initialize = library.symbol::<Initialize>("z42_host_initialize").unwrap();
        let load = library.symbol::<Load>("z42_host_load_zbc").unwrap();
        let resolve = library.symbol::<Resolve>("z42_host_resolve_entry").unwrap();
        let invoke = library.symbol::<Invoke>("z42_host_invoke").unwrap();
        let shutdown = library.symbol::<Shutdown>("z42_host_shutdown").unwrap();
        let mut host = ptr::null_mut();
        assert_eq!(initialize(&cfg, &mut host), Z42HostStatus::Ok);
        let mut module = ptr::null_mut();
        assert_eq!(load(host, bytes.as_ptr(), bytes.len(), &mut module), Z42HostStatus::Ok);
        let mut entry = ptr::null_mut();
        assert_eq!(resolve(host, module, fqn.as_ptr(), &mut entry), Z42HostStatus::Ok);
        let arg = Z42Value { tag: Z42_VALUE_TAG_I64, reserved: 0, payload: 40 };
        let mut result = Z42Value { tag: 0, reserved: 0, payload: 0 };
        assert_eq!(invoke(entry, &arg, 1, &mut result), Z42HostStatus::Ok);
        assert_eq!((result.tag, result.payload), (Z42_VALUE_TAG_I64, 41));
        assert_eq!(invoke(entry, &arg, 1, &mut result), Z42HostStatus::Ok);
        assert_eq!((result.tag, result.payload), (Z42_VALUE_TAG_I64, 42), "the C host reuses the same JIT-backed VM state");
        assert_eq!(shutdown(host), Z42HostStatus::Ok);
    }
}

#[test]
fn dynamic_interpreter_preserves_native_pin_instructions() {
    use z42_zpkg::{BasicBlock, FieldGetInsn, Instruction, Terminator};
    use z42::{metadata::Function, exec::outcome::ExecOutcome};
    let module = z42::host::Module::new(z42_zpkg::image::ModuleData {
        name: "PinProbe".into(), string_pool: vec!["shared-core".into()],
        classes: vec![], func_index: Default::default(),
    }, vec![Function::new(z42_zpkg::ir::Function {
        name: "PinProbe.Main".into(), param_count: 0, ret_type: "i64".into(), exec_mode: ExecMode::Interp,
        blocks: vec![BasicBlock { label: "entry".into(), instructions: vec![
            Instruction::ConstStr { dst: 0, idx: 0 },
            Instruction::PinPtr { dst: 1, src: 0 },
            Instruction::FieldGet(Box::new(FieldGetInsn { dst: 2, obj: 1, field_name: "len".into() })),
            Instruction::UnpinPtr { pinned: 1 },
        ], terminator: Terminator::Ret { reg: Some(2) } }],
        is_static: true, visibility: 0, method_flags: 0, min_arg: 0, params_from: 0xFF,
        max_reg: 3, cold: None, reg_types: Box::new([]), block_index: Default::default(),
        branch_targets: vec![], frame_meta: None,
    })], Default::default());
    let program = load(ExecMode::Interp, native_loading()).unwrap();
    let ctx = program.context();
    let outcome = ctx.core().exec().engines.invoke_mode(ExecMode::Interp, ctx, &module, &module.functions[0], &[]).unwrap();
    assert!(matches!(outcome, ExecOutcome::Returned(Some(Value::I64(11)))));
}

#[test]
fn actual_dynamic_profiler_is_lazy_isolated_and_releases_its_core() {
    let a = load(ExecMode::Interp, native_loading()).unwrap();
    let b = load(ExecMode::Interp, native_loading()).unwrap();
    assert!(!z42::host::profiler_loaded(a.context()));
    assert_eq!(echo(&a, 40), 41);
    assert!(!z42::host::profiler_loaded(a.context()));
    let weak = Arc::downgrade(a.context().core());
    assert!(matches!(a.invoke("EngineComponentProbe.HeapInspection", &[]).unwrap(), Some(Value::I64(42))));
    assert!(z42::host::profiler_loaded(a.context()));
    assert!(!z42::host::profiler_loaded(b.context()));
    let file = input("Z42_TEST_ENGINE_BYTECODE").with_extension("heapsnapshot");
    let bytes = z42::host::write_heap_snapshot(a.context(), file.to_str().unwrap()).unwrap();
    assert!(bytes > 0);
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert!(json.get("snapshot").is_some());
    std::fs::remove_file(file).unwrap();
    drop(a);
    assert!(weak.upgrade().is_none());
}
#[test]
fn disabled_profiler_fails_only_when_requested() {
    let program = LoadedProgram::load_file(input("Z42_TEST_ENGINE_BYTECODE").to_str().unwrap(), LoadOptions {
        mode: ExecMode::Interp, native: native_loading(), allow_profiling: false,
        search_dirs: vec![input("Z42_TEST_ENGINE_LIBS")], ..Default::default()
    }).unwrap();
    assert_eq!(echo(&program, 40), 41);
    let error = z42::host::prepare_profiler(program.context()).unwrap_err().to_string();
    assert!(error.contains("profiler") && error.contains("disabled"), "{error}");
}
