//! The calling thread's native stack bounds.
//!
//! Used by `stack_guard` to turn deep z42 recursion into a reported fatal
//! error instead of a guard-page crash. `None` where the platform gives no
//! answer; the guard is then off for that thread.

/// `(low, high)` addresses of the calling thread's stack. The stack grows
/// down from `high` towards `low` on every supported target.
pub fn current_thread_stack() -> Option<(usize, usize)> {
    imp()
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn imp() -> Option<(usize, usize)> {
    // SAFETY: plain queries on the calling thread's own handle.
    unsafe {
        let me = libc::pthread_self();
        let high = libc::pthread_get_stackaddr_np(me) as usize;
        let size = libc::pthread_get_stacksize_np(me);
        (high > size && size > 0).then(|| (high - size, high))
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn imp() -> Option<(usize, usize)> {
    // SAFETY: `attr` is initialised by `pthread_getattr_np` before it is read
    // and destroyed exactly once.
    unsafe {
        let mut attr: libc::pthread_attr_t = std::mem::zeroed();
        if libc::pthread_getattr_np(libc::pthread_self(), &mut attr) != 0 {
            return None;
        }
        let mut addr: *mut libc::c_void = std::ptr::null_mut();
        let mut size: libc::size_t = 0;
        let rc = libc::pthread_attr_getstack(&attr, &mut addr, &mut size);
        libc::pthread_attr_destroy(&mut attr);
        (rc == 0 && size > 0).then(|| (addr as usize, addr as usize + size))
    }
}

#[cfg(windows)]
fn imp() -> Option<(usize, usize)> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThreadStackLimits(low: *mut usize, high: *mut usize);
    }
    let (mut low, mut high) = (0usize, 0usize);
    // SAFETY: writes two out-params for the calling thread (Windows 8+).
    unsafe { GetCurrentThreadStackLimits(&mut low, &mut high) };
    (high > low).then_some((low, high))
}

#[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "linux",
              target_os = "android", windows)))]
fn imp() -> Option<(usize, usize)> {
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn bounds_contain_a_local() {
        let local = 0u8;
        let here = std::hint::black_box(&local) as *const u8 as usize;
        if let Some((low, high)) = super::current_thread_stack() {
            assert!(low < here && here < high, "{low:#x} < {here:#x} < {high:#x}");
        }
    }
}
