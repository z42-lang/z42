//! Facade adapters for application services consumed by core-library builtins.
use std::{path::PathBuf, sync::Arc};
use crate::{corelib::application::Services, pal::fs::Vfs};

pub(crate) fn services() -> Services {
    Services { run_isolated, report_on_exit: crate::host::report_on_exit }
}

/// Process-host convenience policy for C/Rust and isolated-run adapters.
/// The VM loading service receives only this resolved result.
pub(crate) fn process_load_options(
    file: &str, libs: Option<&std::path::Path>, mode: crate::zpkg::ExecMode, vfs: Arc<dyn Vfs>,
) -> crate::host::LoadOptions {
    use crate::loader::locate::{probing, search_paths};
    let config = crate::config::runtime_config();
    let entry = search_paths::entry_directory(file);
    let extra = probing::expand_probing_paths(entry, &config.probing_paths);
    probing::set_unresolved_sdk_patterns(probing::unresolved_z42_home_patterns(entry, &config.probing_paths));
    let search_dirs = search_paths::program_search_dirs(&*vfs, file, &extra, libs);
    crate::host::LoadOptions { mode, vfs, search_dirs, ..Default::default() }
}

fn run_isolated(path: &str, entry: Option<&str>, libs: Option<PathBuf>, vfs: &Arc<dyn Vfs>) -> anyhow::Result<()> {
    crate::host::run_app(path, entry, crate::host::RunOpts {
        load_options: process_load_options(path, libs.as_deref(), crate::zpkg::ExecMode::Interp, Arc::clone(vfs)),
        program_args: Vec::new(),
        print_stats: false,
        stats_json: false,
        exit_after_run: false,
    })
}
