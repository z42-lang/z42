use std::io::Result;

pub(crate) const HAS_FS: bool = true;

pub(crate) const PATH_LIST_SEP: char = ':';

pub(crate) fn is_name_too_long(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(libc::ENAMETOOLONG)
}

pub(crate) fn sync_before_rename(file: &std::fs::File) -> Result<()> {
    #[cfg(target_vendor = "apple")]
    {
        use std::os::fd::AsRawFd;
        // SAFETY: plain fcntl on a file descriptor `file` owns for the call.
        if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_BARRIERFSYNC) } == 0 {
            return Ok(());
        }
        file.sync_data()
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        file.sync_data()
    }
    #[cfg(not(any(target_vendor = "apple", target_os = "linux", target_os = "android")))]
    {
        file.sync_all()
    }
}

pub(crate) fn make_executable(path: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    let mode = perms.mode() | 0o111;
    perms.set_mode(mode);
    std::fs::set_permissions(path, perms)
}

pub(crate) fn symlink(src: &str, dst: &str) -> Result<()> {
    std::os::unix::fs::symlink(src, dst)
}
