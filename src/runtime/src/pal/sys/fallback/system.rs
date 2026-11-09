pub use super::super::common::system::{
    current_dir, current_exe, pid, set_current_dir, set_env, temp_dir,
};
use crate::pal::system::OsCaps;
pub fn hostname() -> Option<String> {
    None
}
pub fn os_version() -> String {
    String::new()
}
pub fn os_name() -> &'static str {
    std::env::consts::OS
}
const WASM: bool = false;
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
