//! Thread primitives: the calling thread's native stack bounds, a cheap thread
//! identity, and running a closure on a thread with a chosen stack size.
//!
//! Stack bounds are used by `stack_guard` to turn deep z42 recursion into a
//! reported fatal error instead of a guard-page crash. `None` where the
//! platform gives no answer; the guard is then off for that thread.

/// Whether this target can start threads (wasm32 runs single-threaded).
pub const CAN_SPAWN: bool = !cfg!(target_arch = "wasm32");

/// Why [`run_with_stack`] could not produce `f`'s result.
#[derive(Debug)]
pub enum RunError {
    /// The OS refused to start the thread.
    Spawn(std::io::Error),
    /// `f` panicked (the panic message already went to the panic hook).
    Panicked,
}

/// Run `f` on a fresh thread named `name` with a `stack_bytes` stack and wait
/// for it. Where threads do not exist ([`CAN_SPAWN`] is `false`) `f` runs on
/// the calling thread.
pub fn run_with_stack<R: Send + 'static>(
    name: &str, stack_bytes: usize, f: impl FnOnce() -> R + Send + 'static,
) -> Result<R, RunError> {
    if !CAN_SPAWN {
        return Ok(f());
    }
    std::thread::Builder::new()
        .name(name.to_string())
        .stack_size(stack_bytes)
        .spawn(f)
        .map_err(RunError::Spawn)?
        .join()
        .map_err(|_| RunError::Panicked)
}

/// A process-unique id of the calling thread. On unix it is `pthread_self()`
/// — a register read, safe in a signal handler (unlike a `thread_local!`,
/// whose first access may allocate).
#[cfg(unix)]
#[inline]
pub fn current_token() -> usize {
    // SAFETY: `pthread_self` has no preconditions.
    unsafe { libc::pthread_self() as usize }
}

#[cfg(not(unix))]
#[inline]
pub fn current_token() -> usize {
    thread_local! { static TOKEN: u8 = const { 0 }; }
    TOKEN.with(|t| t as *const u8 as usize)
}

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

    #[test]
    fn run_with_stack_returns_the_result() {
        let r = super::run_with_stack("pal-test", 1 << 20, || 6 * 7);
        assert_eq!(r.unwrap(), 42);
    }

    #[test]
    fn run_with_stack_reports_a_panic() {
        let r = super::run_with_stack("pal-test-panic", 1 << 20, || -> i32 { panic!("expected") });
        assert!(matches!(r, Err(super::RunError::Panicked)));
    }

    #[test]
    fn token_is_stable_per_thread_and_differs_across_threads() {
        let here = super::current_token();
        assert_eq!(here, super::current_token());
        let other = std::thread::spawn(super::current_token).join().unwrap();
        assert_ne!(here, other);
    }
}
