pub(crate) const CAN_DECOMMIT: bool = cfg!(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "linux",
    target_os = "android"
));
pub(crate) fn page_size() -> usize {
    // SAFETY: `sysconf` has no preconditions.
    unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(4096) as usize
}
pub(crate) unsafe fn decommit(addr: usize, len: usize) {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    libc::madvise(addr as *mut libc::c_void, len, libc::MADV_FREE_REUSABLE);
    #[cfg(any(target_os = "linux", target_os = "android"))]
    if libc::madvise(addr as *mut libc::c_void, len, libc::MADV_FREE) != 0 {
        libc::madvise(addr as *mut libc::c_void, len, libc::MADV_DONTNEED);
    }
    let _ = (addr, len);
}
pub(crate) unsafe fn recommit(addr: usize, len: usize) {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    libc::madvise(addr as *mut libc::c_void, len, libc::MADV_FREE_REUSE);
    let _ = (addr, len);
}
