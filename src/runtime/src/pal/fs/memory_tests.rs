use super::*;

#[test]
fn mount_read_and_write_ops() {
    let fs = MemoryFs::new();
    fs.mount("/libs/a.zpkg", vec![1, 2, 3]);
    assert_eq!(fs.read("/libs/a.zpkg").unwrap(), vec![1, 2, 3]);
    assert_eq!(fs.file_len("/libs/a.zpkg").unwrap(), 3);
    assert_eq!(fs.read("/libs/none").unwrap_err().kind(), ErrorKind::NotFound);

    fs.write("/out/r.txt", b"hi").unwrap();
    fs.append("/out/r.txt", b"!").unwrap();
    assert_eq!(fs.read_to_string("/out/r.txt").unwrap(), "hi!");
    fs.write("/bad", &[0xff]).unwrap();
    assert_eq!(fs.read_to_string("/bad").unwrap_err().kind(), ErrorKind::InvalidData);

    fs.copy("/out/r.txt", "/out/s.txt").unwrap();
    fs.rename("/out/s.txt", "/out/t.txt").unwrap();
    assert!(!fs.exists("/out/s.txt") && fs.exists("/out/t.txt"));
    fs.write_atomic("/out/t.txt", b"new").unwrap();
    assert_eq!(fs.read("/out/t.txt").unwrap(), b"new");
    fs.remove_file("/out/t.txt").unwrap();
    assert!(!fs.exists("/out/t.txt"));
    assert_eq!(fs.modified_ms("/out/r.txt").unwrap(), 0, "no mtime in memory");
}

#[test]
fn directories_are_derived_from_file_prefixes() {
    let fs = MemoryFs::new();
    fs.mount("/libs/a.zpkg", Vec::new());
    fs.mount("/libs/b.zpkg", Vec::new());
    fs.mount("/libs/sub/c.zbc", Vec::new());
    fs.mount("/libs/sub/d.zbc", Vec::new());
    fs.mount("/libs-backup.zpkg", Vec::new());
    assert!(fs.is_dir("/libs") && fs.is_dir("/libs/") && fs.is_dir("/libs/sub"));
    assert!(!fs.is_dir("/libs/a.zpkg") && !fs.is_dir("/nope"));
    fs.create_dir_all("/empty").unwrap();
    assert!(!fs.is_dir("/empty"), "a directory with no file under it does not exist");

    let mut names = fs.read_dir("/libs").unwrap();
    names.sort();
    assert_eq!(names, vec!["a.zpkg", "b.zpkg", "sub"], "direct children only");
    assert_eq!(fs.glob("/libs/", "*.zpkg").unwrap(), vec!["/libs/a.zpkg", "/libs/b.zpkg"]);
    assert_eq!(fs.glob("/libs", "*sub").unwrap(), vec!["/libs/sub"]);

    let error = fs.remove_dir("/libs/sub", false).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::DirectoryNotEmpty);
    assert!(fs.exists("/libs/sub/c.zbc"));

    fs.remove_dir("/libs/sub", true).unwrap();
    assert!(!fs.exists("/libs/sub/c.zbc") && fs.exists("/libs/a.zpkg"));
    fs.remove_dir("/libs", true).unwrap();
    assert!(fs.exists("/libs-backup.zpkg"));
}

#[test]
fn relative_and_absolute_roots_are_distinct() {
    let fs = MemoryFs::new();
    fs.mount("/abs/a", Vec::new());
    fs.mount("rel/a", Vec::new());
    assert_eq!(fs.read_dir("/").unwrap(), vec!["abs"]);
    assert_eq!(fs.read_dir("").unwrap(), vec!["rel"]);
}

#[cfg(windows)]
#[test]
fn windows_paths_and_mounts_share_keys() {
    let fs = MemoryFs::new();
    fs.mount("/app/main.zpkg", vec![1]);
    fs.append(r"\app\main.zpkg", &[2]).unwrap();
    fs.rename(r"\app\main.zpkg", "/app/main.zpkg").unwrap();
    assert_eq!(fs.read("/app/main.zpkg").unwrap(), vec![1, 2]);
    assert_eq!(fs.file_len(r"\app\main.zpkg").unwrap(), 2);
    assert!(fs.exists(r"\app\main.zpkg"));
    assert!(fs.is_dir(r"\app\"));
    assert_eq!(fs.glob(r"\app\", "*.zpkg").unwrap(), vec!["/app/main.zpkg"]);
    fs.remove_dir(r"\app\", true).unwrap();
    assert!(!fs.exists("/app/main.zpkg"));
}

#[test]
fn instances_do_not_share_files() {
    let a = MemoryFs::new();
    let b = MemoryFs::new();
    a.mount("/x", b"a".to_vec());
    assert!(a.exists("/x"));
    assert!(!b.exists("/x"));
    assert!(a.local_path("/x").is_none(), "a virtual file system has no local paths");
}
