//! Backend source selection and registration before any managed execution.
use super::ComponentOptions;
use anyhow::{bail, Result};
use std::path::PathBuf;
use crate::component_abi::*;
use crate::{zpkg::ExecMode, context::VmContext, exec::engine::EngineRegistry};

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

    // Embedded libraries need not live beside the application's executable.
    // Keep explicit and executable-relative overrides ahead of the core image.
    if let Some(image) = crate::pal::dynlib::image_path(crate::component_bridge::core_identity()) {
        if let Some(directory) = image.parent() {
            let directory = directory.to_path_buf();
            if !paths.contains(&directory) { paths.push(directory.clone()); }
            #[cfg(target_os = "ios")]
            { let bundled = directory.join("Frameworks"); if !paths.contains(&bundled) { paths.push(bundled); } }
            #[cfg(target_os = "ios")]
            if directory.extension().is_some_and(|extension| extension == "framework") {
                if let Some(frameworks) = directory.parent() {
                    let frameworks = frameworks.to_path_buf();
                    if !paths.contains(&frameworks) { paths.push(frameworks); }
                }
            }
        }
    }
    paths
}

fn dynamic_paths(id: ComponentId) -> Vec<PathBuf> {
    let stem = if id == INTERP_COMPONENT { "z42_interp" } else if id == PROFILER_COMPONENT { "z42_profiler" } else { "z42_jit" };
    crate::pal::dynlib::candidate_paths(stem, native_paths())
}
fn auto_available(id: ComponentId) -> bool {
    if linked(id).is_some() { return true; }
    cfg!(feature = "component-loading") && dynamic_paths(id).iter().any(|path| known_source(path))
}
pub(super) fn known_source(path: &std::path::Path) -> bool {
    if path.is_file() { return true; }
    #[cfg(any(feature = "native-interop", feature = "component-loading"))]
    if crate::pal::dynlib::component_is_loaded(path) { return true; }
    false
}
/// Default sources present in this runtime pack; actual loading still validates ABI and identity.
pub fn jit_available() -> bool { cfg!(feature = "jit") && jit_supported() && auto_available(INTERP_COMPONENT) && auto_available(JIT_COMPONENT) }
pub fn interp_available() -> bool { auto_available(INTERP_COMPONENT) }
pub(super) fn select(options: ComponentOptions, source: &BackendSource, id: ComponentId, name: &str) -> Result<ComponentOptions> {
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
    pub(crate) fn components(&self, mode: ExecMode, user: &ComponentOptions) -> Result<ComponentOptions> {
        if mode == ExecMode::Aot { bail!("AOT mode is not implemented yet"); }
        if mode == ExecMode::Jit && !jit_supported() { bail!("JIT backend is unsupported on this target"); }
        let extra = select(ComponentOptions::default(), &self.interp, INTERP_COMPONENT, "interpreter")?;
        let extra = if mode == ExecMode::Jit { select(extra, &self.jit, JIT_COMPONENT, "JIT")? } else { extra };
        Ok(user.merged(&extra))
    }
}

pub(crate) fn registry() -> EngineRegistry {
    #[cfg(test)]
    let baseline = Some(&crate::interp::engine::ENGINE as &'static dyn crate::exec::engine::Engine);
    #[cfg(not(test))]
    let baseline = None;
    let registry = EngineRegistry::new(baseline);
    #[cfg(all(test, feature = "jit", z42_jit_supported))]
    { let _ = registry.register_jit(crate::jit::execution_engine(), crate::config::runtime_config().jit_config()); }
    registry
}
