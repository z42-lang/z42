use super::{LoadOptions, LoadedProgram};
use crate::{
    zpkg::{self, BasicBlock, ExecMode, Instruction, Terminator},
    object::Value,
    pal::fs::MemoryFs,
    vm::VmContext, metadata::Module,
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
    crate::loader::build_block_indices(&mut module);
    crate::loader::package::build_func_index(&mut module);
    let ctx = VmContext::with_module(module);
    crate::vm::assembly::resolve_tokens(&ctx, ctx.module().unwrap());
    LoadedProgram::new(ctx, mode, Some("Demo.Main".into())).unwrap()
}

use crate::loader::test_support::empty_zbc;

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
        .statics().set_named("Demo.sentinel", Value::I64(91))
        .unwrap();
    program.run_entry(None).unwrap();
    assert_eq!(
        program.context().statics().get_named("Demo.sentinel"),
        Value::I64(91)
    );
    assert!(program.invoke("missing", &[]).is_err());
    assert!(program
        .invoke("Demo.Main", &[Value::I64(1)])
        .unwrap_err()
        .to_string()
        .contains("arg-count-mismatch"));
    let other = self::program(ExecMode::Interp, false);
    assert_eq!(other.context().statics().get_named("Demo.sentinel"), Value::Null);
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
        native: Default::default(), allow_profiling: true,
    };
    let program = LoadedProgram::load_bytes(&empty_zbc(), options).unwrap();
    assert!(program.module().functions.is_empty());
}

#[test]
fn fatal_error_keeps_the_report_for_the_cli_adapter() {
    use crate::exec::stack_guard;
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
    let mut artifact = crate::loader::load_artifact_from_bytes(&empty_zbc()).unwrap();
    artifact.dependencies.push(zpkg::format::ZpkgDep {
        file: "broken.zpkg".into(), namespaces: Vec::new(), symbols: Vec::new(),
    });
    let error = crate::loader::prepare_startup(
        artifact, None, &*fs, vec!["/libs".into()], None, true, None,
    )
    .err()
    .expect("must reject a corrupt dependency");
    assert!(format!("{error:#}").contains("broken.zpkg"));
}

/// The session image cache shares decoded definitions across programs (the
/// C# ALC "already loaded ⇒ shared" rule): identical artifact content loaded
/// twice through one cache decodes exactly once and hands every program a
/// shallow view whose function-definition `Arc`s are pointer-equal.
#[test]
fn session_cache_shares_definitions_between_programs() {
    use crate::zpkg::ir::{self, Instruction};
    fn artifact() -> crate::loader::LoadedArtifact {
        let module = crate::metadata::Module::from_ir(ir::Module {
            name: "Shared".into(),
            string_pool: vec!["constant".into()],
            classes: Vec::new(),
            functions: vec![ir::Function {
                name: "Shared.Main".into(),
                param_count: 0,
                ret_type: "void".into(),
                exec_mode: ir::ExecMode::Interp,
                blocks: vec![ir::BasicBlock {
                    label: "entry".into(),
                    instructions: vec![Instruction::ConstI64 { dst: 0, val: 7 }],
                    terminator: ir::Terminator::Ret { reg: Some(0) },
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
            }],
            func_index: Default::default(),
        });
        crate::loader::LoadedArtifact {
            module,
            entry_hint: None,
            dependencies: Vec::new(),
            import_namespaces: vec!["Shared".into()],
            impl_pairs: Vec::new(),
            package_name: Some("shared_fixture".into()),
        }
    }
    let cache = crate::loader::ImageCache::default();
    let id = crate::loader::content_id(b"identical-bytes");
    let decode = || Ok::<_, anyhow::Error>(artifact());
    let first = cache.artifact_for(&id, decode).expect("first decode");
    let second = cache.artifact_for(&id, decode).expect("second load shares the decode");
    assert_eq!(cache.len(), 1, "identical content decodes exactly once per session");
    assert_eq!(first.module.functions.len(), second.module.functions.len());
    for (fa, fb) in first.module.functions.iter().zip(second.module.functions.iter()) {
        assert!(
            std::sync::Arc::ptr_eq(&fa.shared_definition(), &fb.shared_definition()),
            "definitions must be Arc-shared between programs of one session"
        );
    }
    // Metadata survives the hit path intact (resolver/dep loops depend on it).
    assert_eq!(second.import_namespaces, vec!["Shared".to_string()]);
    assert_eq!(second.package_name.as_deref(), Some("shared_fixture"));
}

/// merge-retirement M1 (design §4): the no-eager-merge track registers the
/// decoded dependency packages per-package through the lazy loader instead of
/// merging them into the entry module — and the program still runs. The
/// fixture carries real stdlib dependencies (embedding hello), exercised on
/// the interp tier.
#[test]
#[cfg(z42_have_embedding_hello)]
fn no_eager_merge_track_registers_packages_and_runs() {
    let bytes: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/embedding_hello.zbc"));
    let libs = std::path::PathBuf::from(env!("Z42_EMBEDDING_TEST_LIBS"));
    let options = crate::host::LoadOptions {
        mode: crate::host::ExecMode::Interp,
        search_dirs: vec![libs],
        ..Default::default()
    };
    let program = super::LoadedProgram::load_bytes(bytes, options)
        .expect("no-eager-merge track loads");
    // The stdlib must NOT be merged into the entry module on this track: the
    // entry's own functions stay, cross-package calls resolve through the
    // global FuncTable on first dispatch.
    let module = program.module();
    assert!(
        !module.functions.iter().any(|f| f.name.starts_with("Std.")),
        "stdlib functions must live in their own packages, not the entry module"
    );
    let outcome = program.invoke("Embedding.Hello.Main", &[]);
    let value = outcome.expect("entry runs on the per-package track");
    // The fixture's Main is void: a clean Ok(None) IS the success proof here.
    assert!(value.is_none(), "embedding hello Main is void; got {value:?}");
}

/// Unit-level repro: drive `register_decoded_artifact` directly against a
/// fresh VM (bypassing prepare's VFS-driven collection) and assert the
/// dependency's function lands in the global FuncTable.
#[test]
fn no_eager_merge_unit_registration_lands_in_func_table() {
    use crate::zpkg::ir::{self, Instruction, Terminator};
    let dep = crate::loader::LoadedArtifact {
        module: crate::metadata::Module::from_ir(ir::Module {
            name: "Dep".into(),
            string_pool: Vec::new(),
            classes: Vec::new(),
            functions: vec![ir::Function {
                name: "Dep.Work".into(),
                param_count: 0,
                ret_type: "void".into(),
                exec_mode: ir::ExecMode::Interp,
                blocks: vec![ir::BasicBlock {
                    label: "entry".into(),
                    instructions: vec![Instruction::ConstI64 { dst: 0, val: 1 }],
                    terminator: Terminator::Ret { reg: Some(0) },
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
            }],
            func_index: Default::default(),
        }),
        entry_hint: None,
        dependencies: Vec::new(),
        import_namespaces: Vec::new(),
        impl_pairs: Vec::new(),
        package_name: Some("dep_pkg".into()),
    };
    let options = crate::host::LoadOptions {
        mode: crate::host::ExecMode::Interp,
        ..Default::default()
    };
    let program = super::LoadedProgram::load_bytes(&empty_zbc(), options).expect("loads");
    let ctx = program.context();
    ctx.loader().register_eager_artifact(dep).expect("registration runs");
    assert!(
        ctx.funcs().id_of("Dep.Work").is_some(),
        "registered dep function must be resolvable by name after explicit registration"
    );
}

/// M4 dry-run CI repro shape: bare entry zbc + a single libs search dir
/// holding the full stdlib (the C component-verify harness's exact shape —
/// `cfg.search_paths = [pkg/libs]`, `z42_host_load_zbc(bytes)`). The entry
/// reaches `Std.Compression.*` with NO declared deps: cross-package symbols
/// must resolve through the lazy loader's on-demand path on BOTH tracks.
#[test]
fn bare_entry_resolves_undeclared_stdlib_package_from_search_dir() {
    // Needs the built runtime's native dir (z42_compression dylib); the CI
    // matrix always has it, local bare `cargo test` does not.
    if std::env::var_os("Z42_NATIVE_PATH").is_none() {
        eprintln!("skipping: Z42_NATIVE_PATH not set");
        return;
    }
    let wt = std::path::PathBuf::from("/Users/d.s.qiu/Documents/z42-new/wt/host-simplify");
    let zbc = std::fs::read("/tmp/ecp.zbc").expect("fixture zbc (built by this session)");
    let libs = wt.join("artifacts/intermediate/libraries/flat/release");
    let options = crate::host::LoadOptions {
        mode: crate::host::ExecMode::Interp,
        search_dirs: vec![libs],
        ..Default::default()
    };
    let program = super::LoadedProgram::load_bytes(&zbc, options).expect("loads");
    // CI sequence: resolve + invoke Echo three times first (warms the core
    // package through the same C-harness flow), then the compression probe.
    for i in 40u64..43 {
        let outcome = program.invoke("EngineComponentProbe.Echo", &[crate::host::Value::I64(i as i64)]);
        assert_eq!(outcome.expect("echo runs").map(|v| match v { crate::host::Value::I64(n) => n, _ => -1 }), Some((i + (i - 39)) as i64));
    }
    let outcome = program.invoke("EngineComponentProbe.CompressionRoundtrip", &[]);
    let value = outcome.expect("compression roundtrip resolves on the default track");
    assert_eq!(value, Some(crate::host::Value::I64(36)), "1..8 gzip roundtrip sums to 36");
}
