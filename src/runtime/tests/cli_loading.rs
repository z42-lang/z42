//! Exercise the real process entry without linking a Core into the executable.
#![cfg(feature = "component-loading")]
use std::{path::Path, process::Command};

#[test]
fn dynamic_entry_reads_original_arguments_and_returns_status() {
    let root = std::env::temp_dir().join(format!("z42-main-load-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let image = root.join(libloading::library_filename("cli_fixture"));
    let compiled = Command::new("rustc").arg("--crate-type=cdylib")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/cli_forwarder.rs"))
        .arg("-o").arg(&image).output().unwrap();
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let binary = env!("CARGO_BIN_EXE_z42vm");
    let status = Command::new(binary).env("Z42_RUNTIME_LIBRARY", &image)
        .args(["space value", "中文", "--value=quoted\"value"]).status().unwrap();
    assert_eq!(status.code(), Some(37));
    let missing = Command::new(binary).env("Z42_RUNTIME_LIBRARY", root.join("missing")).output().unwrap();
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("cannot load runtime"));
    let incompatible = root.join(libloading::library_filename("without_entry"));
    let compiled = Command::new("rustc").arg("--crate-type=cdylib").args(["--cfg", "missing_entry"])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/cli_forwarder.rs"))
        .arg("-o").arg(&incompatible).output().unwrap();
    assert!(compiled.status.success());
    let rejected = Command::new(binary).env("Z42_RUNTIME_LIBRARY", incompatible).output().unwrap();
    assert_eq!(rejected.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("z42_main_v1"));
    std::fs::remove_dir_all(root).unwrap();
}
