//! Facade-owned primary VM construction; thread mechanisms remain in vm::context.
use std::sync::Arc;
use crate::vm::context::VmContext;

pub trait VmContextFactory: Sized {
    fn new() -> std::pin::Pin<Box<Self>>;
    fn with_vfs(vfs: Arc<dyn crate::pal::fs::Vfs>) -> std::pin::Pin<Box<Self>>;
    fn with_module(module: crate::vm::runtime::Module) -> std::pin::Pin<Box<Self>>;
    fn with_module_and_vfs(module: crate::vm::runtime::Module, vfs: Arc<dyn crate::pal::fs::Vfs>) -> std::pin::Pin<Box<Self>>;
    fn with_extensions(module: Option<crate::vm::runtime::Module>, vfs: Arc<dyn crate::pal::fs::Vfs>, extensions: crate::vm::ext::VmExtensions) -> std::pin::Pin<Box<Self>>;
    fn with_image(image: Arc<crate::zpkg::image::PackageImage>) -> anyhow::Result<std::pin::Pin<Box<Self>>>;
    fn with_image_and_vfs(image: Arc<crate::zpkg::image::PackageImage>, vfs: Arc<dyn crate::pal::fs::Vfs>) -> anyhow::Result<std::pin::Pin<Box<Self>>>;
}

impl VmContextFactory for VmContext {
    /// Standard test entry: constructs a VmContext with `VmCore.module = None`.
    /// Cargo unit tests use this — they don't need a real Module for
    /// builtin / static-field / alloc tests. Production paths use
    /// [`with_module`](Self::with_module).
    fn new() -> std::pin::Pin<Box<Self>> {
        Self::with_extensions(None, crate::pal::fs::default_vfs(), Default::default())
    }

    /// [`new`](Self::new) on the file system `vfs` instead of the default one.
    fn with_vfs(vfs: Arc<dyn crate::pal::fs::Vfs>) -> std::pin::Pin<Box<Self>> {
        Self::with_extensions(None, vfs, Default::default())
    }

    /// Production entry: constructs a VmContext with the user's compiled
    /// Module wrapped in `Arc` for cross-thread sharing. Required by any
    /// path that may invoke `__thread_spawn` (which dispatches into the
    /// shared module from the spawned thread).
    fn with_module(module: crate::vm::runtime::Module) -> std::pin::Pin<Box<Self>> {
        Self::with_module_and_vfs(module, crate::pal::fs::default_vfs())
    }

    /// [`with_module`](Self::with_module) on the file system `vfs`: every path-based file
    /// builtin and every package load of this VM goes through it.
    fn with_module_and_vfs(
        module: crate::vm::runtime::Module,
        vfs: Arc<dyn crate::pal::fs::Vfs>,
    ) -> std::pin::Pin<Box<Self>> {
        Self::with_extensions(Some(module), vfs, Default::default())
    }

    /// Create a VM with owned extension registrations, frozen for its entire lifetime.
    /// `None` omits the entry module, as in [`Self::new`]. Module linkage is reset
    /// exactly as in [`Self::with_module_and_vfs`]. Worker contexts share this state.
    fn with_extensions(
        module: Option<crate::vm::runtime::Module>,
        vfs: Arc<dyn crate::pal::fs::Vfs>,
        extensions: crate::vm::ext::VmExtensions,
    ) -> std::pin::Pin<Box<Self>> {
        construct(module, vfs, extensions, crate::assembly::engine::registry())
    }

    /// Instantiate shared package definitions with fresh per-VM types and caches.
    fn with_image(
        image: Arc<crate::zpkg::image::PackageImage>,
    ) -> anyhow::Result<std::pin::Pin<Box<Self>>> {
        Self::with_image_and_vfs(image, crate::pal::fs::default_vfs())
    }

    /// Instantiate an image using this VM's file system for subsequent loads.
    fn with_image_and_vfs(
        image: Arc<crate::zpkg::image::PackageImage>,
        vfs: Arc<dyn crate::pal::fs::Vfs>,
    ) -> anyhow::Result<std::pin::Pin<Box<Self>>> {
        let mut module = crate::vm::runtime::Module::from_image(image);
        crate::vm::runtime::types::build_type_registry(&mut module);
        crate::vm::runtime::types::verify_constraints(&module)?;
        crate::assembly::loader::build_block_indices(&mut module);
        Ok(Self::with_module_and_vfs(module, vfs))
    }

}

#[cfg(test)]
#[path = "context_assembly_tests.rs"]
mod tests;

/// Construct a context with explicit engine bindings; high-level hosts attach
/// selected components only after loading and metadata preparation have succeeded.
pub(crate) fn construct(module: Option<crate::vm::runtime::Module>, vfs: Arc<dyn crate::pal::fs::Vfs>,
    extensions: crate::vm::ext::VmExtensions, engines: crate::vm::exec::engine::EngineRegistry) -> std::pin::Pin<Box<VmContext>> {
        let module = module.map(|mut module| {
            module.clear_link_state();
            Arc::new(module)
        });
        let ctx = with_config(module, vfs, crate::assembly::extension::freeze(extensions),
            crate::config::runtime_config().vm_config(), crate::assembly::object_protocol::services(), engines);
        #[cfg(feature = "native-interop")]
        if let Err(e) = crate::interop::ext::load_all(&ctx) { tracing::warn!("native ext loader: {e:#}"); }
        ctx
}

pub(crate) fn with_config(module: Option<Arc<crate::vm::runtime::Module>>, vfs: Arc<dyn crate::pal::fs::Vfs>,
    extensions: crate::vm::ext::ExtensionState, config: crate::vm::config::VmConfig,
    protocol: crate::vm::exec::object_protocol::ObjectProtocol, engines: crate::vm::exec::engine::EngineRegistry) -> std::pin::Pin<Box<VmContext>> {
    let backend = match config.sample_hz {
        Some(hz) => crate::profiler::cpu::Sampler::start(hz, config.trace_timeline),
        None => crate::profiler::cpu::Sampler::disabled(),
    };
    VmContext::with_runtime(module, vfs, crate::vm::context::RuntimeInputs {
        extensions, config, object_protocol: protocol, engines,
        builtins: crate::assembly::builtin::BUILTINS,
        catalog: &crate::assembly::builtin::CATALOG,
        fusion: crate::assembly::loader::FUSION_INPUTS,
        sampler: crate::vm::diagnostics::sampling::Sampling::new(backend.enabled(), backend),
    })
}
