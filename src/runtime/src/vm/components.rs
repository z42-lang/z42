//! Per-VM component instances. Factories own code; instances own state and borrowed
//! service contexts. Worker contexts share this registry through their VmCore.
use super::context::VmCore;
use anyhow::{bail, Result};
use parking_lot::Mutex;
use std::{
    collections::BTreeMap, ffi::c_void, marker::PhantomData, ops::Deref, ptr::NonNull, sync::Arc,
};
use z42_component_api::*;

pub(crate) struct Factory {
    pub id: ComponentId,
    pub dependencies: Vec<ComponentId>,
    pub interfaces: Vec<InterfaceSpec>,
    pub create: CreateVm,
    pub destroy: DestroyVm,
    pub query: QueryInterface,
    #[cfg(feature = "component-loading")]
    pub library: Option<Arc<crate::pal::dynlib::Library>>,
}

#[derive(Clone, Copy)]
struct CheckedInterface {
    value: InterfaceRef,
    spec: InterfaceSpec,
}
// Unsafe factory registration requires immutable tables and thread-safe callbacks.
// Service contexts and the owning instance keep their pointers alive.
unsafe impl Send for CheckedInterface {}
unsafe impl Sync for CheckedInterface {}

struct SharedServices {
    vm_id: u64,
    interfaces: Mutex<BTreeMap<(ComponentId, InterfaceId), CheckedInterface>>,
}
struct ServiceContext {
    shared: Arc<SharedServices>,
    allowed: Vec<ComponentId>,
}

unsafe extern "C" fn vm_id(vm: VmHandle, out: *mut u64) -> ComponentStatus {
    if vm.is_null() || out.is_null() {
        return ComponentStatus::INVALID_ARGUMENT;
    }
    unsafe {
        *out = (&*vm.cast::<ServiceContext>()).shared.vm_id;
    }
    ComponentStatus::OK
}
static VM_INFO: VmInfoApi = VmInfoApi {
    header: ApiHeader::of::<VmInfoApi>(1),
    vm_id,
};
unsafe extern "C" fn get_service(
    vm: VmHandle,
    id: InterfaceId,
    version: u32,
    out: *mut InterfaceRef,
) -> ComponentStatus {
    if vm.is_null() || out.is_null() {
        return ComponentStatus::INVALID_ARGUMENT;
    }
    unsafe {
        *out = InterfaceRef::NONE;
    }
    if id != VmInfoApi::ID || version != VmInfoApi::VERSION {
        return ComponentStatus::UNAVAILABLE;
    }
    unsafe {
        *out = InterfaceRef {
            api: &VM_INFO.header,
            context: vm,
        };
    }
    ComponentStatus::OK
}
unsafe extern "C" fn query_component(
    vm: VmHandle,
    component: ComponentId,
    interface: InterfaceId,
    version: u32,
    out: *mut InterfaceRef,
) -> ComponentStatus {
    if vm.is_null() || out.is_null() {
        return ComponentStatus::INVALID_ARGUMENT;
    }
    unsafe {
        *out = InterfaceRef::NONE;
    }
    let ctx = unsafe { &*vm.cast::<ServiceContext>() };
    if !ctx.allowed.contains(&component) {
        return ComponentStatus::UNAVAILABLE;
    }
    let map = ctx.shared.interfaces.lock();
    match map.get(&(component, interface)) {
        Some(value) if value.spec.version == version => {
            unsafe {
                *out = value.value;
            }
            ComponentStatus::OK
        }
        _ => ComponentStatus::UNAVAILABLE,
    }
}
static RUNTIME_API: RuntimeApi = RuntimeApi {
    header: ApiHeader::of::<RuntimeApi>(COMPONENT_ABI_VERSION),
    get_service,
    query_component,
};

struct Instance {
    factory: Arc<Factory>,
    state: InstanceHandle,
    context: Arc<ServiceContext>,
    interfaces: BTreeMap<InterfaceId, CheckedInterface>,
}
// The unsafe registration contract covers component state and callback concurrency.
// Arc owners prevent destruction while an interface or a VM can call the component.
unsafe impl Send for Instance {}
unsafe impl Sync for Instance {}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            (self.factory.destroy)(self.state);
        }
        let mut map = self.context.shared.interfaces.lock();
        for id in self.interfaces.keys() {
            map.remove(&(self.factory.id, *id));
        }
    }
}
impl Instance {
    fn create(factory: Arc<Factory>, shared: Arc<SharedServices>) -> Result<Arc<Self>> {
        let context = Arc::new(ServiceContext {
            allowed: factory.dependencies.clone(),
            shared,
        });
        let mut state = std::ptr::null_mut();
        let status = unsafe {
            (factory.create)(
                &RUNTIME_API,
                Arc::as_ptr(&context) as *mut c_void,
                &mut state,
            )
        };
        if status != ComponentStatus::OK && state.is_null() {
            bail!(
                "component {} create_vm failed: status {}",
                factory.id,
                status.0
            );
        }
        // Even failed creation can return a destroyable partial instance. The guard
        // owns it before checking status or interfaces, so all exits clean it up.
        let mut instance = Self {
            factory,
            state,
            context,
            interfaces: BTreeMap::new(),
        };
        if status != ComponentStatus::OK {
            bail!(
                "component {} create_vm failed: status {}",
                instance.factory.id,
                status.0
            );
        }
        for spec in &instance.factory.interfaces {
            let mut value = InterfaceRef::NONE;
            let status =
                unsafe { (instance.factory.query)(state, spec.id, spec.version, &mut value) };
            if status != ComponentStatus::OK
                || value.api.is_null()
                || (value.api as usize) % spec.alignment as usize != 0
            {
                bail!(
                    "component {} did not provide interface {}: status {}",
                    instance.factory.id,
                    spec.id,
                    status.0
                );
            }
            let header = unsafe { &*value.api };
            if header.version != spec.version || header.size != spec.size {
                bail!(
                    "component {} interface {} version/size mismatch",
                    instance.factory.id,
                    spec.id
                );
            }
            instance
                .interfaces
                .insert(spec.id, CheckedInterface { value, spec: *spec });
        }
        let instance = Arc::new(instance);
        {
            let mut map = instance.context.shared.interfaces.lock();
            for (id, value) in &instance.interfaces {
                map.insert((instance.factory.id, *id), *value);
            }
        }
        Ok(instance)
    }
}

pub(crate) struct VmComponents {
    instances: Vec<Arc<Instance>>,
}
impl Drop for VmComponents {
    fn drop(&mut self) {
        // Dependency users stop before their providers, on rollback and normal drop.
        while let Some(instance) = self.instances.pop() {
            drop(instance);
        }
    }
}
impl VmComponents {
    pub fn create(factories: &[Arc<Factory>], vm_id: u64) -> Result<Self> {
        let mut result = Self {
            instances: Vec::new(),
        };
        if factories.is_empty() {
            return Ok(result);
        }
        let shared = Arc::new(SharedServices {
            vm_id,
            interfaces: Mutex::new(BTreeMap::new()),
        });
        for factory in factories {
            result
                .instances
                .push(Instance::create(Arc::clone(factory), Arc::clone(&shared))?);
        }
        Ok(result)
    }
    pub fn ids(&self) -> Vec<ComponentId> {
        self.instances.iter().map(|x| x.factory.id).collect()
    }
    pub fn lease<T: ComponentInterface>(
        &self,
        id: ComponentId,
        core: Arc<VmCore>,
    ) -> Result<Option<ComponentLease<T>>> {
        let Some(instance) = self.instances.iter().find(|x| x.factory.id == id) else {
            return Ok(None);
        };
        let Some(value) = instance.interfaces.get(&T::ID) else {
            return Ok(None);
        };
        if value.spec.version != T::VERSION
            || value.spec.size as usize != std::mem::size_of::<T>()
            || value.spec.alignment as usize != std::mem::align_of::<T>()
            || (value.value.api as usize) % std::mem::align_of::<T>() != 0
        {
            bail!(
                "component {id} interface {} does not match requested table",
                T::ID
            );
        }
        let table = NonNull::new(value.value.api as *mut T)
            .ok_or_else(|| anyhow::anyhow!("component {id} returned a null table"))?;
        Ok(Some(ComponentLease {
            _instance: Arc::clone(instance),
            _core: core,
            table,
            context: value.value.context,
            marker: PhantomData,
        }))
    }
}

/// A borrowed component table with ownership of its VM, instance and loaded code.
/// Drop this lease after its last callback; copied function pointers alone do not
/// extend component lifetime. Callbacks use the handle returned by context().
pub struct ComponentLease<T: ComponentInterface> {
    // Release the extra instance reference before releasing the VM; the VM then
    // destroys the unique instances in dependency order while all services live.
    _instance: Arc<Instance>,
    _core: Arc<VmCore>,
    table: NonNull<T>,
    context: InstanceHandle,
    marker: PhantomData<T>,
}
unsafe impl<T: ComponentInterface> Send for ComponentLease<T> {}
unsafe impl<T: ComponentInterface> Sync for ComponentLease<T> {}
impl<T: ComponentInterface> ComponentLease<T> {
    pub fn context(&self) -> InstanceHandle {
        self.context
    }
}
impl<T: ComponentInterface> Deref for ComponentLease<T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { self.table.as_ref() }
    }
}
