pub(crate) use super::super::common::system::{
    current_dir, current_exe, pid, set_current_dir, set_env, temp_dir,
};
use crate::pal::system::OsCaps;
pub(crate) fn hostname() -> Option<String> {
    use std::ffi::CStr;
    let mut buf = vec![0u8; 256];
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) };
    if rc != 0 {
        return None;
    }
    // gethostname produces a NUL-terminated C string; find the first NUL.
    let cstr = unsafe { CStr::from_ptr(buf.as_ptr() as *const libc::c_char) };
    Some(cstr.to_string_lossy().into_owned())
}
pub(crate) fn os_version() -> String {
    let mut utsname: libc::utsname = unsafe { std::mem::zeroed() };
    if unsafe { libc::uname(&mut utsname) } != 0 {
        return String::new();
    }
    fn cstr(arr: &[libc::c_char]) -> String {
        let p = arr.as_ptr();
        let cstr = unsafe { std::ffi::CStr::from_ptr(p) };
        cstr.to_string_lossy().into_owned()
    }
    format!(
        "{} {} {}",
        cstr(&utsname.sysname),
        cstr(&utsname.release),
        cstr(&utsname.version),
    )
}
pub(crate) fn os_name() -> &'static str {
    std::env::consts::OS
}
const WASM: bool = false;
const MOBILE: bool = cfg!(any(target_os = "ios", target_os = "android"));
pub(crate) const OS_CAPS: OsCaps = OsCaps {
    threads: crate::pal::thread::CAN_SPAWN,
    fs: !WASM,
    clock: crate::pal::time::HAS_CLOCK,
    entropy: !WASM,
    process: !WASM && !MOBILE,
    tty: !WASM && !MOBILE,
    env: !WASM && !MOBILE,
    socket_listen: !WASM && !MOBILE,
    hardlink: !WASM && !cfg!(target_os = "android"),
};
