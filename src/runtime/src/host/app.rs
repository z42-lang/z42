//! One-shot application execution through the Rust host service.
use super::{LoadOptions, LoadedProgram};
use crate::zpkg::ExecMode;
use anyhow::Result;
/// A host implementation resolves loading inputs, program arguments and reporting.
/// This service loads and executes without choosing process search defaults.
pub struct RunOpts {
    /// Fully resolved loading inputs supplied by the host implementation.
    pub mode: ExecMode,
    pub vfs: std::sync::Arc<dyn crate::pal::fs::Vfs>,
    pub search_dirs: Vec<std::path::PathBuf>,
    pub resolver: Option<std::sync::Arc<dyn super::ZpkgResolver>>,
    /// Program args forwarded to `GetCommandLineArgs()` (the app's `-- <args>`).
    pub program_args: Vec<String>,
    /// Print counter stats to stderr after the run (z42vm `--stats`).
    pub print_stats: bool,
    /// When `print_stats`, emit a single-line JSON object instead of the text
    /// block (z42vm `--stats=json`; `xtask profile` reads it).
    pub stats_json: bool,
    /// The process exits as soon as `run` returns (z42vm): leave the VM to the
    /// OS instead of freeing it block by block. Embedders and nested runs keep
    /// using the process, so they say `false`.
    pub exit_after_run: bool,
}

pub fn run_app(file: &str, entry: Option<&str>, opts: RunOpts) -> Result<()> {
    let program = LoadedProgram::load_file(
        file,
        LoadOptions {
            mode: opts.mode,
            vfs: opts.vfs,
            search_dirs: opts.search_dirs,
            resolver: opts.resolver,
            ..Default::default()
        },
    )?;
    program.set_program_args(opts.program_args)?;
    program.configure_exit_report(opts.print_stats, opts.stats_json);
    let result = program.run_entry(entry);
    program.report_on_exit();
    if opts.exit_after_run {
        std::mem::forget(program);
    }
    result
}
