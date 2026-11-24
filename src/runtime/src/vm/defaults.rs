//! Host-owned defaults and low-level context construction inputs.
use crate::vm::VmContext;
use std::sync::Arc;


#[cfg(test)]
#[path = "defaults_tests.rs"]
mod tests;

/// Construct a context with explicit engine bindings; high-level hosts attach
/// selected components only after loading and metadata preparation have succeeded.
pub(crate) fn construct(
    module: Option<crate::metadata::Module>,
    vfs: Arc<dyn crate::pal::fs::Vfs>,
    extensions: crate::vm::VmExtensions,
    engines: crate::exec::EngineRegistry,
    freeze: fn(crate::vm::VmExtensions) -> crate::vm::ExtensionState,
) -> std::pin::Pin<Box<VmContext>> {
    let module = module.map(|mut module| {
        module.clear_link_state();
        Arc::new(module)
    });
    let ctx = with_config(
        module,
        vfs,
        freeze(extensions),
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
    protocol: crate::object::ObjectProtocol<VmContext, crate::exec::builtin_abi::BuiltinError>,
    engines: crate::exec::EngineRegistry,
) -> std::pin::Pin<Box<VmContext>> {
    let sampler = crate::diagnostics::Sampling::with_config(
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

#[cfg(feature = "jit-test-support")]
#[doc(hidden)]
pub fn jit_test_context(config: crate::config::RuntimeConfig) -> std::pin::Pin<Box<VmContext>> {
    with_config(None, crate::pal::fs::default_vfs(), crate::composition::freeze(Default::default()),
        config.vm_config(), crate::corelib::object_protocol(), crate::components::api::engine_registry())
}
