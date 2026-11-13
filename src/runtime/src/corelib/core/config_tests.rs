use super::search_dirs;
use crate::{
    context::VmContext,
    host::{ExecMode, LoadOptions, LoadedProgram},
    object::Value,
};

fn strings(value: Value) -> Vec<String> {
    let Value::Array(array) = value else {
        panic!("expected search directory array")
    };
    let result = array
        .borrow()
        .iter_boxed()
        .map(|v| {
            let Value::Str(s) = v else {
                panic!("expected directory string")
            };
            s.to_string()
        })
        .collect();
    result
}

fn program(dirs: &[&str]) -> LoadedProgram {
    LoadedProgram::load_bytes(
        &crate::host::program_tests::empty_zbc(),
        LoadOptions {
            mode: ExecMode::Interp,
            search_dirs: dirs.iter().map(std::path::PathBuf::from).collect(),
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn module_search_paths_belong_to_each_loaded_program_and_its_workers() {
    let first = program(&["/first/app", "/first/libs"]);
    let second = program(&["/second/libs", "/second/app"]);
    assert_eq!(
        strings(search_dirs(first.context())),
        ["/first/app", "/first/libs"]
    );
    assert_eq!(
        strings(search_dirs(second.context())),
        ["/second/libs", "/second/app"]
    );
    let core = first.context().core_arc();
    std::thread::spawn(move || {
        let worker = VmContext::new_with_core(core);
        assert_eq!(strings(search_dirs(&worker)), ["/first/app", "/first/libs"]);
    })
    .join()
    .unwrap();
    assert_eq!(
        strings(search_dirs(first.context())),
        ["/first/app", "/first/libs"]
    );
}

#[test]
fn module_search_without_a_loader_is_empty() {
    let ctx = VmContext::new();
    assert!(strings(search_dirs(&ctx)).is_empty());
}

#[test]
fn module_search_returns_an_independent_snapshot() {
    let program = program(&["/app", "/libs"]);
    let ctx = program.context();
    let Value::Array(snapshot) = search_dirs(ctx) else {
        panic!("expected array")
    };
    snapshot.borrow().set_boxed(0, Value::Null);
    assert!(matches!(snapshot.borrow().get_boxed(0), Value::Null));
    assert_eq!(strings(search_dirs(ctx)), ["/app", "/libs"]);
}

#[test]
fn module_search_reflects_directories_added_by_later_loads() {
    let fs = std::sync::Arc::new(crate::pal::vfs::MemoryFs::new());
    fs.mount(
        "/plugins/extra.zbc",
        crate::host::program_tests::empty_zbc(),
    );
    let ctx = VmContext::with_vfs(fs);
    ctx.install_lazy_loader_with_deps(vec!["/app".into()], 0, Vec::new(), Vec::new());
    assert_eq!(strings(search_dirs(&ctx)), ["/app"]);
    ctx.load_module_into_vm("/plugins/extra.zbc").unwrap();
    assert_eq!(strings(search_dirs(&ctx)), ["/app", "/plugins"]);
}
