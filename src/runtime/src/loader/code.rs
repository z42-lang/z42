//! Process-lifetime native code images, independent of any consumer ABI or VM.
use std::{collections::BTreeMap, path::{Path, PathBuf}, sync::{Arc, OnceLock}};

/// A retained image owns code, not component factories or per-VM native state.
pub(crate) struct CodeImage(crate::pal::dynlib::Library);

fn images() -> &'static parking_lot::Mutex<BTreeMap<PathBuf, Arc<CodeImage>>> {
    static IMAGES: OnceLock<parking_lot::Mutex<BTreeMap<PathBuf, Arc<CodeImage>>>> = OnceLock::new();
    IMAGES.get_or_init(Default::default)
}

fn key(path: &Path) -> std::io::Result<PathBuf> {
    if crate::pal::dynlib::is_loader_name(path) { Ok(path.to_path_buf()) }
    else { path.canonicalize() }
}

/// Query retained code without running native initializers or calling OS open.
pub(crate) fn is_loaded(path: &Path) -> bool {
    key(path).is_ok_and(|path| images().lock().contains_key(&path))
}

/// Retain trusted native code through process/TLS teardown. The caller validates
/// deployment policy before opening, and its own ABI before calling symbols.
///
/// # Safety
/// Opening executes native initializers. The library must be trusted and built
/// for this process; cached images do not grant permission to a new caller.
pub(crate) unsafe fn open(path: &Path, policy: &super::NativeLoading) -> std::io::Result<Arc<CodeImage>> {
    policy.check(super::NativeOrigin::RuntimeLoad)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::PermissionDenied, error.to_string()))?;
    let path = key(path)?;
    if let Some(image) = images().lock().get(&path).cloned() { return Ok(image); }
    // Native initializers may reenter the loader. Never hold the cache lock here.
    let image = Arc::new(CodeImage(unsafe { crate::pal::dynlib::Library::open_image(&path) }?));
    // Concurrent opens converge on one retained handle; dropping an extra OS
    // reference cannot unload the image retained in the map.
    let retained = {
        let mut images = images().lock();
        images.entry(path).or_insert_with(|| image.clone()).clone()
    };
    // Release a redundant OS handle outside the lock as well: its teardown is
    // native code and must obey the same reentry rule as initialization.
    drop(image);
    Ok(retained)
}

impl CodeImage {
    /// # Safety
    /// T must match the actual symbol type. The caller validates its own ABI.
    pub(crate) unsafe fn symbol<T: Copy>(&self, name: &str) -> std::io::Result<T> {
        unsafe { self.0.symbol(name) }
    }
}

#[cfg(all(test, z42_test_facade))]
#[path = "code_tests.rs"]
mod tests;
