use super::*;

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("z42-pal-localfs-{tag}-{}-{:?}", std::process::id(), std::thread::current().id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn s(p: &std::path::Path) -> String {
    p.to_str().unwrap().to_string()
}

#[test]
fn round_trips_through_the_os_file_system() {
    let dir = temp_dir("rt");
    let f = s(&dir.join("a.txt"));
    let fs = LocalFs;
    fs.write(&f, b"hello").unwrap();
    fs.append(&f, b" world").unwrap();
    assert_eq!(fs.read_to_string(&f).unwrap(), "hello world");
    assert_eq!(std::fs::read(&f).unwrap(), b"hello world", "visible to std::fs directly");
    assert_eq!(fs.file_len(&f).unwrap(), 11);
    assert!(fs.modified_ms(&f).unwrap() > 0);
    assert!(fs.exists(&f) && !fs.is_dir(&f));

    let g = s(&dir.join("b.txt"));
    fs.copy(&f, &g).unwrap();
    fs.rename(&g, &s(&dir.join("c.txt"))).unwrap();
    let mut names = fs.read_dir(&s(&dir)).unwrap();
    names.sort();
    assert_eq!(names, vec!["a.txt", "c.txt"]);
    assert_eq!(fs.glob(&s(&dir), "*.txt").unwrap(), vec![s(&dir.join("a.txt")), s(&dir.join("c.txt"))]);

    fs.remove_file(&f).unwrap();
    assert!(!fs.exists(&f));
    fs.remove_dir(&s(&dir), true).unwrap();
    assert!(!fs.is_dir(&s(&dir)));
}

#[test]
fn write_atomic_replaces_and_leaves_no_tmp() {
    let dir = temp_dir("atomic");
    let f = s(&dir.join("t.bin"));
    let fs = LocalFs;
    fs.write_atomic(&f, b"one").unwrap();
    fs.write_atomic(&f, b"two").unwrap();
    assert_eq!(fs.read(&f).unwrap(), b"two");
    assert_eq!(fs.read_dir(&s(&dir)).unwrap(), vec!["t.bin"], "no tmp sibling left behind");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn directory_length_is_not_found_and_missing_dir_globs_empty() {
    let dir = temp_dir("dirlen");
    let fs = LocalFs;
    assert_eq!(fs.file_len(&s(&dir)).unwrap_err().kind(), std::io::ErrorKind::NotFound);
    assert!(fs.glob(&s(&dir.join("missing")), "*").unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reports_local_paths() {
    assert_eq!(LocalFs.local_path("/x/y"), Some(std::path::Path::new("/x/y")));
}
