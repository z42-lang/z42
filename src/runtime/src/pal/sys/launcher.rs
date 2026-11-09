//! Native launcher bootstrap, independent of runtime Rust types and initialization.
use crate::entry_api::*;
use anyhow::{bail, ensure, Context, Result};
use std::{ffi::CString, mem::ManuallyDrop, path::PathBuf};
mod identity { include!(concat!(env!("OUT_DIR"), "/identity.rs")); }

fn library_name() -> &'static str {
    if cfg!(windows) { "z42.dll" } else if cfg!(target_os = "macos") { "libz42.dylib" } else { "libz42.so" }
}
fn library_path() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("locating z42vm")?;
    let parent = exe.parent().context("z42vm has no parent directory")?;
    let mut paths = vec![parent.join(library_name()), parent.join("native").join(library_name())];
    if let Some(root) = parent.parent() { paths.push(root.join("native").join(library_name())); }
    if let Some(path) = paths.iter().find(|p| p.is_file()) { return Ok(path.clone()); }
    bail!("runtime library was not found; searched {}", paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", "))
}
unsafe fn open(path: &std::path::Path) -> Result<libloading::Library> {
    #[cfg(windows)]
    { Ok(unsafe { libloading::os::windows::Library::load_with_flags(path, 0x00000100 | 0x00001000) }?.into()) }
    #[cfg(not(windows))]
    { Ok(unsafe { libloading::Library::new(path) }?) }
}

pub fn run() -> Result<i32> {
    let path = library_path()?;
    // Native initializers and runtime TLS destructors can outlive the entry call,
    // including when descriptor validation fails. Retain the code until process exit.
    let library = ManuallyDrop::new(unsafe { open(&path) }.with_context(|| format!("loading runtime `{}`", path.display()))?);
    let get = unsafe { library.get::<GetRuntimeEntry>(b"z42_runtime_entry_v1\0") }.context("runtime entry symbol is missing")?;
    let entry = unsafe { get() };
    let run = unsafe { callback(entry) }?;
    let args: Vec<CString> = std::env::args_os().map(|s| {
        let s = s.into_string().map_err(|_| anyhow::anyhow!("launcher arguments must be valid Unicode"))?;
        Ok(CString::new(s)?)
    }).collect::<Result<_>>()?;
    let argc = i32::try_from(args.len()).context("too many launcher arguments")?;
    let argv: Vec<_> = args.iter().map(|s| s.as_ptr()).collect();
    Ok(unsafe { run(argc, argv.as_ptr()) })
}

unsafe fn callback(entry: *const RuntimeEntry) -> Result<unsafe extern "C" fn(i32, *const *const std::ffi::c_char) -> i32> {
    ensure!(!entry.is_null() && (entry as usize) % std::mem::align_of::<RuntimeEntry>() == 0, "null or misaligned runtime entry");
    let header = unsafe { entry.cast::<EntryHeader>().read() };
    ensure!(header.version == ENTRY_VERSION && header.size as usize == std::mem::size_of::<RuntimeEntry>(), "runtime entry ABI version/size mismatch");
    let entry = unsafe { &*entry };
    ensure!(entry.build_id == identity::COMPONENT_BUILD_ID, "runtime build identity mismatch; use z42vm and runtime libraries from the same pack");
    entry.run.context("runtime entry has no run callback")

}

#[cfg(test)]
#[path = "launcher_tests.rs"]
mod tests;
