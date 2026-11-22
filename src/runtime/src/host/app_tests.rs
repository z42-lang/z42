#[test]
fn run_app_consumes_explicit_vfs_resolver_and_search_paths_without_entry_defaults() {
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    struct Resolver(AtomicUsize);
    impl super::ZpkgResolver for Resolver {
        fn resolve(&self, _: &str) -> Option<Vec<u8>> {
            self.0.fetch_add(1, Ordering::Relaxed);
            None
        }
    }
    let vfs = Arc::new(crate::pal::fs::MemoryFs::new());
    vfs.mount("/virtual/app.zbc", super::program_tests::empty_zbc());
    // An implicit entry-directory search would select this invalid core.
    vfs.mount("/virtual/z42.core.zpkg", b"invalid implicit prelude".to_vec());
    let resolver = Arc::new(Resolver(AtomicUsize::new(0)));
    let error = super::run_app("/virtual/app.zbc", Some("Missing.Main"), super::RunOpts {
        mode: crate::zpkg::ExecMode::Interp, vfs,
        resolver: Some(resolver.clone()), search_dirs: Vec::new(),
        program_args: Vec::new(), print_stats: false, stats_json: false, exit_after_run: false,
    }).unwrap_err();
    assert!(error.to_string().contains("function `Missing.Main` not found"), "{error:#}");
    assert!(resolver.0.load(Ordering::Relaxed) > 0, "the supplied resolver was not consumed");
}
