//! Process-lifetime symbol providers for explicitly linked stdlib native libraries.
use std::{collections::BTreeMap, ffi::{c_char, c_void}, sync::OnceLock};
use anyhow::{ensure, Result};
use parking_lot::RwLock;

pub type NativeSymbolResolver = unsafe extern "C" fn(*const c_char) -> *const c_void;
fn providers() -> &'static RwLock<BTreeMap<String, NativeSymbolResolver>> {
    static PROVIDERS: OnceLock<RwLock<BTreeMap<String, NativeSymbolResolver>>> = OnceLock::new();
    PROVIDERS.get_or_init(Default::default)
}
/// Register a linked library without resolving or initializing its symbols.
///
/// # Safety
/// The resolver and returned functions must obey the library's C ABI and remain
/// valid for the process lifetime. Register before creating hosts or invoking code.
pub unsafe fn register_static_library(name: &str, resolver: NativeSymbolResolver) -> Result<()> {
    ensure!(super::KNOWN_EXT_LIBS.iter().any(|(library, _)| format!("z42_{library}") == name),
        "unknown stdlib native library `{name}`");
    let mut registered = providers().write();
    if let Some(previous) = registered.get(name) {
        ensure!(std::ptr::fn_addr_eq(*previous, resolver), "native library `{name}` already has a different provider");
    } else {
        ensure!(super::LOADED_COMPRESSION.get().is_none(), "native library `{name}` is already loaded");
        registered.insert(name.to_owned(), resolver);
    }
    Ok(())
}
pub(super) fn find(name: &str) -> Option<NativeSymbolResolver> {
    providers().read().get(name).copied()
}
