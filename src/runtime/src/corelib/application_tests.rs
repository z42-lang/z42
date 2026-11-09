use super::*;

#[test]
fn application_services_share_with_workers_and_remain_vm_owned() {
    let parent = VmContext::new();
    let other = VmContext::new();
    let worker = VmContext::new_with_core(parent.core_arc());
    assert!(std::ptr::eq(state(&parent).unwrap(), state(&worker).unwrap()));
    assert!(!std::ptr::eq(state(&parent).unwrap(), state(&other).unwrap()));
    let weak = Arc::downgrade(&parent.core_arc());
    drop(parent);
    assert!(state(&worker).is_ok());
    drop(worker);
    assert!(weak.upgrade().is_none(), "application services cannot retain the core");
}

#[test]
fn exit_reports_before_terminating() {
    use std::io::Write;
    const CHILD: &str = "Z42_APPLICATION_SERVICES_EXIT_TEST";
    if std::env::var_os(CHILD).is_some() {
        fn report(_: &VmContext) {
            let mut out = std::io::stdout();
            out.write_all(b"report-before-exit\n").unwrap();
            out.flush().unwrap();
        }
        struct DropMarker;
        impl Drop for DropMarker {
            fn drop(&mut self) {
                std::io::stdout().write_all(b"drop-ran\n").unwrap();
            }
        }
        let _marker = DropMarker;
        let ctx = VmContext::new();
        let services = Services { report_on_exit: report, ..*state(&ctx).unwrap() };
        services.exit(&ctx, 37);
    }
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "corelib::application::tests::exit_reports_before_terminating", "--nocapture"])
        .env(CHILD, "1").output().unwrap();
    assert_eq!(result.status.code(), Some(37), "{}", String::from_utf8_lossy(&result.stderr));
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("report-before-exit"), "{output}");
    assert!(!output.contains("drop-ran"), "process exit must not run destructors");
}

use crate::assembly::context::VmContextFactory as _;
