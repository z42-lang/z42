//! Dynamic libraries: platform file names, opening a shared library, looking up symbols.
//! The one place the runtime calls `dlopen` / `LoadLibrary` (native extensions, the
//! compression and REPL cdylibs).

/// Whether script-requested native extensions can be opened by this build.
/// Trusted runtime components use their separate component-loading contract.
pub const fn is_supported() -> bool {
    cfg!(feature = "component-loading") && super::system::OS_CAPS.native_extensions
}

/// Reject unsupported extension loads before probing files or running native initializers.
pub fn require_supported() -> std::io::Result<()> {
    check_supported(is_supported())
}

fn check_supported(supported: bool) -> std::io::Result<()> {
    if supported { return Ok(()); }
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported,
        "native extension loading is not supported by this platform or build"))
}

/// The platform file name of library `stem`: `lib<stem>.dylib` / `lib<stem>.so` /
/// `<stem>.dll`.
pub fn file_name(stem: &str) -> String {
    format!("{}{}{}", std::env::consts::DLL_PREFIX, stem, std::env::consts::DLL_SUFFIX)
}

/// Candidate names for colocated libraries and platform framework/loader layouts.
pub(crate) fn candidate_paths(stem: &str, directories: Vec<std::path::PathBuf>) -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    for directory in directories {
        paths.push(directory.join(file_name(stem)));
        #[cfg(target_os = "ios")]
        paths.push(directory.join(format!("{stem}.framework")).join(stem));
    }
    #[cfg(target_os = "android")]
    paths.push(file_name(stem).into());
    paths
}

/// Android may load unextracted APK libraries through its linker namespace.
pub(crate) fn is_loader_name(path: &std::path::Path) -> bool {
    cfg!(target_os = "android") && path.components().count() == 1
        && path.file_name() == Some(path.as_os_str())
        && path.extension().is_some_and(|extension| extension == "so")
}

/// Locate the loaded image containing an address without acquiring another handle.
#[cfg(unix)]
pub(crate) fn image_path(address: *const core::ffi::c_void) -> Option<std::path::PathBuf> {
    use std::{ffi::{CStr, OsStr}, os::unix::ffi::OsStrExt};
    let mut info = std::mem::MaybeUninit::<libc::Dl_info>::zeroed();
    // dladdr queries the loader's mapping; it does not dereference the address.
    if unsafe { libc::dladdr(address, info.as_mut_ptr()) } == 0 { return None; }
    let info = unsafe { info.assume_init() };
    if info.dli_fname.is_null() { return None; }
    // The loader owns this NUL-terminated name for the lifetime of the image.
    Some(std::path::PathBuf::from(OsStr::from_bytes(unsafe { CStr::from_ptr(info.dli_fname) }.to_bytes())))
}

#[cfg(windows)]
pub(crate) fn image_path(address: *const core::ffi::c_void) -> Option<std::path::PathBuf> {
    use std::{ffi::{c_void, OsString}, os::windows::ffi::OsStringExt};
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleExW(flags: u32, address: *const u16, module: *mut *mut c_void) -> i32;
        fn GetModuleFileNameW(module: *mut c_void, name: *mut u16, capacity: u32) -> u32;
    }
    let mut module = std::ptr::null_mut();
    // FROM_ADDRESS | UNCHANGED_REFCOUNT: inspect the image without owning it.
    if unsafe { GetModuleHandleExW(0x00000004 | 0x00000002, address.cast(), &mut module) } == 0 { return None; }
    let mut capacity = 512;
    while capacity <= 32768 {
        let mut name = vec![0u16; capacity];
        let length = unsafe { GetModuleFileNameW(module, name.as_mut_ptr(), capacity as u32) } as usize;
        if length == 0 { return None; }
        if length < capacity { return Some(OsString::from_wide(&name[..length]).into()); }
        capacity *= 2;
    }
    None
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn image_path(_: *const core::ffi::c_void) -> Option<std::path::PathBuf> { None }

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
fn component_images() -> &'static parking_lot::Mutex<std::collections::BTreeMap<std::path::PathBuf, std::sync::Arc<Library>>> {
    static IMAGES: std::sync::OnceLock<parking_lot::Mutex<std::collections::BTreeMap<std::path::PathBuf, std::sync::Arc<Library>>>> = std::sync::OnceLock::new();
    IMAGES.get_or_init(Default::default)
}

/// Inspect retained images without invoking the operating-system loader.
#[cfg(any(feature = "native-interop", feature = "component-loading"))]
pub(crate) fn component_is_loaded(path: &std::path::Path) -> bool {
    let key = if is_loader_name(path) { path.to_path_buf() }
        else if let Ok(path) = path.canonicalize() { path }
        else { return false; };
    component_images().lock().contains_key(&key)
}

#[cfg(any(feature = "native-interop", feature = "component-loading"))]
impl Library {
    /// Open a runtime component and retain its native image for the process.
    /// Rust/shared-std TLS may keep destructor callbacks after the last VM exits.
    /// Per-VM state still drops normally; component images cannot be hot-unloaded.
    ///
    /// # Safety
    /// The same trust and initializer requirements as `open` apply.
    pub unsafe fn open_component(path: &std::path::Path) -> std::io::Result<std::sync::Arc<Self>> {
        use std::sync::Arc;
        let path = if is_loader_name(path) { path.to_path_buf() } else { path.canonicalize()? };
        let images = component_images();
        if let Some(image) = images.lock().get(&path).cloned() {
            return Ok(image);
        }
        // Initializers may load other components; never run them under the lock.
        let image = Arc::new(unsafe { Self::open_image(&path) }?);
        Ok(images.lock().entry(path).or_insert(image).clone())
    }

    /// Open the library at `path`. The error names the path.
    ///
    /// # Safety
    /// Opening runs the library's initialisers; the library must be one this process may
    /// load (built for this target, trusted).
    pub unsafe fn open(path: &std::path::Path) -> std::io::Result<Self> {
        require_supported()?;
        unsafe { Self::open_image(path) }
    }

    unsafe fn open_image(path: &std::path::Path) -> std::io::Result<Self> {
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
