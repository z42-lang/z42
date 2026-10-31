//! Dynamic libraries: platform file names, opening a shared library, looking up symbols.
//! The one place the runtime calls `dlopen` / `LoadLibrary` (native extensions, the
//! compression and REPL cdylibs).

/// The platform file name of library `stem`: `lib<stem>.dylib` / `lib<stem>.so` /
/// `<stem>.dll`.
pub fn file_name(stem: &str) -> String {
    format!("{}{}{}", std::env::consts::DLL_PREFIX, stem, std::env::consts::DLL_SUFFIX)
}

/// An open shared library. Dropping it unloads the library, so keep it alive for as long
/// as any symbol taken from it may be called.
#[cfg(feature = "native-interop")]
pub struct Library(libloading::Library);

#[cfg(feature = "native-interop")]
impl Library {
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

#[cfg(test)]
mod tests {
    #[test]
    fn file_name_wraps_the_stem() {
        let f = super::file_name("z42_repl");
        assert!(f.contains("z42_repl"));
        assert!(f.ends_with(std::env::consts::DLL_SUFFIX));
    }

    #[cfg(all(feature = "native-interop", unix))]
    #[test]
    fn open_reports_the_path() {
        let err = unsafe { super::Library::open(std::path::Path::new("/nonexistent/libz42_none.so")) }
            .err()
            .expect("opening a missing file fails");
        assert!(err.to_string().contains("/nonexistent/libz42_none.so"), "{err}");
    }
}
