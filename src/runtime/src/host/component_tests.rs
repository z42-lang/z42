use super::*;
use crate::vm::context::VmContext;
use std::sync::Mutex;
#[path = "../../tests/data/component/src/lib.rs"]
mod probe;
use probe::{CounterApi, A, B};
static LOCK: Mutex<()> = Mutex::new(());

fn options() -> ComponentOptions {
    unsafe {
        ComponentOptions::default()
            .with_static(B, probe::z42_component_probe_b_init)
            .with_static(A, probe::z42_component_probe_a_init)
    }
}
fn count(lease: &ComponentLease<CounterApi>) -> (u64, u64) {
    let (mut n, mut vm) = (0, 0);
    assert_eq!(
        unsafe { (lease.read)(lease.context(), &mut n, &mut vm) },
        ComponentStatus::OK
    );
    (n, vm)
}
fn load(components: ComponentOptions) -> anyhow::Result<LoadedProgram> {
    LoadedProgram::load_bytes(
        &super::runtime_tests::empty_zbc(),
        LoadOptions {
            mode: ExecMode::Interp,
            components,
            ..Default::default()
        },
    )
}
#[test]
fn static_factories_attach_automatically_and_instances_are_per_vm() {
    let _lock = LOCK.lock().unwrap();
    probe::probe_reset();
    let opts = options();
    let first = load(opts.clone()).unwrap();
    let second = load(opts).unwrap();
    assert_eq!(probe::probe_inits(), 2, "factory descriptors are shared");
    assert_eq!(probe::probe_live(), 4);
    assert_eq!(component_ids(first.context()), [A, B, INTERP_COMPONENT]);
    let a = component_interface::<CounterApi>(first.context(), A)
        .unwrap()
        .unwrap();
    let b = component_interface::<CounterApi>(second.context(), A)
        .unwrap()
        .unwrap();
    assert_eq!(
        count(&a).0,
        1,
        "B invoked its dependency during construction"
    );
    assert_ne!(count(&a).1, count(&b).1);
    assert_eq!(unsafe { (a.increment)(a.context()) }, ComponentStatus::OK);
    assert_eq!(count(&a).0, 2);
    assert_eq!(count(&b).0, 1);
    let worker = VmContext::new_with_core(first.context().core_arc());
    let shared = component_interface::<CounterApi>(&worker, A)
        .unwrap()
        .unwrap();
    assert_eq!(count(&shared), count(&a));
    drop((shared, worker, a, b, first, second));
    assert_eq!(probe::probe_live(), 0);
    assert_eq!(probe::probe_destroyed(), 0x2121);
}
#[test]
fn invalid_factories_and_dependency_graphs_never_create_instances() {
    let _lock = LOCK.lock().unwrap();
    let cases = [
        (
            A,
            probe::z42_component_probe_wrong_version_init as ComponentInit,
            "ABI version/size",
        ),
        (
            A,
            probe::z42_component_probe_wrong_build_init,
            "build identity",
        ),
        (B, probe::z42_component_probe_a_init, "unexpected ID"),
        (
            A,
            probe::z42_component_probe_no_callback_init,
            "no create_vm",
        ),
        (
            A,
            probe::z42_component_probe_unknown_status_init,
            "4294967295",
        ),
        (
            A,
            probe::z42_component_probe_null_span_init,
            "invalid descriptor span",
        ),
        (
            A,
            probe::z42_component_probe_null_init,
            "null or misaligned",
        ),
        (B, probe::z42_component_probe_b_init, "missing component"),
    ];
    for (id, init, message) in cases {
        probe::probe_reset();
        let opts = unsafe { ComponentOptions::default().with_static(id, init) };
        let err = match load(opts) {
            Err(e) => e,
            Ok(_) => panic!("accepted {message}"),
        };
        assert!(format!("{err:#}").contains(message), "{err:#}");
        assert_eq!(probe::probe_created(), 0);
        assert_eq!(probe::probe_live(), 0);
    }
    probe::probe_reset();
    let duplicate = unsafe {
        ComponentOptions::default()
            .with_static(A, probe::z42_component_probe_a_init)
            .with_static(A, probe::z42_component_probe_a_init)
    };
    assert!(load(duplicate)
        .err()
        .unwrap()
        .to_string()
        .contains("duplicate"));
    assert_eq!(probe::probe_inits(), 0);
    let cycle = unsafe {
        ComponentOptions::default()
            .with_static(A, probe::z42_component_probe_cycle_a_init)
            .with_static(B, probe::z42_component_probe_b_init)
    };
    assert!(load(cycle).err().unwrap().to_string().contains("cycle"));
    assert_eq!(probe::probe_created(), 0);
}
#[test]
fn partial_creation_and_invalid_interfaces_roll_back_in_reverse_order() {
    let _lock = LOCK.lock().unwrap();
    for failed in [
        probe::z42_component_probe_failed_init as ComponentInit,
        probe::z42_component_probe_wrong_interface_init,
    ] {
        probe::probe_reset();
        let ctx = VmContext::new();
        let opts = unsafe {
            ComponentOptions::default()
                .with_static(A, probe::z42_component_probe_a_init)
                .with_static(B, failed)
        };
        assert!(opts.install(&ctx).is_err());
        assert!(ctx.core.components.get().is_none());
        assert!(component_ids(&ctx).is_empty());
        assert_eq!(probe::probe_created(), 0x12);
        assert_eq!(probe::probe_destroyed(), 0x21);
        assert_eq!(probe::probe_live(), 0);
    }
}
#[test]
fn registrations_freeze_and_interface_leases_retain_the_vm() {
    let _lock = LOCK.lock().unwrap();
    probe::probe_reset();
    let program = load(options()).unwrap();
    let weak = std::sync::Arc::downgrade(&program.context().core_arc());
    assert!(options()
        .install(program.context())
        .err()
        .unwrap()
        .to_string()
        .contains("frozen"));
    assert!(component_interface::<VmInfoApi>(program.context(), A)
        .unwrap()
        .is_none());
    let lease = component_interface::<CounterApi>(program.context(), A)
        .unwrap()
        .unwrap();
    drop(program);
    assert!(weak.upgrade().is_some());
    assert_eq!(probe::probe_live(), 2);
    assert_eq!(count(&lease).0, 1);
    drop(lease);
    assert!(weak.upgrade().is_none());
    assert_eq!(probe::probe_live(), 0);
    assert_eq!(probe::probe_destroyed(), 0x21);
}
#[test]
fn default_backend_is_attached_without_optional_components() {
    let program = load(Default::default()).unwrap();
    assert_eq!(component_ids(program.context()), [INTERP_COMPONENT]);
    assert!(component_interface::<CounterApi>(program.context(), A)
        .unwrap()
        .is_none());
}

#[cfg(feature = "component-loading")]
fn fixture() -> &'static std::path::Path {
    static PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    PATH.get_or_init(|| {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let identity: String = COMPONENT_BUILD_ID[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let target = root
            .join("../../artifacts/build/runtime/tests/data/component")
            .join(identity);
        let output =
            std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
                .args(["build", "--offline", "--manifest-path"])
                .arg(root.join("tests/data/component/Cargo.toml"))
                .arg("--target-dir")
                .arg(&target)
                .arg("--target")
                .arg(COMPONENT_TARGET)
                .output()
                .unwrap();
        assert!(
            output.status.success(),
            "real component build failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let path = target
            .join(COMPONENT_TARGET)
            .join("debug")
            .join(crate::pal::dynlib::file_name("z42_component_probe"));
        assert!(
            path.is_file(),
            "component artifact missing: {}",
            path.display()
        );
        path
    })
}
#[cfg(feature = "component-loading")]
fn dynamic_options(second: &str) -> ComponentOptions {
    unsafe {
        ComponentOptions::default()
            .with_dynamic(B, fixture(), second)
            .with_dynamic(A, fixture(), "z42_component_probe_a_init")
    }
}
#[cfg(feature = "component-loading")]
unsafe fn symbol<T: Copy>(library: &crate::pal::dynlib::Library, name: &str) -> T {
    unsafe { library.symbol(name).unwrap() }
}
#[test]
#[cfg(feature = "component-loading")]
fn real_dynamic_library_shares_the_static_protocol_and_stays_loaded_for_leases() {
    let _lock = LOCK.lock().unwrap();
    let options = dynamic_options("z42_component_probe_b_init");
    let program = load(options.clone()).unwrap();
    let catalog = options.catalog_for_test();
    let library = catalog.factories[0].library().unwrap();
    let weak_library = std::sync::Arc::downgrade(library);
    let live: extern "C" fn() -> usize = unsafe { symbol(library, "probe_live") };
    assert_eq!(live(), 2);
    let first = component_interface::<CounterApi>(program.context(), A)
        .unwrap()
        .unwrap();
    let other = load(options.clone()).unwrap();
    let second = component_interface::<CounterApi>(other.context(), A)
        .unwrap()
        .unwrap();
    assert_ne!(count(&first).1, count(&second).1);
    assert_eq!(count(&first).0, 1);
    assert_eq!(
        unsafe { (first.increment)(first.context()) },
        ComponentStatus::OK
    );
    assert_eq!(count(&first).0, 2);
    assert_eq!(count(&second).0, 1);
    drop((second, other));
    assert_eq!(live(), 2);
    drop((options, catalog, program));
    assert!(weak_library.upgrade().is_some());
    assert_eq!(
        count(&first).0,
        2,
        "code and VM callbacks survive their original owners"
    );
    drop(first);
    assert!(
        weak_library.upgrade().is_none(),
        "last lease releases the library guard"
    );
}
#[test]
#[cfg(feature = "component-loading")]
fn dynamic_failures_reject_before_publication_and_destroy_partial_state() {
    let _lock = LOCK.lock().unwrap();
    let library = unsafe { crate::pal::dynlib::Library::open(fixture()).unwrap() };
    let reset: extern "C" fn() = unsafe { symbol(&library, "probe_reset") };
    let live: extern "C" fn() -> usize = unsafe { symbol(&library, "probe_live") };
    let created: extern "C" fn() -> u64 = unsafe { symbol(&library, "probe_created") };
    let destroyed: extern "C" fn() -> u64 = unsafe { symbol(&library, "probe_destroyed") };
    for entry in [
        "z42_component_probe_failed_init",
        "z42_component_probe_wrong_interface_init",
    ] {
        reset();
        let ctx = VmContext::new();
        assert!(dynamic_options(entry).install(&ctx).is_err());
        assert!(ctx.core.components.get().is_none());
        assert_eq!(created(), 0x12);
        assert_eq!(destroyed(), 0x21);
        assert_eq!(live(), 0);
    }
    for entry in [
        "z42_component_probe_wrong_version_init",
        "z42_component_probe_wrong_build_init",
        "z42_component_probe_unknown_status_init",
        "missing_entry",
    ] {
        reset();
        let opts = unsafe { ComponentOptions::default().with_dynamic(A, fixture(), entry) };
        assert!(load(opts).is_err());
        assert_eq!(created(), 0);
        assert_eq!(live(), 0);
    }
    let missing = unsafe {
        ComponentOptions::default().with_dynamic(
            A,
            fixture().with_file_name("missing-component"),
            "z42_component_probe_a_init",
        )
    };
    assert!(load(missing).is_err());
}
#[test]
#[cfg(not(feature = "component-loading"))]
fn unavailable_dynamic_loading_is_an_explicit_error() {
    let opts = unsafe { ComponentOptions::default().with_dynamic(A, "component", "init") };
    let error = load(opts).err().unwrap();
    assert!(format!("{error:#}").contains("component-loading"));
}

#[test]
fn undeclared_dependency_is_not_exposed_even_when_its_provider_exists() {
    let _lock = LOCK.lock().unwrap();
    probe::probe_reset();
    let opts = unsafe {
        ComponentOptions::default()
            .with_static(A, probe::z42_component_probe_a_init)
            .with_static(B, probe::z42_component_probe_undeclared_init)
    };
    assert!(load(opts).is_err());
    assert_eq!(probe::probe_created(), 1);
    assert_eq!(probe::probe_destroyed(), 1);
    assert_eq!(probe::probe_live(), 0);
}

use crate::assembly::context::VmContextFactory as _;
