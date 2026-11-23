//! Process crash reporting for native and Rust hosts.

/// Install fatal signal handlers. Safe to call more than once.
pub fn install() {
    crate::diagnostics::install_crash_report(|| {
        crate::config::runtime_config().crash_dir.as_deref()
    });
}
