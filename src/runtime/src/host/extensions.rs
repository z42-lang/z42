//! Facade assembly of core-library services; the VM stores only typed extension state.
use crate::corelib;
use crate::context::extensions::{ExtensionState, VmExtensions};

pub(crate) fn freeze(extensions: VmExtensions) -> ExtensionState {
    let mut defaults = VmExtensions::new();
    // Library code outlives every provider/type/resource destructor.
    #[cfg(feature = "native-interop")]
    add(&mut defaults, crate::loader::native::Libraries::default());
    // Interop providers must outlive heap, engines, user state and core-library resources.
    add(&mut defaults, crate::interop::state_for_vm(crate::host::builtins::registry()));
    add(&mut defaults, crate::exec::call::builtins::NativeProviderServices { invoke: crate::interop::invoke_export });
    #[cfg(feature = "native-interop")]
    add(&mut defaults, crate::exec::native::NativeServices {
        call: crate::interop::call_registered,
        pin_owned_buffer: crate::interop::pin_owned_buffer,
        release_owned_buffer: crate::interop::release_owned_buffer,
    });
    #[cfg(feature = "native-interop")]
    add(&mut defaults, crate::loader::native::SearchPaths(native_candidates));
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
    add(&mut defaults, corelib::tls::State::default());
    extensions.with_defaults(defaults).freeze()
}

fn add<T: std::any::Any + Send + Sync>(extensions: &mut VmExtensions, state: T) {
    // Only this assembly can register these private types; duplicates are a wiring error.
    assert!(extensions.register(state).is_ok(), "duplicate built-in VM extension type");
}

#[cfg(feature = "native-interop")]
fn repl_candidates() -> Vec<std::path::PathBuf> {
    let file = crate::pal::dynlib::file_name("z42_repl");
    let mut out = Vec::new();
    if let Some(over) = crate::config::runtime_config().repl_native.as_ref() {
        let p = over.clone();
        // Accept either a direct file path or a directory containing the lib.
        out.push(p.join(&file));
        out.push(p);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.join(&file));
            // <sdk>/programs/z42i/ (beside the interactive payload), via the
            // shared colocated-native-dependency resolver.
            if let Some(sdk_root) = dir.parent() {
                let z42i_dir = sdk_root.join("programs").join("z42i");
                if let Some(p) = crate::pal::dynlib::resolve_native_beside(&z42i_dir, "z42_repl") {
                    out.push(p);
                }
            }
        }
    }
    out
}

#[cfg(feature = "native-interop")]
fn native_candidates(name: &str) -> Vec<std::path::PathBuf> {
    if name == "z42_repl" { return repl_candidates(); }
    let file = crate::pal::dynlib::file_name(name);
    crate::components::native_paths().into_iter().map(|dir| dir.join(&file)).collect()
}
