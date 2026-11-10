//! Interop-owned per-VM state. Buffer pinning is portable; native registration is optional.
use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use std::collections::HashMap;
use crate::context::VmContext;
#[cfg(feature = "native-interop")]
use std::sync::Arc;

pub(crate) struct State {
    pub(super) providers: &'static super::providers::NativeRegistry,
    pub(super) external_modules: parking_lot::RwLock<HashMap<&'static str, std::sync::Arc<super::module::NativeModule>>>,
    pub(super) instances: parking_lot::RwLock<HashMap<&'static str, std::sync::Arc<super::module::ModuleInstance>>>,
    #[cfg(feature = "native-interop")]
    native_types: parking_lot::RwLock<HashMap<(String, String), Arc<super::RegisteredType>>>,
    pinned_owned_buffers: Mutex<HashMap<u64, Box<[u8]>>>,
}

pub(super) fn state(ctx: &VmContext) -> Result<&State> {
    ctx.ext::<State>().ok_or_else(|| anyhow!("missing interop VM state"))
}

/// Register a type once per (module, name), preserving the original on duplicate.
#[cfg(feature = "native-interop")]
pub fn register_native_type(ctx: &VmContext, ty: Arc<super::RegisteredType>) -> Result<bool> {
    let key = (ty.module().to_string(), ty.type_name().to_string());
    let mut map = state(ctx)?.native_types.write();
    if map.contains_key(&key) { return Ok(false); }
    map.insert(key, ty);
    Ok(true)
}

/// Resolve a previously registered native type by its module and name.
#[cfg(feature = "native-interop")]
pub fn resolve_native_type(ctx: &VmContext, module: &str, name: &str) -> Result<Option<Arc<super::RegisteredType>>> {
    let key = (module.to_string(), name.to_string());
    Ok(state(ctx)?.native_types.read().get(&key).cloned())
}

#[cfg(feature = "native-interop")]
pub fn native_type_count(ctx: &VmContext) -> Result<usize> {
    Ok(state(ctx)?.native_types.read().len())
}

/// Own an Array<u8> snapshot until its matching UnpinPtr or VM destruction.
pub fn pin_owned_buffer(ctx: &VmContext, buf: Box<[u8]>) -> Result<u64> {
    let ptr = buf.as_ptr() as u64;
    state(ctx)?.pinned_owned_buffers.lock().insert(ptr, buf);
    Ok(ptr)
}

/// Release a snapshot. Unknown pointers, including borrowed string pins, are a no-op.
pub fn release_owned_buffer(ctx: &VmContext, ptr: u64) -> Result<()> {
    let _ = state(ctx)?.pinned_owned_buffers.lock().remove(&ptr);
    Ok(())
}

pub fn pinned_owned_buffer_count(ctx: &VmContext) -> Result<usize> {
    Ok(state(ctx)?.pinned_owned_buffers.lock().len())
}

impl State {
    pub(super) fn new(providers: &'static super::providers::NativeRegistry) -> Self {
        Self { providers, external_modules: Default::default(), instances: Default::default(), pinned_owned_buffers: Default::default(),
            #[cfg(feature = "native-interop")]
            native_types: Default::default(),
        }
    }
}
