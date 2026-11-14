//! One process entry: load the deployed Core and run its command-line host.
use std::path::{Path, PathBuf};

fn runtime_path(executable: &Path, configured: Option<PathBuf>) -> std::io::Result<PathBuf> {
    if let Some(path) = configured.filter(|p| !p.as_os_str().is_empty()) { return Ok(path); }
    let directory = executable.parent().ok_or_else(|| std::io::Error::other("z42vm has no parent directory"))?;
    let name = libloading::library_filename("z42");
    let deployed = directory.parent().unwrap_or(directory).join("native").join(&name);
    if deployed.is_file() { return Ok(deployed); }
    Ok(directory.join(name))
}

unsafe fn open_runtime(path: &Path) -> Result<libloading::Library, libloading::Error> {
    #[cfg(unix)]
    { unsafe { libloading::os::unix::Library::open(Some(path), libloading::os::unix::RTLD_NOW | libloading::os::unix::RTLD_GLOBAL) }.map(Into::into) }
    #[cfg(windows)]
    {
        // The Core's DLL dependencies live beside it, outside bin/. The absolute
        // path and DLL_LOAD_DIR resolve them without changing process PATH.
        unsafe { libloading::os::windows::Library::load_with_flags(path, 0x00000100 | 0x00001000) }.map(Into::into)
    }
}

fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let path = std::path::absolute(runtime_path(&std::env::current_exe()?, std::env::var_os("Z42_RUNTIME_LIBRARY").map(PathBuf::from))?)?;
    // Keep the image loaded through the call and process teardown. Runtime
    // callbacks, TLS destructors and registered signal handlers may retain code.
    let library = unsafe { open_runtime(&path) }.map_err(|e| format!("cannot load runtime {}: {e}", path.display()))?;
    let library = std::mem::ManuallyDrop::new(library);
    let entry = unsafe { library.get::<unsafe extern "C" fn() -> i32>(b"z42_main_v1\0") }
        .map_err(|error| format!("runtime {} has no z42_main_v1 entry: {error}", path.display()))?;
    Ok(unsafe { entry() })
}

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(error) => { eprintln!("z42vm: {error}"); 1 }
    };
    std::process::exit(code);
}

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
