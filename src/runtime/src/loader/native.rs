//! Native library loader: `dlopen` + invoke the library's `<basename>_register`
//! entry point so its types land in the VM's `TypeRegistry`.
//!
//! The library handle is stored on loader extension state for the VM's
//! lifetime, keeping every `fn_ptr` referenced by a `RegisteredType` valid
//! until VM drop.

use std::ffi::OsStr;
use std::path::Path;

use anyhow::{Context, Result, anyhow};

use crate::context::VmContext;
#[cfg(feature = "native-interop")]
use crate::interop::error;

/// Categorical error code mirrored into the last-error slot for native
/// library load failures.  retire-z-codes: was `Z0910`; the
/// named constant was retired but the numeric value stays for embedder
/// wire-compat with `Z42Error.code`.
#[cfg(feature = "native-interop")]
const LIB_LOAD_FAILURE: u32 = 910;

/// Type signature of the library's register entry point — `void(void)`.
#[cfg(feature = "native-interop")]
type RegisterFn = unsafe extern "C" fn();

/// Open a script-requested library after checking PAL capability.
///
/// # Safety
/// The path must identify trusted code; opening runs library initializers.
#[cfg(feature = "native-interop")]
pub unsafe fn open_library(
    path: &Path,
) -> std::io::Result<std::sync::Arc<crate::pal::dynlib::Library>> {
    crate::pal::dynlib::require_supported()?;
    unsafe { crate::pal::dynlib::Library::open_component(path) }.map_err(|error| {
        std::io::Error::new(error.kind(), format!("dlopen({}): {error}", path.display()))
    })
}

/// Load a Tier 1 type library and register against the explicitly supplied VM.
#[cfg(feature = "native-interop")]
pub fn load_library(ctx: &VmContext, path: &Path) -> Result<()> {
    error::clear();

    let lib = unsafe { open_library(path) }
        .map_err(anyhow::Error::from)
        .map_err(|e| {
            error::set(LIB_LOAD_FAILURE, format!("{e:#}"));
            e
        })?;

    let entry_name = guess_register_symbol(path)
        .ok_or_else(|| {
            let msg = format!(
                "cannot derive register-entry symbol from path {} (file name must end in `<libname>.<ext>` so we can call `<libname>_register`)",
                path.display()
            );
            error::set(LIB_LOAD_FAILURE, &msg);
            anyhow!(msg)
        })?;

    let register: RegisterFn = unsafe { lib.symbol(&entry_name) }
        .with_context(|| {
            format!(
                "missing register symbol `{entry_name}` in {}",
                path.display()
            )
        })
        .map_err(|e| {
            error::set(LIB_LOAD_FAILURE, format!("{e:#}"));
            e
        })?;

    // Call the entry. The library is expected to invoke `z42_register_type`
    // for each of its types; those calls flow through `exports::z42_register_type`
    // → `interop::register_native_type` on the current thread.
    let _guard = crate::interop::exports::VmGuard::enter(ctx);
    unsafe { register() };

    retain_library(ctx, lib)?;
    Ok(())
}

/// Strip lib prefix / extension from `path` and form `<basename>_register`.
///
/// Examples:
/// - `libnumz42_c.dylib`  → `numz42_c_register`
/// - `libnumz42_c.so`     → `numz42_c_register`
/// - `numz42_c.dll`       → `numz42_c_register`
#[cfg(feature = "native-interop")]
pub fn guess_register_symbol(path: &Path) -> Option<String> {
    let stem = path.file_stem().and_then(OsStr::to_str)?;
    let core = stem.strip_prefix("lib").unwrap_or(stem);
    Some(format!("{core}_register"))
}

#[cfg(all(test, feature = "native-interop"))]
#[path = "native_tests.rs"]
mod tests;

/// Released last by the host extension assembly, after provider destructors and type tables.
#[derive(Default)]
#[cfg(any(feature = "native-interop", feature = "component-loading"))]
pub(crate) struct Libraries {
    owned: parking_lot::Mutex<Vec<std::sync::Arc<crate::pal::dynlib::Library>>>,
    named: parking_lot::Mutex<
        std::collections::HashMap<
            String,
            Result<std::sync::Arc<crate::pal::dynlib::Library>, String>,
        >,
    >,
}

#[cfg(feature = "native-interop")]
pub(crate) fn retain_library(
    ctx: &VmContext,
    library: std::sync::Arc<crate::pal::dynlib::Library>,
) -> Result<()> {
    let libraries = ctx
        .ext::<Libraries>()
        .ok_or_else(|| anyhow!("missing native loader VM state"))?;
    libraries.owned.lock().push(library);
    Ok(())
}

/// Candidate policy injected by the host. The logical library name never carries a suffix.
pub(crate) type PathSource = fn(&str) -> Vec<std::path::PathBuf>;
pub(crate) struct SearchPaths(pub(crate) PathSource);

pub(crate) fn path_source(ctx: &VmContext) -> Result<PathSource> {
    ctx.ext::<SearchPaths>()
        .map(|paths| paths.0)
        .ok_or_else(|| anyhow!("native library path service was not registered"))
}

/// Resolve a trusted implementation through host-supplied candidate policy, including
/// signed iOS frameworks. Script path loading uses the separate capability-checked
/// load_native_module/load_library entries. No lock is held while opening an image.
pub(crate) fn resolve_symbol(
    ctx: &VmContext,
    library: &str,
    symbol: &str,
) -> Result<*mut std::ffi::c_void> {
    if let Some(result) = super::linked::resolve(library, symbol) {
        return result;
    }
    #[cfg(any(feature = "native-interop", feature = "component-loading"))]
    {
        let libraries = ctx
            .ext::<Libraries>()
            .ok_or_else(|| anyhow!("missing native loader VM state"))?;
        let cached = libraries.named.lock().get(library).cloned();
        let image = match cached {
            Some(result) => result,
            None => {
                let paths = path_source(ctx)?;
                let result = (|| -> Result<_> {
                    for candidate in paths(library) {
                        if !candidate.is_file() && !crate::pal::dynlib::is_loader_name(&candidate) {
                            continue;
                        }
                        return unsafe { crate::pal::dynlib::Library::open_component(&candidate) }
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
        let _ = (ctx, library, symbol);
        Err(anyhow!("native extension loading is disabled"))
    }
}

#[cfg(any(feature = "native-interop", feature = "component-loading"))]
fn bail_native_missing<T>(library: &str) -> Result<T> {
    Err(anyhow!(
        "native library `{library}` not found in the configured candidates"
    ))
}

/// Load a self-registering module through the same descriptor entry used for static linkage.
#[cfg(feature = "native-interop")]
pub fn load_native_module(ctx: &VmContext, path: &Path) -> Result<()> {
    crate::pal::dynlib::require_supported()?;
    let image = unsafe { crate::pal::dynlib::Library::open_component(path)? };
    let stem = path
        .file_stem()
        .and_then(OsStr::to_str)
        .ok_or_else(|| anyhow!("native module path has no name"))?;
    let entry_name = format!("{}_module_v1", stem.strip_prefix("lib").unwrap_or(stem));
    let entry: z42_abi::native::ModuleEntry = unsafe { image.symbol(&entry_name)? };
    let descriptor = unsafe { entry() };
    let module = unsafe { crate::interop::module::NativeModule::parse(descriptor)? };
    let libraries = ctx
        .ext::<Libraries>()
        .ok_or_else(|| anyhow!("missing native loader VM state"))?;
    let mut named = libraries.named.lock();
    if let Some(Ok(existing)) = named.get(module.name) {
        if !std::sync::Arc::ptr_eq(existing, &image) {
            return Err(anyhow!(
                "native module `{}` already resolved to a different image",
                module.name
            ));
        }
    }
    unsafe { crate::interop::register_native_module(ctx, descriptor)? };
    named.insert(module.name.to_owned(), Ok(image));
    Ok(())
}
