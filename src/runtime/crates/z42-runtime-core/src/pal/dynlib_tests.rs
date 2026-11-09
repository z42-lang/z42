#[test]
fn file_name_wraps_the_stem() {
    let f = super::file_name("z42_repl");
    assert!(f.contains("z42_repl"));
    assert!(f.ends_with(std::env::consts::DLL_SUFFIX));
}

#[cfg(all(feature = "native-interop", unix))]
#[test]
fn open_reports_the_path() {
    let err = unsafe { super::Library::open(std::path::Path::new("/nonexistent/libz42_none.so")) }
        .err()
        .expect("opening a missing file fails");
    assert!(
        err.to_string().contains("/nonexistent/libz42_none.so"),
        "{err}"
    );
}

#[test]
fn resolve_beside_requires_a_file_and_keeps_the_named_directory() {
    let temp = std::env::temp_dir().join(format!("z42-pal-dynlib-{}", std::process::id()));
    std::fs::create_dir(&temp).unwrap();
    let path = temp.join(super::file_name("editor"));
    assert_eq!(super::resolve_native_beside(&temp, "editor"), None);
    std::fs::create_dir(&path).unwrap();
    assert_eq!(super::resolve_native_beside(&temp, "editor"), None);
    std::fs::remove_dir(&path).unwrap();
    std::fs::write(&path, b"library").unwrap();
    assert_eq!(super::resolve_native_beside(&temp, "editor"), Some(path));
    assert_eq!(super::resolve_native_beside(&temp, "other"), None);
    std::fs::remove_dir_all(temp).unwrap();
}
