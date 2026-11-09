//! Dynamic libraries: platform file names, opening a shared library, looking up symbols.
//! The one place the runtime calls `dlopen` / `LoadLibrary` (native extensions, the
//! compression and REPL cdylibs).

/// The platform file name of library `stem`: `lib<stem>.dylib` / `lib<stem>.so` /
/// `<stem>.dll`.
pub fn file_name(stem: &str) -> String {
    format!("{}{}{}", std::env::consts::DLL_PREFIX, stem, std::env::consts::DLL_SUFFIX)
}

/// Resolve a single native library *beside* a given directory — the "colocated
/// native dependency" layout: a component's private native lib ships flat in the
/// same directory as its zpkg (`<zpkg-dir>/lib<name>.<suffix>`), NOT in the shared
/// `<sdk>/native/` (which is eager-scanned by `load_all` for cross-platform stdlib
/// ext libs like compression). This is a *named, directed* stat — it never walks
/// the directory, so a non-stdlib cdylib dropped there is only ever loaded by the
/// caller that asks for it by name, and the generic ext scanner never warns about it.
///
/// `lib_name` is the base name between the platform prefix/suffix (e.g. `z42_repl`
/// → `libz42_repl.dylib` / `libz42_repl.so` / `z42_repl.dll`). Returns the full path
/// only when the file exists.
///
/// (add-path-dependencies D9/D10 — native dependencies colocated beside the
/// consumer's zpkg; supersedes isolate-repl-cdylib.)
pub fn resolve_native_beside(zpkg_dir: &std::path::Path, lib_name: &str) -> Option<std::path::PathBuf> {
    let candidate = zpkg_dir.join(file_name(lib_name));
    candidate.is_file().then_some(candidate)
}

/// An open shared library. Dropping it unloads the library, so keep it alive for as long
/// as any symbol taken from it may be called.
#[cfg(any(feature = "native-interop", feature = "component-loading"))]
pub struct Library(libloading::Library);

#[cfg(any(feature = "native-interop", feature = "component-loading"))]
impl Library {
    /// Open a runtime component and retain its native image for the process.
    /// Rust/shared-std TLS may keep destructor callbacks after the last VM exits.
    /// Per-VM state still drops normally; component images cannot be hot-unloaded.
    ///
    /// # Safety
    /// The same trust and initializer requirements as `open` apply.
    pub unsafe fn open_component(path: &std::path::Path) -> std::io::Result<std::sync::Arc<Self>> {
        use std::{collections::BTreeMap, sync::{Arc, Mutex, OnceLock}};
        static IMAGES: OnceLock<Mutex<BTreeMap<std::path::PathBuf, Arc<Library>>>> = OnceLock::new();
        let path = path.canonicalize()?;
        let images = IMAGES.get_or_init(Default::default);
        if let Some(image) = images.lock().unwrap().get(&path).cloned() {
            return Ok(image);
        }
        // Initializers may load other components; never run them under the lock.
        let image = Arc::new(unsafe { Self::open(&path) }?);
        Ok(images.lock().unwrap().entry(path).or_insert(image).clone())
    }

    /// Open the library at `path`. The error names the path.
    ///
    /// # Safety
    /// Opening runs the library's initialisers; the library must be one this process may
    /// load (built for this target, trusted).
    pub unsafe fn open(path: &std::path::Path) -> std::io::Result<Self> {
        unsafe { libloading::Library::new(path) }
            .map(Library)
            .map_err(|e| std::io::Error::other(format!("dlopen({}): {e}", path.display())))
    }

    /// The address of symbol `name`, as `T` (a function pointer type).
    ///
    /// # Safety
    /// `T` must be the symbol's real type, and the value must not be used after `self`
    /// drops.
    pub unsafe fn symbol<T: Copy>(&self, name: &str) -> std::io::Result<T> {
        unsafe { self.0.get::<T>(name.as_bytes()) }
            .map(|s| *s)
            .map_err(|e| std::io::Error::other(format!("missing symbol `{name}`: {e}")))
    }
}

#[cfg(all(test, z42_test_facade))]
#[path = "dynlib_tests.rs"]
mod tests;
