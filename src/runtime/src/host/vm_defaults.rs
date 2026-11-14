//! Host-owned defaults and low-level context construction inputs.
use crate::context::VmContext;
use std::sync::Arc;

impl VmContext {
    /// Bare context without an entry module, useful for testing VM services.
    /// Hosts loading callable programs use `host::LoadedProgram`.
    pub fn new() -> std::pin::Pin<Box<Self>> {
        Self::with_extensions(None, crate::pal::fs::default_vfs(), Default::default())
    }

    /// [`new`](Self::new) on the file system `vfs` instead of the default one.
    pub fn with_vfs(vfs: Arc<dyn crate::pal::fs::Vfs>) -> std::pin::Pin<Box<Self>> {
        Self::with_extensions(None, vfs, Default::default())
    }

    /// Low-level context construction with the user's compiled
    /// Module wrapped in `Arc` for cross-thread sharing. Required by any
    /// path that may invoke `__thread_spawn` (which dispatches into the
    /// shared module from the spawned thread).
    pub fn with_module(module: crate::metadata::Module) -> std::pin::Pin<Box<Self>> {
        Self::with_module_and_vfs(module, crate::pal::fs::default_vfs())
    }

    /// [`with_module`](Self::with_module) on the file system `vfs`: every path-based file
    /// builtin and every package load of this VM goes through it.
    pub fn with_module_and_vfs(
        module: crate::metadata::Module,
        vfs: Arc<dyn crate::pal::fs::Vfs>,
    ) -> std::pin::Pin<Box<Self>> {
        Self::with_extensions(Some(module), vfs, Default::default())
    }

    /// Create a VM with owned extension registrations, frozen for its entire lifetime.
    /// `None` omits the entry module, as in [`Self::new`]. Module linkage is reset
    /// exactly as in [`Self::with_module_and_vfs`]. Worker contexts share this state.
    pub fn with_extensions(
        module: Option<crate::metadata::Module>,
        vfs: Arc<dyn crate::pal::fs::Vfs>,
        extensions: crate::context::extensions::VmExtensions,
    ) -> std::pin::Pin<Box<Self>> {
        construct(
            module,
            vfs,
            extensions,
            crate::components::backends::registry(),
        )
    }

    /// Instantiate shared package definitions with fresh per-VM types and caches.
    pub fn with_image(
        image: Arc<crate::zpkg::image::PackageImage>,
    ) -> anyhow::Result<std::pin::Pin<Box<Self>>> {
        Self::with_image_and_vfs(image, crate::pal::fs::default_vfs())
    }

    /// Instantiate an image using this VM's file system for subsequent loads.
    pub fn with_image_and_vfs(
        image: Arc<crate::zpkg::image::PackageImage>,
        vfs: Arc<dyn crate::pal::fs::Vfs>,
    ) -> anyhow::Result<std::pin::Pin<Box<Self>>> {
        let mut module = crate::metadata::Module::from_image(image);
        crate::metadata::types::build_type_registry(&mut module);
        crate::metadata::types::verify_constraints(&module)?;
        crate::host::artifact::build_block_indices(&mut module);
        Ok(Self::with_module_and_vfs(module, vfs))
    }
}

#[cfg(test)]
#[path = "vm_defaults_tests.rs"]
mod tests;

/// Construct a context with explicit engine bindings; high-level hosts attach
/// selected components only after loading and metadata preparation have succeeded.
pub(crate) fn construct(
    module: Option<crate::metadata::Module>,
    vfs: Arc<dyn crate::pal::fs::Vfs>,
    extensions: crate::context::extensions::VmExtensions,
    engines: crate::exec::engine::EngineRegistry,
) -> std::pin::Pin<Box<VmContext>> {
    let module = module.map(|mut module| {
        module.clear_link_state();
        Arc::new(module)
    });
    let ctx = with_config(
        module,
        vfs,
        crate::host::extensions::freeze(extensions),
        crate::config::runtime_config().vm_config(),
        crate::corelib::object_protocol(),
        engines,
    );
    ctx
}

pub(crate) fn with_config(
    module: Option<Arc<crate::metadata::Module>>,
    vfs: Arc<dyn crate::pal::fs::Vfs>,
    extensions: crate::context::extensions::ExtensionState,
    config: crate::config::VmConfig,
    protocol: crate::exec::object_protocol::ObjectProtocol,
    engines: crate::exec::engine::EngineRegistry,
) -> std::pin::Pin<Box<VmContext>> {
    let backend = match config.sample_hz {
        Some(hz) => crate::profiler::cpu::Sampler::start(hz, config.trace_timeline),
        None => crate::profiler::cpu::Sampler::disabled(),
    };
    VmContext::with_runtime(
        module,
        vfs,
        crate::context::RuntimeInputs {
            extensions,
            config,
            object_protocol: protocol,
            engines,
            builtins: crate::host::builtins::functions(),
            catalog: crate::host::builtins::catalog(),
            fusion: crate::host::artifact::FUSION_INPUTS,
            sampler: crate::diagnostics::sampling::Sampling::new(backend.enabled(), backend),
        },
    )
}
