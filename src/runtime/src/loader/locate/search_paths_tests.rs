use super::*;

#[test]
fn explicit_paths_keep_entry_extra_libs_order_and_use_the_supplied_vfs() {
    let vfs = crate::pal::vfs::MemoryFs::new();
    vfs.mount("app/entry.zpkg", vec![1]);
    let extra = vec![PathBuf::from("custom"), PathBuf::from("app"), PathBuf::from("custom")];
    assert_eq!(program_search_dirs(&vfs, "app/entry.zpkg", &extra, Some(Path::new("libs"))),
        ["app", "custom", "libs"].map(PathBuf::from));
}

#[test]
fn empty_inputs_do_not_add_process_defaults_or_nonexistent_entry_directories() {
    let vfs = crate::pal::vfs::MemoryFs::new();
    assert!(program_search_dirs(&vfs, "missing/entry.zpkg", &[], None).is_empty());
    assert_eq!(entry_directory("entry.zpkg"), Path::new("."));
    assert_eq!(program_search_dirs(&vfs, "missing/entry.zpkg", &[], Some(Path::new("explicit"))),
        [PathBuf::from("explicit")]);
}
