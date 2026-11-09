const HAS_PROCESS_ENV: bool = true;
pub fn pid() -> u32 {
    if HAS_PROCESS_ENV {
        std::process::id()
    } else {
        0
    }
}
pub fn current_exe() -> Option<std::path::PathBuf> {
    if HAS_PROCESS_ENV {
        std::env::current_exe().ok()
    } else {
        None
    }
}
pub fn current_dir() -> std::io::Result<std::path::PathBuf> {
    if HAS_PROCESS_ENV {
        std::env::current_dir()
    } else {
        Ok(std::path::PathBuf::from("/"))
    }
}
pub fn set_current_dir(path: &str) -> std::io::Result<()> {
    if HAS_PROCESS_ENV {
        std::env::set_current_dir(path)
    } else {
        Ok(())
    }
}
pub fn temp_dir() -> std::path::PathBuf {
    if HAS_PROCESS_ENV {
        std::env::temp_dir()
    } else {
        std::path::PathBuf::from("/tmp")
    }
}
pub fn set_env(name: &str, value: Option<&str>) {
    if !HAS_PROCESS_ENV {
        return;
    }
    // SAFETY: environment writes race only with other threads' reads; z42 promises no
    // more than .NET's `SetEnvironmentVariable` here.
    unsafe {
        match value {
            None => std::env::remove_var(name),
            Some(v) => std::env::set_var(name, v),
        }
    }
}
