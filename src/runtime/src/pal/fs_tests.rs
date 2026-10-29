use super::*;

#[cfg(unix)]
#[test]
fn make_executable_sets_exec_bits() {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::temp_dir().join(format!("z42-pal-fs-exec-{}.tmp", std::process::id()));
    std::fs::write(&path, b"x").unwrap();
    // start at 0o644 (no exec bits), then make_executable should add u+x,g+x,o+x.
    let mut p = std::fs::metadata(&path).unwrap().permissions();
    p.set_mode(0o644);
    std::fs::set_permissions(&path, p).unwrap();

    make_executable(path.to_str().unwrap()).unwrap();

    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o111, 0o111, "all three execute bits set");
    let _ = std::fs::remove_file(&path);
}

#[cfg(unix)]
#[test]
fn symlink_creates_resolving_link() {
    let target = std::env::temp_dir().join(format!("z42-pal-fs-tgt-{}.tmp", std::process::id()));
    let link = std::env::temp_dir().join(format!("z42-pal-fs-lnk-{}.tmp", std::process::id()));
    std::fs::write(&target, b"hello").unwrap();
    let _ = std::fs::remove_file(&link);

    // symlink(src, dst) → dst is the new link pointing at src.
    symlink(target.to_str().unwrap(), link.to_str().unwrap()).unwrap();

    assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(&link).unwrap(), "hello", "link resolves to target");
    let _ = std::fs::remove_file(&link);
    let _ = std::fs::remove_file(&target);
}

#[test]
fn path_list_sep_splits_path() {
    let joined = std::env::join_paths(["a", "b"]).unwrap();
    assert_eq!(joined.to_str().unwrap(), format!("a{}b", super::PATH_LIST_SEP));
}
