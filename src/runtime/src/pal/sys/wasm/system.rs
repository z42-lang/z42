use crate::pal::system::OsCaps;
const HAS_PROCESS_ENV: bool = false;
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
pub fn hostname() -> Option<String> {
    // WASM sandbox has no network identity. Graceful degrade.
    None
}
pub fn os_version() -> String {
    String::from("wasm")
}
pub fn os_name() -> &'static str {
    "wasm"
}
const WASM: bool = true;
const MOBILE: bool = false;
pub const OS_CAPS: OsCaps = OsCaps {
    threads: crate::pal::thread::CAN_SPAWN,
    fs: !WASM,
    clock: crate::pal::time::HAS_CLOCK,
    entropy: !WASM,
    process: !WASM && !MOBILE,
    tty: !WASM && !MOBILE,
    env: !WASM && !MOBILE,
    socket_listen: !WASM && !MOBILE,
    hardlink: !WASM && true,
};
