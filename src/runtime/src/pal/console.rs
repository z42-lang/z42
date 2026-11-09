//! Console presence and interactive terminals are separate host capabilities.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    pub input: bool,
    pub terminal: bool,
}

const fn classify(platform: bool, valid_input: bool, terminal: bool) -> Capabilities {
    let input = platform && valid_input;
    Capabilities {
        input,
        terminal: input && terminal,
    }
}

/// Inspect handles without reading input or opening terminal/history resources.
/// Mobile/browser/fallback hosts provide their own UI and scripting input loop.
pub fn capabilities() -> Capabilities {
    use std::io::IsTerminal;
    let platform = super::system::OS_CAPS.tty;
    if !platform {
        return classify(false, false, false);
    }
    let valid_input = standard_input_valid();
    let terminal = valid_input
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && std::io::stderr().is_terminal();
    classify(platform, valid_input, terminal)
}

#[cfg(all(unix, not(target_arch = "wasm32")))]
fn standard_input_valid() -> bool {
    // F_GETFL inspects access mode without consuming a pipe or blocking for input.
    let flags = unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_GETFL) };
    flags != -1 && flags & libc::O_ACCMODE != libc::O_WRONLY
}

#[cfg(windows)]
fn standard_input_valid() -> bool {
    use core::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetStdHandle(kind: u32) -> *mut c_void;
    }
    let handle = unsafe { GetStdHandle((-10_i32) as u32) };
    !handle.is_null() && handle as isize != -1
}

#[cfg(not(any(all(unix, not(target_arch = "wasm32")), windows)))]
fn standard_input_valid() -> bool {
    false
}

#[cfg(all(test, z42_test_facade))]
#[path = "console_tests.rs"]
mod tests;
