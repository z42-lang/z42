//! Load and validate factories before creating any per-VM instances.
use crate::vm::components::Factory;
use anyhow::{bail, Context, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{Arc, OnceLock},
};
use z42_component_api::*;

#[derive(Clone)]
pub(super) enum Registration {
    Static {
        id: ComponentId,
        init: ComponentInit,
        cache: Arc<OnceLock<Result<Arc<Factory>, String>>>,
    },
    Dynamic {
        id: ComponentId,
        path: PathBuf,
        symbol: String,
        cache: Arc<OnceLock<Result<Arc<Factory>, String>>>,
    },
}
pub(super) struct Catalog {
    pub factories: Vec<Arc<Factory>>,
}

/// Safety comes from unsafe registration: descriptors/spans are immutable and valid
/// while the factory's code is retained. Header validation precedes full reads.
unsafe fn copy_span<T: Copy>(data: *const T, len: usize) -> Result<Vec<T>> {
    if len == 0 {
        return Ok(Vec::new());
    }
    if data.is_null()
        || (data as usize) % std::mem::align_of::<T>() != 0
        || len > isize::MAX as usize / std::mem::size_of::<T>().max(1)
    {
        bail!("invalid descriptor span");
    }
    Ok(unsafe { std::slice::from_raw_parts(data, len) }.to_vec())
}
unsafe fn read_descriptor(id: ComponentId, init: ComponentInit) -> Result<Factory> {
    let ptr = unsafe { init() };
    if ptr.is_null() || (ptr as usize) % std::mem::align_of::<ComponentDescriptor>() != 0 {
        bail!("component {id} returned a null or misaligned descriptor");
    }
    let header = unsafe { ptr.cast::<ApiHeader>().read() };
    if header.version != COMPONENT_ABI_VERSION
        || header.size as usize != std::mem::size_of::<ComponentDescriptor>()
    {
        bail!("component {id} descriptor ABI version/size mismatch");
    }
    let descriptor = unsafe { ptr.read() };
    if descriptor.id != id {
        bail!("component {id} returned unexpected ID {}", descriptor.id);
    }
    if descriptor.build_id != COMPONENT_BUILD_ID {
        bail!("component {id} build identity mismatch");
    }
    let dependencies =
        unsafe { copy_span(descriptor.dependencies.data, descriptor.dependencies.len) }?;
    let interfaces = unsafe { copy_span(descriptor.interfaces.data, descriptor.interfaces.len) }?;
    let mut ids = BTreeSet::new();
    for dependency in &dependencies {
        if !ids.insert(*dependency) {
            bail!("component {id} has duplicate dependency {dependency}");
        }
    }
    let mut ids = BTreeSet::new();
    for spec in &interfaces {
        if !ids.insert(spec.id)
            || spec.version == 0
            || (spec.size as usize) < std::mem::size_of::<ApiHeader>()
            || !spec.alignment.is_power_of_two()
            || (spec.alignment as usize) < std::mem::align_of::<ApiHeader>()
        {
            bail!(
                "component {id} has an invalid or duplicate interface {}",
                spec.id
            );
        }
    }
    unsafe { Factory::from_validated_descriptor(&descriptor, dependencies, interfaces) }
}
fn load(registration: &Registration) -> Result<Factory> {
    match registration {
        Registration::Static { id, init, .. } => unsafe { read_descriptor(*id, *init) },
        Registration::Dynamic { id, path, symbol, .. } => {
            #[cfg(feature = "component-loading")]
            {
                let library = Arc::new(unsafe { crate::pal::dynlib::Library::open(path) }?);
                let init = unsafe { library.symbol::<ComponentInit>(symbol) }?;
                let factory = unsafe { read_descriptor(*id, init) }?;
                Ok(unsafe { factory.with_library(library) })
            }
            #[cfg(not(feature = "component-loading"))]
            {
                let _ = (path, symbol);
                bail!("component {id}: dynamic component loading requires the `component-loading` feature");
            }
        }
    }
}
fn cached_load(registration: &Registration) -> Result<Arc<Factory>> {
    let cache = match registration { Registration::Static { cache, .. } | Registration::Dynamic { cache, .. } => cache };
    cache.get_or_init(|| load(registration).map(Arc::new).map_err(|e| format!("{e:#}")))
        .as_ref().map(Arc::clone).map_err(|e| anyhow::anyhow!("{e}"))
}
impl Catalog {
    pub fn load(registrations: &[Registration]) -> Result<Self> {
        let mut factories = BTreeMap::new();
        // Reject duplicates before native initialization has side effects.
        for registration in registrations {
            let id = match registration {
                Registration::Static { id, .. } | Registration::Dynamic { id, .. } => *id,
            };
            if factories.insert(id, None).is_some() {
                bail!("duplicate component {id}");
            }
        }
        for registration in registrations {
            let factory = cached_load(registration).context("loading runtime component")?;
            factories.insert(factory.id(), Some(factory));
        }
        let mut pending = BTreeMap::new();
        for (id, factory) in factories {
            pending.insert(
                id,
                factory.ok_or_else(|| anyhow::anyhow!("component {id} was not loaded"))?,
            );
        }
        for factory in pending.values() {
            for dep in factory.dependencies() {
                if !pending.contains_key(dep) {
                    bail!("component {} requires missing component {dep}", factory.id());
                }
            }
        }
        let mut done = BTreeSet::new();
        let mut ordered = Vec::new();
        while !pending.is_empty() {
            // Stable IDs order equally-ready providers; never rely on file/hash order.
            let id = pending
                .iter()
                .find(|(_, f)| f.dependencies().iter().all(|x| done.contains(x)))
                .map(|(id, _)| *id)
                .ok_or_else(|| anyhow::anyhow!("component dependency cycle"))?;
            let factory = pending
                .remove(&id)
                .ok_or_else(|| anyhow::anyhow!("component {id} disappeared"))?;
            done.insert(id);
            ordered.push(factory);
        }
        Ok(Self { factories: ordered })
    }
}
