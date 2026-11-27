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
    vfs.mount("/virtual/app.zbc", crate::loader::test_support::empty_zbc());
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

#[test]
fn exit_diagnostics_are_scoped_to_the_application_vm() {
    use std::{process::Command, sync::Arc};

    const CHILD: &str = "Z42_EXIT_DIAGNOSTICS_TEST_CHILD";
    let run = |vfs, search_dirs, file: &str, entry: Option<&str>, print_stats| {
        super::run_app(
            file,
            entry,
            super::RunOpts {
                mode: crate::zpkg::ExecMode::Interp,
                vfs,
                resolver: None,
                search_dirs,
                program_args: Vec::new(),
                print_stats,
                stats_json: true,
                exit_after_run: false,
            },
        )
    };
    if let Ok(case) = std::env::var(CHILD) {
        if case == "exit" {
            let ctx = crate::vm::VmContext::new();
            crate::diagnostics::configure_exit_report(&ctx, true, true);
            crate::corelib::io::fs::typed::process_exit(&ctx, 37).unwrap();
            panic!("the managed exit builtin returned");
        }

        let vfs = Arc::new(crate::pal::fs::MemoryFs::new());
        vfs.mount("/empty.zbc", crate::loader::test_support::empty_zbc());
        let live_vms = crate::vm::vm_cores_snapshot().len();
        if case == "nested" {
            let outer = crate::vm::VmContext::new();
            crate::diagnostics::configure_exit_report(&outer, true, true);
            let _ = run(vfs, Vec::new(), "/empty.zbc", Some("Missing.Main"), false);
            crate::composition::report_on_exit(&outer);
            crate::composition::report_on_exit(&outer);
            return;
        }
        #[cfg(z42_have_embedding_hello)]
        if case == "return" {
            let fixture = include_bytes!(concat!(env!("OUT_DIR"), "/embedding_hello.zbc"));
            let path =
                std::env::temp_dir().join(format!("z42-exit-report-{}.zbc", std::process::id()));
            std::fs::write(&path, fixture).unwrap();
            let result = run(
                crate::pal::fs::default_vfs(),
                vec![env!("Z42_EMBEDDING_TEST_LIBS").into()],
                path.to_str().unwrap(),
                Some("Embedding.Hello.Main"),
                true,
            );
            std::fs::remove_file(path).unwrap();
            result.unwrap();
        }
        if case == "error" {
            let _ = run(vfs.clone(), Vec::new(), "/empty.zbc", Some("Missing.Main"), false);
            let error = run(vfs.clone(), Vec::new(), "/empty.zbc", Some("Missing.Main"), true)
                .unwrap_err();
            assert!(error
                .to_string()
                .contains("function `Missing.Main` not found"));
        }
        let _ = run(vfs, Vec::new(), "/empty.zbc", Some("Missing.Main"), true);
        assert_eq!(crate::vm::vm_cores_snapshot().len(), live_vms);
        return;
    }

    let cases = [
        ("error", 0, 2),
        ("exit", 37, 1),
        ("nested", 0, 1),
        #[cfg(z42_have_embedding_hello)]
        ("return", 0, 2),
    ];
    for (case, code, expected_reports) in cases {
        let result = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "host::app_tests::exit_diagnostics_are_scoped_to_the_application_vm",
                "--nocapture",
            ])
            .env(CHILD, case)
            .env_remove("Z42_SAMPLE_HZ")
            .output()
            .unwrap();
        let stderr = String::from_utf8(result.stderr).unwrap();
        assert_eq!(result.status.code(), Some(code), "{case}: {stderr}");
        assert!(
            !String::from_utf8_lossy(&result.stdout).contains("\"allocations\""),
            "{case}"
        );
        let reports: Vec<_> = stderr
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|report| report.get("allocations").is_some())
            .collect();
        assert_eq!(reports.len(), expected_reports, "{case}: {stderr}");
    }
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn c_run_app_applies_the_sidecar_before_selecting_a_vm_stack() {
    use std::{ffi::CString, process::Command, ptr};

    const CHILD: &str = "Z42_EMBEDDED_SIDECAR_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let path = std::env::temp_dir().join(format!("z42-embedded-sidecar-{}.zbc", std::process::id()));
        let sidecar = path.with_extension("runtimeconfig.toml");
        std::fs::write(&path, crate::loader::test_support::empty_zbc()).unwrap();
        std::fs::write(&sidecar, "[runtime]\nthread-stack-bytes = 12582912\n").unwrap();
        let app = CString::new(path.to_str().unwrap()).unwrap();
        let code = unsafe {
            super::z42_host_run_app(
                app.as_ptr(), c"Missing.Main".as_ptr(), ptr::null(), 0, ptr::null(),
            )
        };
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(sidecar).unwrap();
        assert_eq!(code, 1, "the intentionally missing entry must fail");
        assert_eq!(crate::config::runtime_config().vm_thread_stack_size(), 12 * 1024 * 1024);
        return;
    }

    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "host::app_tests::c_run_app_applies_the_sidecar_before_selecting_a_vm_stack",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .env_remove("Z42_CONFIG")
        .env_remove("Z42_APP_CONFIG")
        .env_remove("Z42_THREAD_STACK_BYTES")
        .env_remove("Z42_SAMPLE_HZ")
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
}
