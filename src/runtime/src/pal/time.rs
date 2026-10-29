//! Monotonic time.
//!
//! `wasm32-unknown-unknown` has no clock (`std::time::Instant::now` panics
//! "time not implemented on this platform"), so callers that must run there
//! check [`HAS_CLOCK`] and [`monotonic_us`] degrades to a counter.

/// Whether `std::time::Instant::now` works on this target.
pub const HAS_CLOCK: bool = !cfg!(target_arch = "wasm32");

/// Microseconds since the first call in this process. Without a clock
/// ([`HAS_CLOCK`] is `false`) each call returns the next tick of a counter, so
/// values stay monotonic but are not microseconds.
pub fn monotonic_us() -> u64 {
    imp()
}

#[cfg(not(target_arch = "wasm32"))]
fn imp() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_micros() as u64
}

#[cfg(target_arch = "wasm32")]
fn imp() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static TICK: AtomicU64 = AtomicU64::new(0);
    TICK.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    #[test]
    fn monotonic_us_never_goes_back() {
        let a = super::monotonic_us();
        let b = super::monotonic_us();
        assert!(b >= a);
    }
}
