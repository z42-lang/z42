use super::*;

#[test]
fn payload_is_resolved_from_the_executable_without_cwd_or_environment_search() {
    let sdk = Path::new("sdk");
    assert_eq!(payload_path(&sdk.join("bin/z42vm.exe")).unwrap(), sdk.join("native/z42vm.exe"));
    assert!(payload_path(Path::new("/")).is_err());
}

#[test]
fn arguments_and_exit_status_reach_the_actual_host() {
    let directory = std::env::temp_dir().join(format!("z42-main-forward-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let executable = directory.join(format!("forwarder{}", std::env::consts::EXE_SUFFIX));
    let output = Command::new("rustc")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/cli_forwarder.rs"))
        .arg("-o").arg(&executable).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let status = forward(&executable, ["space value", "中文", "--value=quoted\"value"].map(OsString::from)).unwrap();
    assert_eq!(status.code(), Some(37));
    assert!(forward(&directory.join("missing"), []).is_err());
    std::fs::remove_dir_all(directory).unwrap();
}
