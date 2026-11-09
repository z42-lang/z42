use super::{LoadOptions, LoadedProgram};
use crate::{
    zpkg::{self, BasicBlock, ExecMode, Instruction, Terminator},
    object::Value,
    pal::vfs::MemoryFs,
    vm::{context::VmContext, runtime::Module},
};
use std::sync::Arc;

fn program(mode: ExecMode, throws: bool) -> LoadedProgram {
    let function = zpkg::ir::Function {
        name: "Demo.Main".into(),
        param_count: 0,
        ret_type: "long".into(),
        exec_mode: mode,
        blocks: vec![BasicBlock {
            label: "entry".into(),
            instructions: vec![Instruction::ConstI64 { dst: 0, val: 42 }],
            terminator: if throws {
                Terminator::Throw { reg: 0 }
            } else {
                Terminator::Ret { reg: Some(0) }
            },
        }],
        is_static: true,
        visibility: 0,
        method_flags: 0,
        min_arg: 0,
        params_from: 0xff,
        max_reg: 0,
        cold: None,
        reg_types: Box::new([]),
        block_index: Default::default(),
        branch_targets: Vec::new(),
        frame_meta: None,
    };
    let mut module = Module::from_ir(zpkg::ir::Module {
        name: "Demo".into(),
        string_pool: Vec::new(),
        classes: Vec::new(),
        functions: vec![function],
        func_index: Default::default(),
    });
    crate::assembly::loader::build_block_indices(&mut module);
    crate::vm::loader::package::build_func_index(&mut module);
    let ctx = VmContext::with_module(module);
    crate::host::boot::prepare_execution(&ctx, ctx.module().unwrap());
    LoadedProgram::new(ctx, mode, Some("Demo.Main".into())).unwrap()
}

pub(crate) fn empty_zbc() -> Vec<u8> {
    use zpkg::format::*;
    let mut bytes = vec![0; 16];
    bytes[..4].copy_from_slice(&ZBC_MAGIC);
    bytes[4..6].copy_from_slice(&ZBC_VERSION_MAJOR.to_le_bytes());
    bytes[6..8].copy_from_slice(&ZBC_VERSION_MINOR.to_le_bytes());
    bytes
}

#[test]
fn file_and_bytes_share_preparation_and_preserve_input_errors() {
    let bytes = empty_zbc();
    let fs = Arc::new(MemoryFs::new());
    fs.mount("/app.zbc", bytes.clone());
    let options = || LoadOptions {
        mode: ExecMode::Interp,
        vfs: fs.clone(),
        ..Default::default()
    };
    let file = LoadedProgram::load_file("/app.zbc", options()).unwrap();
    let memory = LoadedProgram::load_bytes(&bytes, options()).unwrap();
    assert_eq!(file.module().name, memory.module().name);
    assert_eq!(
        file.module().functions.len(),
        memory.module().functions.len()
    );
    assert!(file
        .run_entry(None)
        .unwrap_err()
        .to_string()
        .contains("no entry point"));
    assert!(LoadedProgram::load_bytes(b"bad", options()).is_err());
    assert!(LoadedProgram::load_file("/absent.zbc", options()).is_err());
}

#[test]
fn repeated_invocations_preserve_static_state_and_validate_entries_and_arguments() {
    let program = program(ExecMode::Interp, false);
    assert_eq!(
        program.invoke("Demo::Main", &[]).unwrap(),
        Some(Value::I64(42))
    );
    program
        .context()
        .static_set("Demo.sentinel", Value::I64(91))
        .unwrap();
    program.run_entry(None).unwrap();
    assert_eq!(
        program.context().static_get("Demo.sentinel"),
        Value::I64(91)
    );
    assert!(program.invoke("missing", &[]).is_err());
    assert!(program
        .invoke("Demo.Main", &[Value::I64(1)])
        .unwrap_err()
        .to_string()
        .contains("arg-count-mismatch"));
    let other = self::program(ExecMode::Interp, false);
    assert_eq!(other.context().static_get("Demo.sentinel"), Value::Null);
}

#[test]
fn initialization_failure_is_sticky_and_entry_is_not_executed() {
    let program = program(ExecMode::Interp, false);
    *program.context().core.runtime.static_init_error.lock() = Some("init failed".into());
    for _ in 0..2 {
        assert!(program
            .invoke("Demo.Main", &[])
            .unwrap_err()
            .to_string()
            .contains("init failed"));
    }
}

#[test]
fn interpreted_exceptions_are_reported_by_the_host_service() {
    let program = program(ExecMode::Interp, true);
    assert!(program
        .invoke("Demo.Main", &[])
        .unwrap_err()
        .to_string()
        .contains("uncaught exception"));
}

#[test]
fn unsupported_mode_fails_before_execution() {
    let program = program(ExecMode::Aot, false);
    assert!(program
        .run_entry(None)
        .unwrap_err()
        .to_string()
        .contains("AOT mode is not implemented"));
}

#[cfg(all(feature = "jit", z42_jit_supported))]
#[test]
fn jit_host_calls_return_values_compile_and_preserve_static_state() {
    let program = program(ExecMode::Jit, false);
    assert_eq!(
        program.invoke("Demo.Main", &[]).unwrap(),
        Some(Value::I64(42))
    );
    assert!(program.context().counters_snapshot().jit_methods_compiled > 0);
    program
        .context()
        .static_set("Demo.sentinel", Value::I64(91))
        .unwrap();
    assert_eq!(
        program.invoke("Demo.Main", &[]).unwrap(),
        Some(Value::I64(42))
    );
    assert_eq!(
        program.context().static_get("Demo.sentinel"),
        Value::I64(91)
    );
    let throwing = self::program(ExecMode::Jit, true);
    assert!(throwing
        .invoke("Demo.Main", &[])
        .unwrap_err()
        .to_string()
        .contains("uncaught exception"));
}

#[cfg(not(all(feature = "jit", z42_jit_supported)))]
#[test]
fn jit_request_is_rejected_without_the_feature() {
    let program = program(ExecMode::Jit, false);
    assert!(program
        .run_entry(None)
        .unwrap_err()
        .to_string()
        .contains("JIT backend unavailable"));
}

#[test]
fn resolver_overrides_disk_prelude() {
    struct Resolver;
    impl super::ZpkgResolver for Resolver {
        fn resolve(&self, namespace: &str) -> Option<Vec<u8>> {
            assert_eq!(namespace, "z42.core");
            Some(empty_zbc())
        }
    }
    let fs = Arc::new(MemoryFs::new());
    fs.mount(std::path::Path::new("/libs").join("z42.core.zpkg").to_str().unwrap(), b"malformed disk fallback".to_vec());
    let options = LoadOptions {
        mode: ExecMode::Interp,
        vfs: fs,
        search_dirs: vec!["/libs".into()],
        resolver: Some(Arc::new(Resolver)),
        eager_dependencies: true,
        components: Default::default(), backends: Default::default(),
    };
    let program = LoadedProgram::load_bytes(&empty_zbc(), options).unwrap();
    assert!(program.module().functions.is_empty());
}

#[test]
fn fatal_error_keeps_the_report_for_the_cli_adapter() {
    use crate::vm::exec::stack_guard;
    let _lock = stack_guard::FATAL_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let program = program(ExecMode::Interp, false);
    let _ = stack_guard::fatal_error(program.context(), "host fatal regression");
    let error = program.run_entry(None).unwrap_err();
    assert!(error.to_string().contains("host fatal regression"));
    assert!(stack_guard::take_report()
        .unwrap()
        .contains("host fatal regression"));
}

#[test]
fn malformed_selected_eager_dependency_fails_during_load() {
    let fs = Arc::new(MemoryFs::new());
    fs.mount(std::path::Path::new("/libs").join("broken.zpkg").to_str().unwrap(), b"broken dependency".to_vec());
    let mut artifact = crate::assembly::loader::load_artifact_from_bytes(&empty_zbc()).unwrap();
    artifact.dependencies.push(zpkg::format::ZpkgDep {
        file: "broken.zpkg".into(), namespaces: Vec::new(), symbols: Vec::new(),
    });
    let options = LoadOptions { mode: ExecMode::Interp, vfs: fs,
        search_dirs: vec!["/libs".into()], eager_dependencies: true, resolver: None, components: Default::default(), backends: Default::default() };
    let error = super::load::prepare(artifact, None, &options).err().expect("must reject a corrupt dependency");
    assert!(format!("{error:#}").contains("broken.zpkg"));
}
