use super::*;

#[test]
fn explicit_core_is_authoritative_even_if_missing() {
    let missing = PathBuf::from("selected-but-missing");
    assert_eq!(runtime_path(Path::new("sdk/bin/z42vm"), Some(missing.clone())).unwrap(), missing);
    assert!(unsafe { open_runtime(Path::new("selected-but-missing")) }.is_err());
}

#[test]
fn core_is_resolved_relative_to_the_executable() {
    let root = std::env::temp_dir().join(format!("z42-main-path-{}", std::process::id()));
    let native = root.join("native");
    std::fs::create_dir_all(&native).unwrap();
    let core = native.join(libloading::library_filename("z42"));
    std::fs::write(&core, []).unwrap();
    assert_eq!(runtime_path(&root.join("bin/z42vm"), None).unwrap(), core);
    std::fs::remove_dir_all(&root).unwrap();
    let exe = Path::new("build/z42vm");
    assert_eq!(runtime_path(exe, None).unwrap(), Path::new("build").join(libloading::library_filename("z42")));
}

#[test]
fn cargo_has_one_vm_bin_and_no_windows_forwarder_feature() {
    let manifest = include_str!("../Cargo.toml");
    let entries: Vec<_> = manifest.split("[[bin]]").skip(1).collect();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].starts_with("\nname = \"z42vm\""));
    assert!(!manifest.contains("windows-launcher"));
}
