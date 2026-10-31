use std::process::{Child, Command, ExitStatus};

pub(crate) fn exit_code(status: ExitStatus) -> i64 {
    match status.code() {
        Some(c) => c as i64,
        None => {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                status.signal().map(|s| 128 + (s as i64)).unwrap_or(-1)
            }
            #[cfg(not(unix))]
            {
                -1
            }
        }
    }
}

pub(crate) fn own_process_group(cmd: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(not(unix))]
    let _ = cmd;
}

pub(crate) struct ProcessTree {
    own_group: bool,
    job: Option<win_job::Job>,
}
impl ProcessTree {
    pub(crate) fn attach(child: &Child, own_group: bool) -> Self {
        Self {
            own_group,
            job: if own_group {
                win_job::Job::for_child(child)
            } else {
                None
            },
        }
    }
    pub(crate) fn kill(&self, child: &mut Child) {
        if let Some(job) = &self.job {
            job.terminate();
        }
        let _ = self.own_group;
        let _ = child.kill();
    }
}

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

pub(crate) fn contains_path_separator(s: &str) -> bool {
    s.contains('/') || s.contains('\\')
}

pub(crate) fn candidates_in_dir(dir: &std::path::Path, name: &str) -> Vec<std::path::PathBuf> {
    // If the queried name already has an extension, accept as-is. Otherwise
    // try each PATHEXT entry (default to common set when env unset).
    let has_ext = std::path::Path::new(name).extension().is_some();
    if has_ext {
        return vec![dir.join(name)];
    }
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
    pathext
        .split(';')
        .filter(|ext| !ext.is_empty())
        .map(|ext| dir.join(format!("{}{}", name, ext)))
        .collect()
}

pub(crate) fn check_candidate(path: &std::path::Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    Some(path.to_string_lossy().into_owned())
}
