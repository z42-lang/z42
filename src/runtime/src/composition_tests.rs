use super::*;

#[test]
fn repl_override_is_first_and_other_libraries_use_the_vm_policy() {
    const CHILD: &str = "Z42_REPL_PATH_POLICY_TEST";
    if std::env::var_os(CHILD).is_some() {
        let override_path = std::path::PathBuf::from("/z42-test/repl-override");
        let policy = crate::loader::NativeLoading {
            search_dirs: vec!["/z42-test/native".into()],
            allow_static: true, allow_shared: true, allow_runtime: true,
        };
        let repl = native_candidates("z42_repl", &policy);
        assert_eq!(repl[0], override_path.join(crate::pal::dynlib::file_name("z42_repl")));
        assert_eq!(repl[1], override_path);
        let other = native_candidates("z42_other", &policy);
        assert!(other.iter().any(|p| p.starts_with("/z42-test/native")));
        assert!(!other.iter().any(|p| p.starts_with("/z42-test/repl-override")));
        return;
    }
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "composition::tests::repl_override_is_first_and_other_libraries_use_the_vm_policy"])
        .env(CHILD, "1")
        .env("Z42_REPL_NATIVE", "/z42-test/repl-override")
        .output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
}
