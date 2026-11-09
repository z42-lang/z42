#[cfg(unix)]
#[test]
fn find_executable_walks_path_and_takes_direct_paths() {
    let sh = super::find_executable("sh").expect("sh on PATH");
    assert!(sh.ends_with("/sh"));
    assert_eq!(super::find_executable(&sh).as_deref(), Some(sh.as_str()));
    assert_eq!(super::find_executable("./definitely-not-here-z42"), None);
    assert_eq!(super::find_executable(""), None);
}

#[cfg(unix)]
#[test]
fn exit_code_encodes_signals_as_128_plus() {
    let mut child = std::process::Command::new("sh")
        .args(["-c", "sleep 5"])
        .spawn()
        .unwrap();
    let tree = super::ProcessTree::attach(&child, false);
    tree.kill(&mut child);
    assert_eq!(super::exit_code(child.wait().unwrap()), 128 + 9);
    let ok = std::process::Command::new("sh")
        .args(["-c", "exit 3"])
        .status()
        .unwrap();
    assert_eq!(super::exit_code(ok), 3);
}
