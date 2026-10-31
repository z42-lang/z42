//! Thread primitives: the calling thread's native stack bounds, a cheap thread
//! identity, and running a closure on a thread with a chosen stack size.
//!
//! Stack bounds are used by `stack_guard` to turn deep z42 recursion into a
//! reported fatal error instead of a guard-page crash. `None` where the
//! platform gives no answer; the guard is then off for that thread.

/// Whether this target can start threads (wasm32 runs single-threaded).
pub const CAN_SPAWN: bool = super::sys::thread::CAN_SPAWN;

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

/// A process-unique identity of the calling thread; signal-safe on Unix.
#[inline]
pub fn current_token() -> usize {
    super::sys::thread::current_token()
}

/// The calling thread's native stack bounds, where available.
pub fn current_thread_stack() -> Option<(usize, usize)> {
    super::sys::thread::current_thread_stack()
}

#[cfg(test)]
#[path = "thread_tests.rs"]
mod tests;
