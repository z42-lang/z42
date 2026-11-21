//! Default runtime composition for hosts and low-level VM constructors.
use crate::corelib;
use crate::vm::{ExtensionState, VmExtensions, VmContext};
use std::sync::Arc;

pub(crate) fn freeze(extensions: VmExtensions) -> ExtensionState {
    let mut defaults = VmExtensions::new();
    // Cache logical resolutions per VM; loader/code retains images independently.
    #[cfg(any(feature = "native-interop", feature = "component-loading"))]
    add(&mut defaults, crate::loader::native::Libraries::default());
    // Interop providers must outlive heap, engines, user state and core-library resources.
    add(&mut defaults, crate::interop::state_for_vm(crate::interop::providers::registry()));
    add(&mut defaults, crate::exec::builtins::NativeProviderServices { invoke: crate::interop::invoke_export });
    #[cfg(feature = "native-interop")]
    add(&mut defaults, crate::exec::native::NativeServices {
        call: crate::interop::call_registered,
        pin_owned_buffer: crate::interop::pin_owned_buffer,
        release_owned_buffer: crate::interop::release_owned_buffer,
    });
    #[cfg(any(feature = "native-interop", feature = "component-loading"))]
    add(&mut defaults, crate::loader::native::SearchPaths(crate::loader::native_paths::candidates));
    add(&mut defaults, crate::host::services::services());
    // The service owns only Rust arguments, never the core, so this sharing cannot form a cycle.
    let application = crate::host::app_state::state_for_vm();
    add(&mut defaults, application.clone());
    add(&mut defaults, corelib::os::process::State::default());
    add(&mut defaults, corelib::core::threading::State::default());
    add(&mut defaults, corelib::io::fs::State::new(application));
    #[cfg(feature = "net")]
    {
        add(&mut defaults, corelib::net::tcp::State::default());
        add(&mut defaults, corelib::net::udp::State::default());
    }
    #[cfg(feature = "tls")]
    add(&mut defaults, corelib::net::tls::State::default());
    extensions.with_defaults(defaults).freeze()
}

fn add<T: std::any::Any + Send + Sync>(extensions: &mut VmExtensions, state: T) {
    // Only this assembly can register these private types; duplicates are a wiring error.
    assert!(extensions.register(state).is_ok(), "duplicate built-in VM extension type");
}


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
        crate::vm::defaults::construct(
            module,
            vfs,
            extensions,
            crate::components::api::engine_registry(),
            freeze,
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
