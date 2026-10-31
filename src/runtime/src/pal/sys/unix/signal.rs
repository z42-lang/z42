pub(crate) fn register_fatal_handlers(handler: extern "C" fn(i32)) -> bool {
    unix::register_fatal_handlers(handler);
    true
}
mod unix {
    pub(super) fn register_fatal_handlers(handler: extern "C" fn(i32)) {
        for &sig in &[
            libc::SIGSEGV,
            libc::SIGABRT,
            libc::SIGFPE,
            libc::SIGILL,
            libc::SIGBUS,
        ] {
            // SAFETY: `handler` only invokes async-signal-safe primitives
            // (`sigsafe::write_*` → `libc::write`, atomic loads, `libc::raise`).
            unsafe {
                if let Err(e) =
                    signal_hook_registry::register_signal_unchecked(sig, move || handler(sig))
                {
                    tracing::warn!(
                        "failed to register signal handler for {}: {e}",
                        std::str::from_utf8(super::signal_name(sig)).unwrap_or("?")
                    );
                    continue;
                }
                run_on_alternate_stack(sig);
            }
        }
    }

    /// Add `SA_ONSTACK` to `sig`'s installed action. `signal-hook-registry` installs
    /// without it, so a SIGSEGV from running into a thread's guard page (native
    /// stack overflow) ran the handler on the exhausted stack and the process died
    /// silently. With it the handler runs on the thread's alternate signal stack,
    /// which the Rust runtime sets up for the main thread and for every
    /// `std::thread`.
    unsafe fn run_on_alternate_stack(sig: i32) {
        unsafe {
            let mut act: libc::sigaction = std::mem::zeroed();
            if libc::sigaction(sig, std::ptr::null(), &mut act) != 0 {
                return;
            }
            if act.sa_flags & libc::SA_ONSTACK == 0 {
                act.sa_flags |= libc::SA_ONSTACK;
                libc::sigaction(sig, &act, std::ptr::null_mut());
            }
        }
    }
}

pub(crate) fn leak_fd(file: std::fs::File) -> Option<i32> {
    #[cfg(unix)]
    {
        use std::os::fd::IntoRawFd;
        Some(file.into_raw_fd())
    }
    #[cfg(not(unix))]
    {
        drop(file);
        None
    }
}
pub(crate) fn reset_default_and_reraise(sig: i32) {
    #[cfg(unix)]
    unsafe {
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
    let _ = sig;
}
pub(crate) fn signal_name(sig: i32) -> &'static [u8] {
    #[cfg(unix)]
    match sig {
        libc::SIGSEGV => return b"SIGSEGV",
        libc::SIGABRT => return b"SIGABRT",
        libc::SIGFPE => return b"SIGFPE",
        libc::SIGILL => return b"SIGILL",
        libc::SIGBUS => return b"SIGBUS",
        _ => {}
    }
    let _ = sig;
    b"UNKNOWN"
}
pub(crate) fn write_str(fd: i32, bytes: &[u8]) {
    #[cfg(not(unix))]
    let _ = (fd, bytes);
    #[cfg(unix)]
    let mut remaining = bytes;
    #[cfg(unix)]
    while !remaining.is_empty() {
        let n = unsafe { libc::write(fd, remaining.as_ptr() as *const _, remaining.len()) };
        if n <= 0 {
            break;
        }
        remaining = &remaining[n as usize..];
    }
}
