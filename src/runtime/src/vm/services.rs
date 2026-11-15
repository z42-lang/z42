//! Facade assembly of core-library services; the VM stores only typed extension state.
use crate::corelib;
use crate::vm::{ExtensionState, VmExtensions};

pub(crate) fn freeze(extensions: VmExtensions) -> ExtensionState {
    let mut defaults = VmExtensions::new();
    // Cache logical resolutions per VM; loader/code retains images independently.
    #[cfg(any(feature = "native-interop", feature = "component-loading"))]
    add(&mut defaults, crate::loader::native::Libraries::default());
    // Interop providers must outlive heap, engines, user state and core-library resources.
    add(&mut defaults, crate::interop::state_for_vm(crate::interop::providers::registry()));
    add(&mut defaults, crate::exec::call::builtins::NativeProviderServices { invoke: crate::interop::invoke_export });
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

