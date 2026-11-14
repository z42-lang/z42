//! Windows SDK entry: start the Rust host beside its DLL closure.
use std::{ffi::OsString, path::{Path, PathBuf}, process::{Command, ExitStatus}};

fn payload_path(executable: &Path) -> std::io::Result<PathBuf> {
    let root = executable.parent().and_then(Path::parent)
        .ok_or_else(|| std::io::Error::other("z42vm has no SDK parent directory"))?;
    Ok(root.join("native").join("z42vm.exe"))
}

fn forward(payload: &Path, args: impl IntoIterator<Item = OsString>) -> std::io::Result<ExitStatus> {
    Command::new(payload).args(args).status()
}

fn main() -> std::io::Result<()> {
    let payload = payload_path(&std::env::current_exe()?)?;
    let status = forward(&payload, std::env::args_os().skip(1))?;
    std::process::exit(status.code().unwrap_or(1))
}

#[cfg(test)]
#[path = "main_windows_launcher_tests.rs"]
mod tests;
