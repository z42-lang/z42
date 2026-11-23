//! Facade adapters for application services consumed by core-library builtins.
use std::{path::PathBuf, sync::Arc};
use crate::{corelib::application::Services, pal::fs::Vfs};

pub(crate) fn services() -> Services {
    Services { run_isolated, report_on_exit: crate::composition::report_on_exit }
}

/// Process-host convenience policy for C/Rust and isolated-run adapters.
/// The VM loading service receives only this resolved result.
pub(crate) fn process_search_dirs(
    file: &str, libs: Option<&std::path::Path>, vfs: &dyn Vfs,
) -> Vec<PathBuf> {
    use crate::loader::locate::{probing, search_paths};
    let config = crate::config::runtime_config();
    let entry = search_paths::entry_directory(file);
    let extra = probing::expand_probing_paths(entry, &config.probing_paths);
    probing::set_unresolved_sdk_patterns(probing::unresolved_z42_home_patterns(entry, &config.probing_paths));
    search_paths::program_search_dirs(vfs, file, &extra, libs)
}

fn run_isolated(path: &str, entry: Option<&str>, libs: Option<PathBuf>, vfs: &Arc<dyn Vfs>) -> anyhow::Result<()> {
    crate::host::run_app(path, entry, crate::host::RunOpts {
        mode: crate::zpkg::ExecMode::Interp,
        search_dirs: process_search_dirs(path, libs.as_deref(), &**vfs),
        vfs: Arc::clone(vfs),
        resolver: None,
        program_args: Vec::new(),
        print_stats: false,
        stats_json: false,
        exit_after_run: false,
    })
}
