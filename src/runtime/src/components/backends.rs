//! Backend source selection and registration before any managed execution.
use super::options::ComponentOptions;
use anyhow::{bail, Result};
use std::path::PathBuf;
use crate::components::abi::*;
use crate::{zpkg::ExecMode, exec::EngineRegistry};

#[derive(Clone, Debug, Default)]
pub enum BackendSource {
    /// Prefer an explicitly linked implementation, then a colocated component.
    #[default]
    Auto,
    #[cfg(test)]
    Linked,
    #[cfg(test)]
    Dynamic(PathBuf),
    Disabled,
}
#[derive(Clone, Debug, Default)]
pub struct BackendOptions {
    pub interp: BackendSource,
    pub jit: BackendSource,
}
/// The native backend supports only desktop x64 and arm64 targets.
pub fn jit_supported() -> bool {
    cfg!(all(any(target_arch = "x86_64", target_arch = "aarch64"),
        any(target_os = "linux", target_os = "macos", target_os = "windows", target_os = "android", all(target_os = "ios", target_abi = "sim"))))
}
pub(super) fn linked(id: ComponentId) -> Option<ComponentInit> {
    if let Some(init) = super::linked::find(id) { return Some(init); }
    if id == INTERP_COMPONENT {
        #[cfg(test)]
        { return Some(crate::interp::component::z42_interp_init); }
    }
    if id == JIT_COMPONENT {
        #[cfg(all(test, feature = "jit", z42_jit_supported))]
        { return Some(crate::jit::component::z42_jit_init); }
    }
    #[cfg(test)]
    if id == PROFILER_COMPONENT { return Some(crate::profiler::component::z42_profiler_init); }
    None
}

fn dynamic_paths(id: ComponentId, policy: &crate::loader::NativeLoading) -> Vec<PathBuf> {
    let stem = if id == INTERP_COMPONENT { "z42_interp" } else if id == PROFILER_COMPONENT { "z42_profiler" } else { "z42_jit" };
    crate::pal::dynlib::candidate_paths(stem, policy.search_dirs.clone())
}
fn auto_available(id: ComponentId) -> bool {
    if linked(id).is_some() { return true; }
    cfg!(feature = "component-loading") && dynamic_paths(id, &Default::default()).iter().any(|path| known_source(path))
}
pub(super) fn known_source(path: &std::path::Path) -> bool {
    if path.is_file() { return true; }
    #[cfg(any(feature = "native-interop", feature = "component-loading"))]
    if crate::loader::code::is_loaded(path) { return true; }
    false
}
/// Default sources present in this runtime pack; actual loading still validates ABI and identity.
pub fn jit_available() -> bool { cfg!(feature = "jit") && jit_supported() && auto_available(INTERP_COMPONENT) && auto_available(JIT_COMPONENT) }
pub fn interp_available() -> bool { auto_available(INTERP_COMPONENT) }
pub(super) fn select(options: ComponentOptions, source: &BackendSource, id: ComponentId, name: &str, policy: &crate::loader::NativeLoading) -> Result<ComponentOptions> {
    let init = linked(id);
    match source {
        BackendSource::Disabled => bail!("{name} backend is disabled"),
        #[cfg(test)]
        BackendSource::Linked => {
            let init = init.ok_or_else(|| anyhow::anyhow!("{name} backend is not statically linked"))?;
            Ok(unsafe { options.with_linked(id, init, super::linked::origin(id)) })
        }
        BackendSource::Auto if init.is_some() && policy.allows(super::linked::origin(id)) => {
            let init = init.ok_or_else(|| anyhow::anyhow!("{name} backend is not statically linked"))?;
            Ok(unsafe { options.with_linked(id, init, super::linked::origin(id)) })
        }
        _ => {
            policy.check(crate::loader::NativeOrigin::RuntimeLoad)?;
            let path = match source {
                #[cfg(test)]
                BackendSource::Dynamic(path) => path.clone(),
                _ => {
                    let paths = dynamic_paths(id, policy);
                    paths.iter().find(|p| p.is_file() || crate::pal::dynlib::is_loader_name(p)).cloned().ok_or_else(|| anyhow::anyhow!(
                        "{name} backend unavailable: no linked selection and dynamic library was not found; searched {}",
                        paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")))?
                },
            };
            if !path.is_file() && !crate::pal::dynlib::is_loader_name(&path) {
                bail!("{name} backend unavailable: no linked selection and dynamic library `{}` was not found", path.display());
            }
            let symbol = if id == INTERP_COMPONENT { "z42_interp_init" } else if id == PROFILER_COMPONENT { "z42_profiler_init" } else { "z42_jit_init" };
            Ok(unsafe { options.with_dynamic(id, path, symbol) })
        }
    }
}
impl BackendOptions {
    pub(crate) fn components(&self, mode: ExecMode, user: &ComponentOptions, policy: &crate::loader::NativeLoading) -> Result<ComponentOptions> {
        if mode == ExecMode::Aot { bail!("AOT mode is not implemented yet"); }
        if mode == ExecMode::Jit && !jit_supported() { bail!("JIT backend is unsupported on this target"); }
        let extra = select(ComponentOptions::default(), &self.interp, INTERP_COMPONENT, "interpreter", policy)?;
        let extra = if mode == ExecMode::Jit { select(extra, &self.jit, JIT_COMPONENT, "JIT", policy)? } else { extra };
        Ok(user.merged(&extra))
    }
}

pub(crate) fn registry() -> EngineRegistry {
    #[cfg(test)]
    let baseline = Some(&crate::interp::engine::ENGINE as &'static dyn crate::exec::Engine);
    #[cfg(not(test))]
    let baseline = None;
    let registry = EngineRegistry::new(baseline);
    #[cfg(all(test, feature = "jit", z42_jit_supported))]
    { let _ = registry.register_jit(crate::jit::execution_engine(), crate::config::runtime_config().jit_config()); }
    registry
}
