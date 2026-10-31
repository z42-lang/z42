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
}
impl ProcessTree {
    pub(crate) fn attach(child: &Child, own_group: bool) -> Self {
        let _ = child;
        Self { own_group }
    }
    pub(crate) fn kill(&self, child: &mut Child) {
        // Only an independently spawned group may receive a negative-pid kill.
        // SAFETY: the spawn contract makes child.id() the owned process-group id.
        if self.own_group {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
        }
        let _ = child.kill();
    }
}

pub(crate) fn contains_path_separator(s: &str) -> bool {
    s.contains('/')
}

pub(crate) fn candidates_in_dir(dir: &std::path::Path, name: &str) -> Vec<std::path::PathBuf> {
    vec![dir.join(name)]
}

pub(crate) fn check_candidate(path: &std::path::Path) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    if meta.permissions().mode() & 0o111 == 0 {
        return None;
    }
    Some(path.to_string_lossy().into_owned())
}
