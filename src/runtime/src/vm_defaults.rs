//! Host-owned defaults and low-level context construction inputs.
use crate::vm::VmContext;
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
        extensions: crate::vm::VmExtensions,
    ) -> std::pin::Pin<Box<Self>> {
        construct(
            module,
            vfs,
            extensions,
            crate::components::api::engine_registry(),
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
        crate::loader::build_block_indices(&mut module);
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
    extensions: crate::vm::VmExtensions,
    engines: crate::exec::EngineRegistry,
) -> std::pin::Pin<Box<VmContext>> {
    let module = module.map(|mut module| {
        module.clear_link_state();
        Arc::new(module)
    });
    let ctx = with_config(
        module,
        vfs,
        crate::vm::services::freeze(extensions),
        crate::config::runtime_config().vm_config(),
        crate::corelib::object_protocol(),
        engines,
    );
    ctx
}

pub(crate) fn with_config(
    module: Option<Arc<crate::metadata::Module>>,
    vfs: Arc<dyn crate::pal::fs::Vfs>,
    extensions: crate::vm::ExtensionState,
    config: crate::config::VmConfig,
    protocol: crate::object::ObjectProtocol<VmContext, crate::exec::call::builtin_abi::BuiltinError>,
    engines: crate::exec::EngineRegistry,
) -> std::pin::Pin<Box<VmContext>> {
    let sampler = crate::diagnostics::sampling::Sampling::with_config(
        config.sample_hz, config.trace_timeline, crate::components::api::prepare_profiler,
    );
    VmContext::with_runtime(
        module,
        vfs,
        crate::vm::RuntimeInputs {
            extensions,
            config,
            object_protocol: protocol,
            engines,
            builtins: crate::interop::providers::functions(),
            catalog: crate::interop::providers::catalog(),
            fusion: crate::loader::FUSION_INPUTS,
            sampler,
        },
    )
}
