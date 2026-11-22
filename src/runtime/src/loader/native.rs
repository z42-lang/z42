//! Native library code sources: open, path policy, per-VM image cache and
//! symbol resolution. loader/code retains images through process/TLS teardown.
//! The install transactions that combine these services with descriptor
//! validation/registration live in `interop::install` (interop → loader only).

#[cfg(any(feature = "native-interop", feature = "component-loading"))]
use std::path::Path;

use anyhow::Result;
use anyhow::anyhow;

/// Open a script-requested library after checking PAL capability.
///
/// # Safety
/// The path must identify trusted code; opening runs library initializers.
#[cfg(feature = "native-interop")]
pub(crate) unsafe fn open_library(
    path: &Path, policy: &super::NativeLoading,
) -> std::io::Result<std::sync::Arc<crate::loader::code::CodeImage>> {
    crate::pal::dynlib::require_supported()?;
    unsafe { crate::loader::code::open(path, policy) }.map_err(|error| {
        std::io::Error::new(error.kind(), format!("dlopen({}): {error}", path.display()))
    })
}

#[cfg(all(test, feature = "component-loading"))]
#[path = "native_tests.rs"]
mod tests;

/// Per-VM name resolution cache. Process code lifetime belongs to loader/code.
#[derive(Default)]
#[cfg(any(feature = "native-interop", feature = "component-loading"))]
pub(crate) struct Libraries {
    named: parking_lot::Mutex<
        std::collections::HashMap<
            String,
            Result<std::sync::Arc<crate::loader::code::CodeImage>, String>,
        >,
    >,
}

/// Candidate policy injected by the host. The logical library name never carries a suffix.
#[cfg(any(feature = "native-interop", feature = "component-loading"))]
pub(crate) type PathSource = fn(&str, &super::NativeLoading) -> Vec<std::path::PathBuf>;
#[cfg(any(feature = "native-interop", feature = "component-loading"))]
pub(crate) struct SearchPaths(pub(crate) PathSource);

/// Resolve a trusted implementation through host-supplied candidate policy, including
/// signed iOS frameworks. Script path loading uses the separate capability-checked
/// load_native_module/load_library entries. No lock is held while opening an image.
/// The VM supplies cache and path services lazily so linked lookups only read policy.
pub(crate) fn resolve_symbol<'cache>(
    policy: &super::NativeLoading,
    library: &str,
    symbol: &str,
    #[cfg(any(feature = "native-interop", feature = "component-loading"))]
    libraries: impl FnOnce() -> Option<&'cache Libraries>,
    #[cfg(any(feature = "native-interop", feature = "component-loading"))]
    search_paths: impl FnOnce() -> Option<PathSource>,
) -> Result<*mut std::ffi::c_void> {
    if let Some(result) = super::linked::resolve_with_policy(library, symbol, policy) {
        return result;
    }
    #[cfg(any(feature = "native-interop", feature = "component-loading"))]
    {
        policy.check(super::NativeOrigin::RuntimeLoad)?;
        let libraries = libraries().ok_or_else(|| anyhow!("missing native loader VM state"))?;
        let cached = libraries.named.lock().get(library).cloned();
        let image = match cached {
            Some(result) => result,
            None => {
                let paths = search_paths()
                    .ok_or_else(|| anyhow!("native library path service was not registered"))?;
                let result = (|| -> Result<_> {
                    for candidate in paths(library, policy) {
                        if !candidate.is_file() && !crate::pal::dynlib::is_loader_name(&candidate) {
                            continue;
                        }
                        return unsafe { crate::loader::code::open(&candidate, policy) }
                            .map_err(anyhow::Error::from);
                    }
                    bail_native_missing(library)
                })()
                .map_err(|error| format!("{error:#}"));
                libraries
                    .named
                    .lock()
                    .entry(library.to_owned())
                    .or_insert(result)
                    .clone()
            }
        }
        .map_err(anyhow::Error::msg)?;
        Ok(unsafe { image.symbol::<*mut std::ffi::c_void>(symbol)? })
    }
    #[cfg(not(any(feature = "native-interop", feature = "component-loading")))]
    {
        // This arm is what a statically linked final module (the wasm engine
        // compositions) hits when an optional component archive was not linked
        // and no static provider was registered. Name the missing component —
        // consumers (scripts/src/package/fixtures/wasm_components.mjs) assert on the
        // `<component> component unavailable` shape, and it tells the user what
        // to relink instead of blaming a build flag.
        let component = library.strip_prefix("z42_").unwrap_or(library);
        Err(anyhow!(
            "{component} component unavailable (not linked; native extension \
             loading is disabled in this build)"
        ))
    }
}

#[cfg(any(feature = "native-interop", feature = "component-loading"))]
fn bail_native_missing<T>(library: &str) -> Result<T> {
    Err(anyhow!(
        "native library `{library}` not found in the configured candidates"
    ))
}

/// Record an explicitly installed module image in the per-VM name cache,
/// rejecting a second, different image for the same logical name.
#[cfg(feature = "component-loading")]
pub(crate) fn install_module_image(
    libraries: &Libraries,
    name: &str,
    image: std::sync::Arc<crate::loader::code::CodeImage>,
) -> Result<()> {
    let mut named = libraries.named.lock();
    if let Some(Ok(existing)) = named.get(name) {
        if !std::sync::Arc::ptr_eq(existing, &image) {
            return Err(anyhow!(
                "native module `{name}` already resolved to a different image"
            ));
        }
    }
    named.insert(name.to_owned(), Ok(image));
    Ok(())
}
