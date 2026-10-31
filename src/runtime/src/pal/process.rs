//! Child-process OS primitives: exit-status decoding, process groups / job objects so a
//! timeout can kill a whole process tree, and `PATH` lookup.

use std::process::{Child, Command, ExitStatus};

/// Decode the child status, including Unix 128+signal exits.
pub fn exit_code(status: ExitStatus) -> i64 { super::sys::process::exit_code(status) }
/// Arrange for the child to own its process group, where supported.
pub fn own_process_group(cmd: &mut Command) { super::sys::process::own_process_group(cmd) }
/// OS handle used to terminate a child and its own process tree on timeout.
pub struct ProcessTree(super::sys::process::ProcessTree);
impl ProcessTree {
    /// `own_group` must match the value used at spawn; never kill the caller's group.
    pub fn attach(child: &Child, own_group: bool) -> Self { Self(super::sys::process::ProcessTree::attach(child, own_group)) }
    pub fn kill(&self, child: &mut Child) { self.0.kill(child) }
}
use super::sys::process::{contains_path_separator, candidates_in_dir, check_candidate};
pub fn find_executable(name: &str) -> Option<String> {
    if name.is_empty() {
        return None;
    }

    // Direct-path branch: contains separator → no PATH walk.
    if contains_path_separator(name) {
        return check_candidate(std::path::Path::new(name));
    }

    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        // POSIX: empty entry is current directory.
        let base = if dir.as_os_str().is_empty() {
            std::path::PathBuf::from(".")
        } else {
            dir
        };
        for cand in candidates_in_dir(&base, name) {
            if let Some(hit) = check_candidate(&cand) {
                return Some(hit);
            }
        }
    }
    None
}
#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
