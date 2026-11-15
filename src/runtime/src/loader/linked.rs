//! Process-lifetime native symbol resolvers supplied by statically linked hosts.
use anyhow::{Result, bail};
use parking_lot::RwLock;
use std::{
    collections::HashMap,
    ffi::{CString, c_char, c_void},
    sync::OnceLock,
};

pub type NativeSymbolResolver = unsafe extern "C" fn(*const c_char) -> *const c_void;

#[derive(Default)]
struct Providers(RwLock<HashMap<String, (NativeSymbolResolver, super::NativeOrigin)>>);
impl Providers {
    fn register(&self, name: &str, resolver: NativeSymbolResolver) -> Result<()> {
        self.register_source(name, resolver, super::NativeOrigin::StaticLink)
    }
    fn register_source(&self, name: &str, resolver: NativeSymbolResolver, origin: super::NativeOrigin) -> Result<()> {
        validate_name(name)?;
        let mut providers = self.0.write();
        if let Some(previous) = providers.get(name) {
            if !std::ptr::fn_addr_eq(previous.0, resolver) || previous.1 != origin {
                bail!("native library `{name}` already has a different static provider");
            }
        } else {
            providers.insert(name.to_owned(), (resolver, origin));
        }
        Ok(())
    }

    fn resolve_with_policy(&self, library: &str, symbol: &str, policy: &super::NativeLoading) -> Option<Result<*mut c_void>> {
        let (resolver, origin) = self.0.read().get(library).copied()?;
        Some((|| {
            policy.check(origin)?;
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
    #[cfg(test)]
    fn resolve(&self, library: &str, symbol: &str) -> Option<Result<*mut c_void>> {
        self.resolve_with_policy(library, symbol, &Default::default())
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

/// Register symbols supplied by a shared-library link dependency.
/// # Safety
/// The resolver and library must remain valid through process exit.
pub unsafe fn register_shared_library(name: &str, resolver: NativeSymbolResolver) -> Result<()> {
    providers().register_source(name, resolver, super::NativeOrigin::SharedLink)
}

pub(crate) fn resolve_with_policy(library: &str, symbol: &str, policy: &super::NativeLoading) -> Option<Result<*mut c_void>> {
    if let Some(result) = providers().resolve_with_policy(library, symbol, policy) { return Some(result); }
    if symbol == format!("{library}_module_v1") {
        if let Some((_, entry)) = crate::LINKED_NATIVE_MODULES.iter().find(|(name, _)| *name == library) {
            return Some(policy.check(super::NativeOrigin::StaticLink).map(|_| *entry as *const () as *mut c_void));
        }
    }
    None
}

#[cfg(test)]
#[path = "linked_tests.rs"]
mod tests;


pub(crate) fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name.bytes().any(|byte| matches!(byte, 0 | b'/' | b'\\')) {
        bail!("invalid logical native library name");
    }
    Ok(())
}
