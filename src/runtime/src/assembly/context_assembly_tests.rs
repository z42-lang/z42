use super::VmContext;
use crate::config::RuntimeConfig;
use crate::gc::GcMode;

#[test]
fn captured_vm_inputs_choose_independent_heap_modes() {
    let mut settings = RuntimeConfig::from_getter(|_| None);
    settings.gc_mode = GcMode::StwMarkSweep;
    let stw = settings.vm_config();
    settings.gc_mode = GcMode::GenerationalMarkSweep;
    let generational = settings.vm_config();
    let first = configured_context(
        None,
        crate::pal::fs::default_vfs(),
        crate::assembly::extension::freeze(Default::default()),
        stw,
        crate::assembly::object_protocol::services(),
    );
    let second = configured_context(
        None,
        crate::pal::fs::default_vfs(),
        crate::assembly::extension::freeze(Default::default()),
        generational,
        crate::assembly::object_protocol::services(),
    );
    assert_eq!(first.heap().mode(), GcMode::StwMarkSweep);
    assert_eq!(second.heap().mode(), GcMode::GenerationalMarkSweep);
    assert_eq!(first.heap().mode(), GcMode::StwMarkSweep);
}

#[test]
fn worker_retains_facade_resources_until_last_context_drop() {
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    use crate::pal::fs::Vfs;
    struct Service(Arc<AtomicUsize>);
    impl Drop for Service {
        fn drop(&mut self) { self.0.fetch_add(1, Ordering::SeqCst); }
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let fs = Arc::new(crate::pal::vfs::MemoryFs::new());
    fs.mount("/owned", b"shared".to_vec());
    let vfs: Arc<dyn Vfs> = fs;
    let mut extensions = crate::vm::ext::VmExtensions::new();
    extensions.register(Service(drops.clone())).unwrap();
    let primary = VmContext::with_extensions(None, vfs.clone(), extensions);
    let core = Arc::downgrade(&primary.core_arc());
    let worker = VmContext::new_with_core(primary.core_arc());
    assert_eq!(worker.core.threads.vm_contexts.lock().len(), 2);
    assert!(std::ptr::eq(primary.ext::<Service>().unwrap(), worker.ext::<Service>().unwrap()));
    drop(primary);
    assert!(core.upgrade().is_some());
    assert_eq!(worker.core.threads.vm_contexts.lock().len(), 1);
    assert!(Arc::ptr_eq(worker.vfs(), &vfs));
    assert_eq!(worker.vfs().read("/owned").unwrap(), b"shared");
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(worker);
    assert!(core.upgrade().is_none());
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

use crate::object::{NativeData, TypeDesc, Value};
use crate::vm::exec::call::builtin_abi::BuiltinError;
use crate::vm::exec::object_protocol::ObjectProtocol;
use crate::vm::exec::ops::{stringify::with_obj_str, vcall::{resolve_vcall, VCallTarget}};
use std::{cell::Cell, sync::Arc};

thread_local! { static PROTOCOL_CALLS: Cell<usize> = const { Cell::new(0) }; }

fn protocol_context(protocol: ObjectProtocol) -> std::pin::Pin<Box<VmContext>> {
    configured_context(
        None, crate::pal::fs::default_vfs(),
        crate::assembly::extension::freeze(Default::default()),
        RuntimeConfig::from_getter(|_| None).vm_config(), protocol,
    )
}

fn protocol_module() -> crate::vm::runtime::Module {
    crate::vm::runtime::Module::new(crate::zpkg::image::ModuleData {
        name: "ProtocolTest".into(), string_pool: vec![], classes: vec![],
        func_index: Default::default(),
    }, vec![], Default::default())
}

fn protocol_type(name: &str) -> Arc<TypeDesc> {
    Arc::new(TypeDesc {
        class_flags: 0, visibility: 0, name: name.into(), base_name: None,
        fields: vec![], field_index: Default::default(), vtable: vec![],
        vtable_index: Default::default(), cold: None,
        id: crate::vm::runtime::ids::tokens::TypeId(
            crate::vm::runtime::ids::tokens::alloc_type_id_block(1)),
    })
}

fn protocol_box(ctx: &VmContext, name: &str) -> Value {
    let Value::Object(obj) = ctx.heap().alloc_boxed_prim(
        protocol_type(name), 42i64.to_le_bytes().to_vec().into_boxed_slice(),
    ) else { panic!("expected allocated object") };
    Value::BoxedStruct(obj)
}

fn registered_get_type(ctx: &VmContext, value: &Value) -> Result<Value, BuiltinError> {
    PROTOCOL_CALLS.with(|calls| calls.set(calls.get() + 1));
    let Value::BoxedStruct(obj) = value else { panic!("receiver must remain boxed") };
    assert!(ctx.module().is_none(), "the consumer must pass its own context");
    Ok(Value::I64(if obj.type_desc().name == "Std.Int64" { 64 } else { 42 }))
}

fn registered_string(value: &Value) -> Result<String, BuiltinError> {
    PROTOCOL_CALLS.with(|calls| calls.set(calls.get() + 1));
    assert!(matches!(value, Value::Object(_)));
    Ok("registered default".into())
}

#[test]
fn object_protocol_boxed_get_type_consumes_registration_for_both_receiver_kinds() {
    PROTOCOL_CALLS.with(|calls| calls.set(0));
    let ctx = protocol_context(ObjectProtocol {
        get_type: registered_get_type, ..crate::assembly::object_protocol::services()
    });
    let module = protocol_module();
    for (name, expected) in [("Std.Int64", 64), ("Demo.Pair", 42)] {
        let boxed = protocol_box(&ctx, name);
        let root = ctx.heap().pin_root(boxed.clone());
        let result = resolve_vcall(&ctx, &module, &boxed, "GetType", 0, None).unwrap();
        assert!(matches!(result.target, VCallTarget::Immediate(Value::I64(n)) if n == expected));
        let (Value::BoxedStruct(before), Value::BoxedStruct(after)) = (&boxed, result.this) else {
            panic!("resolved receiver must remain boxed")
        };
        assert!(crate::object::ObjRef::ptr_eq(before, &after));
        ctx.heap().unpin_root(root);
    }
    assert_eq!(PROTOCOL_CALLS.with(Cell::get), 2);
}

#[test]
fn object_protocol_string_fallback_uses_registration_but_scalars_bypass_it() {
    PROTOCOL_CALLS.with(|calls| calls.set(0));
    let ctx = protocol_context(ObjectProtocol {
        default_to_string: registered_string, ..crate::assembly::object_protocol::services()
    });
    let module = protocol_module();
    let object = ctx.heap().alloc_object(protocol_type("Demo.Plain"), vec![], NativeData::None);
    let root = ctx.heap().pin_root(object.clone());
    assert_eq!(with_obj_str(&ctx, &module, &object, str::to_owned).unwrap(), "registered default");
    assert_eq!(with_obj_str(&ctx, &module, &Value::I64(19), str::to_owned).unwrap(), "19");
    assert_eq!(PROTOCOL_CALLS.with(Cell::get), 1);
    ctx.heap().unpin_root(root);
}

#[test]
fn object_protocol_consumer_errors_keep_the_original_channels() {
    fn get_type(_: &VmContext, _: &Value) -> Result<Value, BuiltinError> {
        Err(BuiltinError::Internal("registered GetType failure".into()))
    }
    fn default_to_string(_: &Value) -> Result<String, BuiltinError> {
        Err(BuiltinError::Internal("registered ToString failure".into()))
    }
    let ctx = protocol_context(ObjectProtocol { get_type, default_to_string, ..crate::assembly::object_protocol::services() });
    let module = protocol_module();
    let boxed = protocol_box(&ctx, "Std.Int64");
    let root = ctx.heap().pin_root(boxed.clone());
    let Err(error) = resolve_vcall(&ctx, &module, &boxed, "GetType", 0, None) else {
        panic!("registered error must propagate")
    };
    assert!(error.to_string().contains("registered GetType failure"));
    ctx.heap().unpin_root(root);
    let object = ctx.heap().alloc_object(protocol_type("Demo.Plain"), vec![], NativeData::None);
    let root = ctx.heap().pin_root(object.clone());
    let error = with_obj_str(&ctx, &module, &object, str::to_owned).unwrap_err();
    assert!(error.to_string().contains("registered ToString failure"));
    assert!(ctx.take_pending_thrown().is_none());
    ctx.heap().unpin_root(root);
}

#[test]
fn object_protocol_default_services_are_shared_with_workers_without_retaining_the_core() {
    let primary = VmContext::new();
    let other = VmContext::with_vfs(crate::pal::fs::default_vfs());
    let worker = VmContext::new_with_core(primary.core_arc());
    assert!(std::ptr::eq(&primary.core.exec.object_protocol, &worker.core.exec.object_protocol));
    assert!(!std::ptr::eq(&primary.core.exec.object_protocol, &other.core.exec.object_protocol));
    let weak = Arc::downgrade(&primary.core_arc());
    drop(primary);
    let protocol = worker.core.exec.object_protocol;
    assert_eq!((protocol.default_to_string)(&Value::Null).unwrap(), "null");
    // Reflection needs the Std.Type shell; a bare VM deliberately returns null.
    worker.install_lazy_loader(None, 0);
    worker.core.loader.lazy_loader.write().as_mut().unwrap()
        .insert_type("Std.Type".into(), protocol_type("Std.Type"));
    let receiver = protocol_box(&worker, "Std.Int64");
    let root = worker.heap().pin_root(receiver);
    let reflected = (protocol.get_type)(&worker, &receiver).unwrap();
    let Value::Object(ty) = reflected else { panic!("registered provider must materialize Std.Type") };
    assert_eq!(ty.type_desc().name, "Std.Type");
    worker.heap().unpin_root(root);
    assert!(matches!((protocol.get_type)(&worker, &Value::Null), Err(BuiltinError::Legacy(_))));
    drop(worker);
    assert!(weak.upgrade().is_none());
}

#[test]
fn method_type_materialization_uses_registered_providers_on_primary_and_worker() {
    fn named(ctx: &VmContext, name: &str) -> Value {
        assert!(ctx.module().is_none());
        assert_eq!(name, "Demo.Named");
        PROTOCOL_CALLS.with(|calls| calls.set(calls.get() + 1));
        Value::I64(41)
    }
    fn constructed(ctx: &VmContext, name: &str, args: &[String]) -> Value {
        assert!(ctx.module().is_none());
        assert_eq!(name, "T");
        assert!(args.is_empty());
        PROTOCOL_CALLS.with(|calls| calls.set(calls.get() + 1));
        Value::I64(42)
    }
    PROTOCOL_CALLS.with(|calls| calls.set(0));
    let primary = protocol_context(ObjectProtocol {
        make_type_from_name: named,
        make_constructed_type: constructed,
        ..crate::assembly::object_protocol::services()
    });
    let args = vec!["Demo.Named".to_string()];
    use crate::vm::exec::call::type_param_env::method_type_value;
    assert!(matches!(method_type_value(&primary, &args, 0), Value::I64(41)));
    assert!(matches!(method_type_value(&primary, &args, 1), Value::I64(42)));
    let worker = VmContext::new_with_core(primary.core_arc());
    drop(primary);
    assert!(matches!(method_type_value(&worker, &args, 0), Value::I64(41)));
    assert!(matches!(method_type_value(&worker, &[], 0), Value::I64(42)));
    assert_eq!(PROTOCOL_CALLS.with(Cell::get), 4);
}

#[test]
fn stack_allocation_policy_is_per_vm_and_shared_with_workers() {
    use crate::vm::exec::call::stack_alloc::{stack_alloc_enabled, stats_enabled};
    let mut contexts = Vec::new();
    for (value, enabled, stats) in [("on", true, false), ("off", false, false), ("stats", true, true)] {
        let mut settings = RuntimeConfig::from_getter(|_| None);
        settings.stackalloc = Some(value.into());
        let ctx = configured_context(None, crate::pal::fs::default_vfs(),
            crate::assembly::extension::freeze(Default::default()), settings.vm_config(),
            crate::assembly::object_protocol::services());
        let worker = VmContext::new_with_core(ctx.core.clone());
        assert_eq!(stack_alloc_enabled(&ctx), enabled);
        assert_eq!(stats_enabled(&ctx), stats);
        assert_eq!(stack_alloc_enabled(&worker), enabled);
        assert_eq!(stats_enabled(&worker), stats);
        contexts.push((ctx, enabled, stats));
    }
    for (ctx, enabled, stats) in contexts {
        assert_eq!(stack_alloc_enabled(&ctx), enabled);
        assert_eq!(stats_enabled(&ctx), stats);
    }
}

#[test]
fn safepoint_policy_is_per_vm_and_shared_by_worker_resets() {
    use crate::vm::threads::safepoint::{throttle_n, check_safepoint};
    use std::sync::atomic::Ordering;
    let mut contexts = Vec::new();
    for throttle in [1, 7, 1024] {
        let mut settings = RuntimeConfig::from_getter(|_| None);
        settings.safepoint_throttle = throttle;
        let ctx = configured_context(None, crate::pal::fs::default_vfs(),
            crate::assembly::extension::freeze(Default::default()), settings.vm_config(),
            crate::assembly::object_protocol::services());
        let worker = VmContext::new_with_core(ctx.core.clone());
        for view in [&ctx, &worker] {
            assert_eq!(throttle_n(view), throttle);
            assert_eq!(view.threads.safepoint_skip.load(Ordering::Relaxed), throttle);
            view.threads.safepoint_skip.store(1, Ordering::Relaxed);
            check_safepoint(view);
            assert_eq!(view.threads.safepoint_skip.load(Ordering::Relaxed), throttle);
            #[cfg(all(feature = "jit", z42_jit_supported))]
            unsafe {
                crate::jit::helpers::control::jit_check_safepoint_slow(std::ptr::null_mut(), &**view);
                assert_eq!(view.threads.safepoint_skip.load(Ordering::Relaxed), throttle);
            }
        }
        contexts.push((ctx, throttle));
    }
    for (ctx, throttle) in contexts { assert_eq!(throttle_n(&ctx), throttle); }
}


fn configured_context(module: Option<std::sync::Arc<crate::vm::runtime::Module>>, vfs: std::sync::Arc<dyn crate::pal::fs::Vfs>,
    extensions: crate::vm::ext::ExtensionState, config: crate::vm::config::VmConfig,
    protocol: crate::vm::exec::object_protocol::ObjectProtocol) -> std::pin::Pin<Box<VmContext>> {
    super::with_config(module, vfs, extensions, config, protocol, crate::assembly::engine::registry())
}
