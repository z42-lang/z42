pub(crate) const CAN_SPAWN: bool = true;
pub(crate) fn current_token() -> usize {
    // SAFETY: `pthread_self` has no preconditions.
    unsafe { libc::pthread_self() as usize }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub(crate) fn current_thread_stack() -> Option<(usize, usize)> {
    // SAFETY: plain queries on the calling thread's own handle.
    unsafe {
        let me = libc::pthread_self();
        let high = libc::pthread_get_stackaddr_np(me) as usize;
        let size = libc::pthread_get_stacksize_np(me);
        (high > size && size > 0).then(|| (high - size, high))
    }
}
#[cfg(any(target_os = "linux", target_os = "android"))]
pub(crate) fn current_thread_stack() -> Option<(usize, usize)> {
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
#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "linux",
    target_os = "android"
)))]
pub(crate) fn current_thread_stack() -> Option<(usize, usize)> {
    None
}
