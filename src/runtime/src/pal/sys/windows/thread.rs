pub(crate) const CAN_SPAWN: bool = true;
pub(crate) use super::super::common::thread::current_token;
pub(crate) fn current_thread_stack() -> Option<(usize, usize)> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThreadStackLimits(low: *mut usize, high: *mut usize);
    }
    let (mut low, mut high) = (0usize, 0usize);
    // SAFETY: writes two out-params for the calling thread (Windows 8+).
    unsafe { GetCurrentThreadStackLimits(&mut low, &mut high) };
    (high > low).then_some((low, high))
}
