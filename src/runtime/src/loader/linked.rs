//! Process-lifetime native symbol resolvers supplied by statically linked hosts.
use anyhow::{Result, bail};
use parking_lot::RwLock;
use std::{
    collections::HashMap,
    ffi::{CString, c_char, c_void},
    sync::OnceLock,
};

pub type NativeSymbolResolver = unsafe extern "C" fn(*const c_char) -> *const c_void;

/// The stdlib native libraries a statically linked host may supply. Hosts
/// register implementations by logical name (`z42_<library>`); anything else
/// is a configuration mistake and must fail loudly instead of sitting in the
/// table unread (C host self-check pins this).
const KNOWN_STATIC_LIBRARIES: &[&str] = &[
    "z42_compression",
    // Synthetic library driven through the generic declaration/lazy-load
    // machinery by interop module tests.
    #[cfg(test)]
    "native.lazy.test",
];

#[derive(Default)]
struct Providers(RwLock<HashMap<String, NativeSymbolResolver>>);
impl Providers {
    fn register(&self, name: &str, resolver: NativeSymbolResolver) -> Result<()> {
        if name.is_empty() || name.bytes().any(|byte| matches!(byte, 0 | b'/' | b'\\')) {
            bail!("invalid logical native library name");
        }
        if !KNOWN_STATIC_LIBRARIES.contains(&name) {
            bail!("unknown stdlib native library `{name}`");
        }
        let mut providers = self.0.write();
        if let Some(previous) = providers.get(name) {
            if !std::ptr::fn_addr_eq(*previous, resolver) {
                bail!("native library `{name}` already has a different static provider");
            }
        } else {
            providers.insert(name.to_owned(), resolver);
        }
        Ok(())
    }

    fn resolve(&self, library: &str, symbol: &str) -> Option<Result<*mut c_void>> {
        let resolver = self.0.read().get(library).copied()?;
        Some((|| {
            let symbol = CString::new(symbol)?;
            // Third-party code must never run under the provider table lock.
            let pointer = unsafe { resolver(symbol.as_ptr()) };
            if pointer.is_null() {
                bail!(
                    "static library `{library}` does not export {}",
                    symbol.to_string_lossy()
                );
            }
            Ok(pointer.cast_mut())
        })())
    }
}

fn providers() -> &'static Providers {
    static PROVIDERS: OnceLock<Providers> = OnceLock::new();
    PROVIDERS.get_or_init(Providers::default)
}

/// Supply a library without initializing it or probing its symbols.
///
/// # Safety
/// Register before creating hosts or invoking code. The resolver and every returned
/// C function must remain valid for the process lifetime and support concurrent calls.
pub unsafe fn register_static_library(name: &str, resolver: NativeSymbolResolver) -> Result<()> {
    providers().register(name, resolver)
}

pub(crate) fn resolve(library: &str, symbol: &str) -> Option<Result<*mut c_void>> {
    providers().resolve(library, symbol)
}

#[cfg(test)]
#[path = "linked_tests.rs"]
mod tests;
