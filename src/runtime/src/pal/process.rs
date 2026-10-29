//! Child-process OS primitives: exit-status decoding, process groups / job objects so a
//! timeout can kill a whole process tree, and `PATH` lookup.

use std::process::{Child, Command, ExitStatus};

/// The exit code of `status`. Unix: killed by signal → `code()` is `None`; encode as
/// 128 + signal (the sh convention). Otherwise `-1` when there is no code.
pub fn exit_code(status: ExitStatus) -> i64 {
    match status.code() {
        Some(c) => c as i64,
        None => {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                status.signal().map(|s| 128 + (s as i64)).unwrap_or(-1)
            }
            #[cfg(not(unix))]
            { -1 }
        }
    }
}

/// Start the child in its own process group (pgid == child pid) so [`ProcessTree::kill`]
/// can reach its grandchildren. Unix only; elsewhere [`ProcessTree::attach`] uses a job
/// object instead.
pub fn own_process_group(cmd: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(not(unix))]
    let _ = cmd;
}

/// What a run-timeout kills besides the direct child. Unix: the child's process group,
/// when it was started with [`own_process_group`] — `kill(-pid)` reaches the tree. Windows:
/// a job object the child was assigned to right after spawn (only for `own_group`, like
/// the unix group), so `TerminateJobObject` ends the grandchildren too. A grandchild
/// forked in the few microseconds before the assignment escapes the job — `sh` / `cmd`
/// start far slower.
pub struct ProcessTree {
    own_group: bool,
    #[cfg(windows)]
    job: Option<win_job::Job>,
}

impl ProcessTree {
    /// `own_group` must be the flag that decided [`own_process_group`] at spawn: only a
    /// child in its OWN group may be killed through `kill(-pid)` — otherwise a negative-pid
    /// kill would target the *caller's* group.
    pub fn attach(child: &Child, own_group: bool) -> Self {
        #[cfg(windows)]
        {
            Self { own_group, job: if own_group { win_job::Job::for_child(child) } else { None } }
        }
        #[cfg(not(windows))]
        {
            let _ = child;
            Self { own_group }
        }
    }

    /// Kill `child` and, where [`attach`](Self::attach) set it up, every process in its tree
    /// (e.g. the `sleep` a `sh -c "sleep 5"` forked).
    pub fn kill(&self, child: &mut Child) {
        #[cfg(unix)]
        if self.own_group {
            // SAFETY: kill(2) on a negative pid targets the process group; a stale/no-such
            // group just returns ESRCH, which we ignore (the child.kill() below covers it).
            let pid = child.id() as i32;
            unsafe { libc::kill(-pid, libc::SIGKILL); }
        }
        #[cfg(windows)]
        if let Some(job) = &self.job {
            job.terminate();
        }
        #[cfg(not(unix))]
        let _ = self.own_group;
        let _ = child.kill();
    }
}

#[cfg(windows)]
mod win_job {
    use std::os::windows::io::AsRawHandle;

    type Handle = *mut core::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *mut core::ffi::c_void, name: *const u16) -> Handle;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        fn TerminateJobObject(job: Handle, exit_code: u32) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
    }

    /// An unnamed job object owning one child's process tree; closed on drop.
    pub(super) struct Job(Handle);

    impl Job {
        /// A fresh job with `child` assigned to it, or `None` if either call fails (the
        /// timeout then falls back to killing the direct child only).
        pub(super) fn for_child(child: &std::process::Child) -> Option<Job> {
            // SAFETY: no security attributes, no name — creates a private job object.
            let handle = unsafe { CreateJobObjectW(std::ptr::null_mut(), std::ptr::null()) };
            if handle.is_null() {
                return None;
            }
            let job = Job(handle);
            // SAFETY: both handles are valid for the duration of the call; `child` owns its.
            let ok = unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle() as Handle) };
            (ok != 0).then_some(job)
        }

        pub(super) fn terminate(&self) {
            // SAFETY: `self.0` is a live job handle owned by this value.
            unsafe { TerminateJobObject(self.0, 1) };
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: closed exactly once; processes in the job keep running (no
            // KILL_ON_JOB_CLOSE limit) — only a timeout terminates them.
            unsafe { CloseHandle(self.0) };
        }
    }
}

// Locate an executable on `$PATH`, matching the semantics of POSIX `command -v` and
// Windows `where`:
//   - Names containing a path separator (Unix `/`, Windows `/` or `\`) are
//     treated as direct paths: stat the file and return if it exists and
//     is executable, otherwise null. PATH lookup is skipped — matches
//     POSIX `command -v ./foo` behaviour.
//   - Otherwise iterate `$PATH` entries (`;`-separated on Windows, `:`
//     elsewhere). Empty entries are treated as the current directory
//     per POSIX. First hit wins.
//   - On Unix, "executable" means `metadata.permissions().mode() & 0o111
//     != 0` (any of u/g/o exec bit set), matching `access(X_OK)`'s
//     practical behaviour for normal files.
//   - On Windows, when the queried name lacks an extension, also try
//     each `PATHEXT` suffix (`.COM;.EXE;.BAT;.CMD;...`); when it already
//     has an extension, match it literally.

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

#[cfg(windows)]
fn contains_path_separator(s: &str) -> bool {
    s.contains('/') || s.contains('\\')
}
#[cfg(not(windows))]
fn contains_path_separator(s: &str) -> bool {
    s.contains('/')
}

#[cfg(windows)]
fn candidates_in_dir(dir: &std::path::Path, name: &str) -> Vec<std::path::PathBuf> {
    // If the queried name already has an extension, accept as-is. Otherwise
    // try each PATHEXT entry (default to common set when env unset).
    let has_ext = std::path::Path::new(name).extension().is_some();
    if has_ext {
        return vec![dir.join(name)];
    }
    let pathext = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
    pathext
        .split(';')
        .filter(|ext| !ext.is_empty())
        .map(|ext| dir.join(format!("{}{}", name, ext)))
        .collect()
}
#[cfg(not(windows))]
fn candidates_in_dir(dir: &std::path::Path, name: &str) -> Vec<std::path::PathBuf> {
    vec![dir.join(name)]
}

#[cfg(unix)]
fn check_candidate(path: &std::path::Path) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() { return None; }
    if meta.permissions().mode() & 0o111 == 0 { return None; }
    Some(path.to_string_lossy().into_owned())
}
#[cfg(not(unix))]
fn check_candidate(path: &std::path::Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() { return None; }
    Some(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn find_executable_walks_path_and_takes_direct_paths() {
        let sh = super::find_executable("sh").expect("sh on PATH");
        assert!(sh.ends_with("/sh"));
        assert_eq!(super::find_executable(&sh).as_deref(), Some(sh.as_str()));
        assert_eq!(super::find_executable("./definitely-not-here-z42"), None);
        assert_eq!(super::find_executable(""), None);
    }

    #[cfg(unix)]
    #[test]
    fn exit_code_encodes_signals_as_128_plus() {
        let mut child = std::process::Command::new("sh").args(["-c", "sleep 5"]).spawn().unwrap();
        let tree = super::ProcessTree::attach(&child, false);
        tree.kill(&mut child);
        assert_eq!(super::exit_code(child.wait().unwrap()), 128 + 9);
        let ok = std::process::Command::new("sh").args(["-c", "exit 3"]).status().unwrap();
        assert_eq!(super::exit_code(ok), 3);
    }
}
