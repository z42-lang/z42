use super::*;
use crate::component_abi::*;
use crate::host::{LoadedProgram, LoadOptions, ExecMode};
use crate::context::VmContext;
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
        &crate::host::program_tests::empty_zbc(),
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
    let opts = unsafe { ComponentOptions::default()
        .with_static(B, probe::z42_component_probe_b_init)
        .with_static_provider(A, probe::z42_component_probe_a_init) };
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
pub(super) fn fixture() -> &'static std::path::Path {
    static PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    PATH.get_or_init(|| {
        let root = std::path::PathBuf::from(env!("Z42_RUNTIME_DIR"));
        let identity: String = COMPONENT_BUILD_ID[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let target = root
            .join("../../artifacts/build/runtime/tests/data/component")
            .join(identity);
        let output =
            std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
                .args(["build", "--locked", "--offline", "--manifest-path"])
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
        weak_library.upgrade().is_some(),
        "native code remains mapped for thread-local destructors"
    );
    assert_eq!(live(), 0, "image retention must not retain VM instances");
    let same = unsafe { crate::pal::dynlib::Library::open_component(fixture()).unwrap() };
    assert!(std::sync::Arc::ptr_eq(&same, &weak_library.upgrade().unwrap()));
    let alias = fixture().parent().unwrap().join(".").join(fixture().file_name().unwrap());
    let aliased = unsafe { crate::pal::dynlib::Library::open_component(&alias).unwrap() };
    assert!(std::sync::Arc::ptr_eq(&same, &aliased));
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

const TRANSITIVE: ComponentId = ComponentId::new(0x300);
static TRANSITIVE_DEPS: [ComponentId; 1] = [B];
static DEPS_LIVE_AT_DESTROY: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
unsafe extern "C" fn create_transitive(runtime: *const RuntimeApi, vm: VmHandle, out: *mut InstanceHandle) -> ComponentStatus {
    unsafe { *out = std::ptr::null_mut(); }
    let mut dependency = InterfaceRef::NONE;
    let status = unsafe { ((*runtime).query_component)(vm, B, CounterApi::ID, CounterApi::VERSION, &mut dependency) };
    if status != ComponentStatus::OK { return status; }
    let api = unsafe { &*dependency.api.cast::<CounterApi>() };
    unsafe { (api.increment)(dependency.context) }
}
unsafe extern "C" fn destroy_transitive(_: InstanceHandle) {
    DEPS_LIVE_AT_DESTROY.store(probe::probe_live(), std::sync::atomic::Ordering::SeqCst);
}
unsafe extern "C" fn query_transitive(_: InstanceHandle, _: InterfaceId, _: u32, _: *mut InterfaceRef) -> ComponentStatus {
    ComponentStatus::UNAVAILABLE
}
static TRANSITIVE_DESCRIPTOR: ComponentDescriptor = ComponentDescriptor {
    header: ApiHeader::of::<ComponentDescriptor>(COMPONENT_ABI_VERSION),
    build_id: COMPONENT_BUILD_ID,
    id: TRANSITIVE,
    dependencies: IdSpan { data: TRANSITIVE_DEPS.as_ptr(), len: 1 },
    interfaces: InterfaceSpan { data: std::ptr::null(), len: 0 },
    create_vm: Some(create_transitive),
    destroy_vm: Some(destroy_transitive),
    query_interface: Some(query_transitive),
};
unsafe extern "C" fn transitive_init() -> *const ComponentDescriptor { &TRANSITIVE_DESCRIPTOR }

#[test]
fn selected_roots_activate_transitive_providers_before_instance_creation() {
    let _lock = LOCK.lock().unwrap();
    probe::probe_reset();
    DEPS_LIVE_AT_DESTROY.store(0, std::sync::atomic::Ordering::SeqCst);
    let options = unsafe { ComponentOptions::default()
        .with_static(TRANSITIVE, transitive_init)
        .with_static_provider(B, probe::z42_component_probe_b_init)
        .with_static_provider(A, probe::z42_component_probe_a_init) };
    let program = load(options).unwrap();
    assert_eq!(component_ids(program.context()), [A, B, TRANSITIVE, INTERP_COMPONENT]);
    assert_eq!(probe::probe_created(), 0x12);
    let b = component_interface::<CounterApi>(program.context(), B).unwrap().unwrap();
    assert_eq!(count(&b).0, 1, "the root invoked its already-created direct dependency");
    drop(b);
    drop(program);
    assert_eq!(DEPS_LIVE_AT_DESTROY.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert_eq!(probe::probe_destroyed(), 0x21);
}

#[test]
fn unused_static_and_dynamic_providers_are_not_loaded_by_install_or_query() {
    let _lock = LOCK.lock().unwrap();
    probe::probe_reset();
    let options = unsafe { ComponentOptions::default()
        .with_static_provider(A, probe::z42_component_probe_wrong_version_init)
        .with_dynamic_provider(B, "/no-such-component-library", "no_such_entry") };
    let program = load(options).unwrap();
    assert_eq!(component_ids(program.context()), [INTERP_COMPONENT]);
    assert!(component_interface::<CounterApi>(program.context(), A).unwrap().is_none());
    assert!(component_interface::<CounterApi>(program.context(), B).unwrap().is_none());
    assert_eq!(probe::probe_inits(), 0);
    assert_eq!(probe::probe_created(), 0);
}

#[test]
fn provider_cycles_duplicates_and_creation_failures_preserve_atomicity() {
    let _lock = LOCK.lock().unwrap();
    probe::probe_reset();
    let duplicate = unsafe { ComponentOptions::default()
        .with_static_provider(A, probe::z42_component_probe_a_init)
        .with_static_provider(A, probe::z42_component_probe_a_init)
        .with_static(B, probe::z42_component_probe_b_init) };
    assert!(load(duplicate).err().unwrap().to_string().contains("duplicate"));
    assert_eq!(probe::probe_inits(), 0);
    let cycle = unsafe { ComponentOptions::default()
        .with_static(B, probe::z42_component_probe_b_init)
        .with_static_provider(A, probe::z42_component_probe_cycle_a_init) };
    assert!(load(cycle).err().unwrap().to_string().contains("cycle"));
    assert_eq!(probe::probe_created(), 0);
    probe::probe_reset();
    let failure = unsafe { ComponentOptions::default()
        .with_static(B, probe::z42_component_probe_failed_init)
        .with_static_provider(A, probe::z42_component_probe_a_init) };
    let ctx = VmContext::new();
    assert!(failure.install(&ctx).is_err());
    assert!(component_ids(&ctx).is_empty());
    assert_eq!(probe::probe_destroyed(), 0x21);
    assert_eq!(probe::probe_live(), 0);
}

#[cfg(feature = "component-loading")]
#[test]
fn a_static_root_can_activate_a_dynamic_dependency_provider() {
    let _lock = LOCK.lock().unwrap();
    probe::probe_reset();
    let options = unsafe { ComponentOptions::default()
        .with_static(B, probe::z42_component_probe_b_init)
        .with_dynamic_provider(A, fixture(), "z42_component_probe_a_init") };
    let program = load(options).unwrap();
    let a = component_interface::<CounterApi>(program.context(), A).unwrap().unwrap();
    assert_eq!(count(&a).0, 1, "the static root called its dynamic dependency during creation");
    assert_eq!(component_ids(program.context()), [A, B, INTERP_COMPONENT]);
    drop((a, program));
    assert_eq!(probe::probe_live(), 0);
}
