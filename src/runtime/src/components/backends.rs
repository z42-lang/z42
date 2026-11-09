//! Backend source selection and registration before any managed execution.
use super::{ComponentOptions, component_interface};
use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use crate::components::api::*;
use crate::{zpkg::ExecMode, vm::{context::VmContext, exec::engine::EngineRegistry}};

#[derive(Clone, Debug, Default)]
pub enum BackendSource {
    /// Prefer an explicitly linked implementation, then a colocated component.
    #[default]
    Auto,
    Linked,
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
        any(target_os = "linux", target_os = "macos", target_os = "windows")))
}
fn linked(id: ComponentId) -> Option<ComponentInit> {
    if let Some(init) = super::linked::find(id) { return Some(init); }
    if id == INTERP_COMPONENT {
        #[cfg(test)]
        { return Some(crate::interp::component::z42_interp_init); }
    }
    if id == JIT_COMPONENT {
        #[cfg(all(test, feature = "jit", z42_jit_supported))]
        { return Some(crate::jit::component::z42_jit_init); }
    }
    None
}
pub(crate) fn native_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // 1. Explicit override (CI / dev / power user). `Z42_NATIVE_PATH`
    //    is colon-separated on Unix, semicolon-separated on Windows.
    //    runtime-config-phase2 : centralised through
    //    `RuntimeConfig::native_search_paths` (pre-split at config init).
    paths.extend(crate::config::runtime_config().native_search_paths.iter().cloned());

    // 2. Default SDK layout, relative to the running z42vm binary:
    //      <exec_dir>/../native/    — SDK package layout (bin/ and native/ siblings)
    //      <exec_dir>/native/       — dev / cargo-target layout when scripts symlink
    //      <exec_dir>              — raw cargo-target layout (libz42_*.dylib sit next
    //                                 to z42vm directly; added  to remove
    //                                 the manual `ln -sf ../libz42_compression.dylib
    //                                 release/native/libz42_compression.dylib` setup
    //                                 step every fresh `cargo build` previously
    //                                 required). `parse_z42_lib_name` filters to
    //                                 `libz42_<name>.{so,dylib,dll}`, so dependency
    //                                 dylibs / `libz42.rlib` / `*.a` are ignored.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent_dir) = exe.parent() {
            if let Some(sdk_root) = parent_dir.parent() {
                paths.push(sdk_root.join("native"));
            }
            paths.push(parent_dir.join("native"));
            paths.push(parent_dir.to_path_buf());
        }
    }

    paths
}

fn dynamic_paths(id: ComponentId) -> Vec<PathBuf> {
    let stem = if id == INTERP_COMPONENT { "z42_interp" } else { "z42_jit" };
    native_paths().into_iter().map(|p| p.join(crate::pal::dynlib::file_name(stem))).collect()
}
fn auto_available(id: ComponentId) -> bool {
    if linked(id).is_some() { return true; }
    cfg!(feature = "component-loading") && dynamic_paths(id).iter().any(|p| p.is_file())
}
/// Default sources present in this runtime pack; actual loading still validates ABI and identity.
pub fn jit_available() -> bool { cfg!(feature = "jit") && jit_supported() && auto_available(INTERP_COMPONENT) && auto_available(JIT_COMPONENT) }
pub fn interp_available() -> bool { auto_available(INTERP_COMPONENT) }
fn select(options: ComponentOptions, source: &BackendSource, id: ComponentId, name: &str) -> Result<ComponentOptions> {
    let init = linked(id);
    match source {
        BackendSource::Disabled => bail!("{name} backend is disabled"),
        BackendSource::Linked => {
            let init = init.ok_or_else(|| anyhow::anyhow!("{name} backend is not statically linked"))?;
            Ok(unsafe { options.with_static(id, init) })
        }
        BackendSource::Auto if init.is_some() => {
            let init = init.ok_or_else(|| anyhow::anyhow!("{name} backend is not statically linked"))?;
            Ok(unsafe { options.with_static(id, init) })
        }
        BackendSource::Auto | BackendSource::Dynamic(_) => {
            let path = match source {
                BackendSource::Dynamic(path) => path.clone(),
                _ => {
                    let paths = dynamic_paths(id);
                    paths.iter().find(|p| p.is_file()).cloned().ok_or_else(|| anyhow::anyhow!(
                        "{name} backend unavailable: no linked selection and dynamic library was not found; searched {}",
                        paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")))?
                },
            };
            if !path.is_file() { bail!("{name} backend unavailable: no linked selection and dynamic library `{}` was not found", path.display()); }
            let symbol = if id == INTERP_COMPONENT { "z42_interp_init" } else { "z42_jit_init" };
            Ok(unsafe { options.with_dynamic(id, path, symbol) })
        }
    }
}
impl BackendOptions {
    pub(crate) fn components(&self, mode: ExecMode, user: &ComponentOptions) -> Result<ComponentOptions> {
        if mode == ExecMode::Aot { bail!("AOT mode is not implemented yet"); }
        if mode == ExecMode::Jit && !jit_supported() { bail!("JIT backend is unsupported on this target"); }
        let extra = select(ComponentOptions::default(), &self.interp, INTERP_COMPONENT, "interpreter")?;
        let extra = if mode == ExecMode::Jit { select(extra, &self.jit, JIT_COMPONENT, "JIT")? } else { extra };
        Ok(user.merged(&extra))
    }
}
pub(crate) fn attach(ctx: &VmContext, mode: ExecMode) -> Result<()> {
    for id in [INTERP_COMPONENT, JIT_COMPONENT] {
        if id == JIT_COMPONENT && mode != ExecMode::Jit { continue; }
        let name = if id == INTERP_COMPONENT { "interpreter" } else { "JIT" };
        let interface = component_interface::<ExecutionApi>(ctx, id)?
            .ok_or_else(|| anyhow::anyhow!("{name} component does not provide its execution interface"))?;
        let identity = unsafe { (interface.core_identity)() };
        if identity.image != super::core_identity() || identity.abi != super::CORE_ABI {
            bail!("{name} component core image or ABI mismatch; use components from the same shared-core runtime pack");
        }
        let code = ctx.core().components().get().and_then(|x| x.code_lease(id))
            .ok_or_else(|| anyhow::anyhow!("{name} component has no code lease"))?;
        // Acquire code lifetime before installing any backend-owned trait objects.
        ctx.core().exec().engines.retain_code(code);
        let cfg = crate::config::runtime_config().jit_config();
        let config = ExecutionConfig { threshold: cfg.threshold, osr_threshold: cfg.osr_threshold,
            interp_tierup: cfg.interp_tierup, profile: cfg.profile as u32, debug_promote: cfg.debug_promote as u32 };
        let status = unsafe { (interface.attach)((ctx as *const VmContext).cast(), &config) };
        anyhow::ensure!(status == ComponentStatus::OK, "{name} registration failed: status {}", status.0);
    }
    Ok(())
}

pub(crate) fn registry() -> EngineRegistry {
    #[cfg(test)]
    let baseline = Some(&crate::interp::engine::ENGINE as &'static dyn crate::vm::exec::engine::Engine);
    #[cfg(not(test))]
    let baseline = None;
    let registry = EngineRegistry::new(baseline);
    #[cfg(all(test, feature = "jit", z42_jit_supported))]
    { let _ = registry.register_jit(crate::jit::execution_engine(), crate::config::runtime_config().jit_config()); }
    registry
}
