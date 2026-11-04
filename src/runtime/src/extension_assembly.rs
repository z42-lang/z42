//! Facade assembly of core-library services; the VM stores only typed extension state.
use crate::corelib;
use crate::vm::ext::{ExtensionState, VmExtensions};

pub(crate) fn freeze(extensions: VmExtensions) -> ExtensionState {
    let mut defaults = VmExtensions::new();
    // Interop providers must outlive heap, engines, user state and core-library resources.
    add(&mut defaults, crate::interop::state_for_vm());
    add(&mut defaults, crate::app_services::services());
    // The service owns only Rust arguments, never the core, so this sharing cannot form a cycle.
    let application = crate::app_state::state_for_vm();
    add(&mut defaults, application.clone());
    add(&mut defaults, corelib::process::State::default());
    add(&mut defaults, corelib::threading::State::default());
    add(&mut defaults, corelib::fs::State::new(application));
    #[cfg(feature = "net")]
    {
        add(&mut defaults, corelib::network::tcp::State::default());
        add(&mut defaults, corelib::network::udp::State::default());
    }
    #[cfg(feature = "tls")]
    add(&mut defaults, corelib::tls::State::default());
    extensions.with_defaults(defaults).freeze()
}

fn add<T: std::any::Any + Send + Sync>(extensions: &mut VmExtensions, state: T) {
    // Only this assembly can register these private types; duplicates are a wiring error.
    assert!(extensions.register(state).is_ok(), "duplicate built-in VM extension type");
}
