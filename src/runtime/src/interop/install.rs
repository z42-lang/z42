//! Native library installation transactions: open a library through loader
//! services, then validate and register it here. loader/native keeps the pure
//! code-source side (open / path policy / per-VM image cache / symbol
//! resolution); this module owns the cross-domain flow, so the dependency runs
//! interop → loader only.

use anyhow::{Context, Result, anyhow};
use std::path::Path;

use crate::vm::VmContext;

/// Categorical error code mirrored into the last-error slot for native
/// library load failures.  retire-z-codes: was `Z0910`; the
/// named constant was retired but the numeric value stays for embedder
/// wire-compat with `Z42Error.code`.
#[cfg(feature = "native-interop")]
const LIB_LOAD_FAILURE: u32 = 910;

/// Type signature of the library's register entry point — `void(void)`.
#[cfg(feature = "native-interop")]
type RegisterFn = unsafe extern "C" fn();

/// Load a Tier 1 type library and register against the explicitly supplied VM.
#[cfg(feature = "native-interop")]
pub fn load_library(ctx: &VmContext, path: &Path) -> Result<()> {
    use super::error;
    error::clear();

    ctx.core().loader().native_loading().check(crate::loader::NativeOrigin::RuntimeLoad)?;
    let lib = unsafe { crate::loader::native::open_library(path, ctx.core().loader().native_loading()) }
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
    let _guard = super::exports::VmGuard::enter(ctx);
    unsafe { register() };

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
    let stem = path.file_stem().and_then(std::ffi::OsStr::to_str)?;
    let core = stem.strip_prefix("lib").unwrap_or(stem);
    Some(format!("{core}_register"))
}

/// Load a self-registering module through the same descriptor entry used for static linkage.
#[cfg(feature = "component-loading")]
pub fn load_native_module(ctx: &VmContext, path: &Path) -> Result<()> {
    ctx.core().loader().native_loading().check(crate::loader::NativeOrigin::RuntimeLoad)?;
    crate::pal::dynlib::require_supported()?;
    let image = unsafe { crate::loader::code::open(path, ctx.core().loader().native_loading())? };
    let stem = path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .ok_or_else(|| anyhow!("native module path has no name"))?;
    let entry_name = format!("{}_module_v1", stem.strip_prefix("lib").unwrap_or(stem));
    let entry: z42_abi::module::ModuleEntry = unsafe { image.symbol(&entry_name)? };
    let descriptor = unsafe { entry() };
    let module = unsafe { super::module::NativeModule::parse(descriptor)? };
    unsafe { super::register_native_module(ctx, descriptor)? };
    crate::loader::native::install_module_image(ctx, &module.name, image)
}

#[cfg(all(test, any(feature = "native-interop", feature = "component-loading")))]
#[path = "install_tests.rs"]
mod tests;
