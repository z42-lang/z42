//! Facade adapters for application services consumed by core-library builtins.
use std::{path::PathBuf, sync::Arc};
use crate::{corelib::application::Services, pal::fs::Vfs};

pub(crate) fn services() -> Services {
    Services { run_isolated, report_on_exit: crate::host::report_on_exit }
}

fn run_isolated(path: &str, entry: Option<&str>, libs: Option<PathBuf>, vfs: &Arc<dyn Vfs>) -> anyhow::Result<()> {
    crate::host::run_app(path, entry, crate::host::RunOpts {
        mode: crate::zpkg::ExecMode::Interp,
        libs_dir: libs,
        program_args: Vec::new(),
        print_stats: false,
        stats_json: false,
        exit_after_run: false,
        vfs: Some(Arc::clone(vfs)),
    })
}
