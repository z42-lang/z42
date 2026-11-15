#[cfg(any(feature = "native-interop", feature = "component-loading"))]
#[test]
fn a_missing_image_is_not_loaded_by_cache_queries() {
    let path = std::env::temp_dir().join(format!("z42-query-missing-{}", std::process::id()))
        .join(crate::pal::dynlib::file_name("missing"));
    for _ in 0..4 { assert!(!super::is_loaded(&path)); }
    assert!(!path.exists());
}

#[test]
fn actual_code_images_converge_across_threads_and_outlive_callers() {
    use std::sync::{Arc, Barrier};
    let directory = std::env::temp_dir().join(format!("z42-code-images-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(crate::pal::dynlib::file_name("z42_fixture"));
    let source = std::path::Path::new(env!("Z42_RUNTIME_DIR"))
        .join("tests/data/native_module/registration.rs");
    // A wrong consumer ABI remains valid native code: the code loader must not
    // interpret component/native descriptors on behalf of an adapter.
    let output = std::process::Command::new("rustc")
        .args(["--edition=2021", "--crate-type=cdylib", "-Awarnings", "--cfg", "broken"])
        .arg(source).arg("-o").arg(&path).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(!super::is_loaded(&path), "file presence is not an opened image");
    let barrier = Arc::new(Barrier::new(8));
    let threads: Vec<_> = (0..8).map(|_| {
        let path = path.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            unsafe { super::open(&path, &Default::default()) }.unwrap()
        })
    }).collect();
    let images: Vec<_> = threads.into_iter().map(|thread| thread.join().unwrap()).collect();
    assert!(images.iter().all(|image| Arc::ptr_eq(&images[0], image)));
    let alias = directory.join(".").join(path.file_name().unwrap());
    assert!(Arc::ptr_eq(&images[0], &unsafe { super::open(&alias, &Default::default()) }.unwrap()));
    assert!(super::is_loaded(&alias));
    let denied = super::super::NativeLoading { allow_runtime: false, ..Default::default() };
    assert_eq!(unsafe { super::open(&alias, &denied) }.err().unwrap().kind(), std::io::ErrorKind::PermissionDenied);
    assert!(Arc::ptr_eq(&images[0], &unsafe { super::open(&alias, &Default::default()) }.unwrap()));
    let weak = Arc::downgrade(&images[0]);
    assert!(unsafe { images[0].symbol::<extern "C" fn()>("missing_symbol") }.is_err());
    drop(images);
    let image = weak.upgrade().expect("code survives caller and thread teardown");
    let drops: extern "C" fn() -> u64 = unsafe { image.symbol("z42_fixture_drop_count") }.unwrap();
    assert_eq!(drops(), 0, "loading code does not create native VM instances");
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn invalid_images_are_not_published_or_cached_as_success() {
    let directory = std::env::temp_dir().join(format!("z42-code-invalid-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(crate::pal::dynlib::file_name("invalid"));
    std::fs::write(&path, b"not native code").unwrap();
    for _ in 0..2 {
        assert!(unsafe { super::open(&path, &Default::default()) }.is_err());
        assert!(!super::is_loaded(&path));
    }
    std::fs::remove_dir_all(directory).unwrap();
}


#[test]
fn denied_load_does_not_probe_paths_or_reuse_cached_permission() {
    let denied = super::super::NativeLoading { allow_runtime: false, ..Default::default() };
    let missing = std::env::temp_dir().join("z42-denied-image-never-created");
    let error = unsafe { super::open(&missing, &denied) }.err().unwrap();
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    assert!(!super::is_loaded(&missing));
}
