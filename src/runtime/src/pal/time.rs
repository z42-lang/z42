//! Monotonic time.
//!
//! `wasm32-unknown-unknown` has no clock (`std::time::Instant::now` panics
//! "time not implemented on this platform"), so callers that must run there
//! check [`HAS_CLOCK`] and [`monotonic_us`] degrades to a counter.

/// Whether `std::time::Instant::now` works on this target.
pub const HAS_CLOCK: bool = !cfg!(target_arch = "wasm32");

/// Microseconds since the first clock read in this process. Without a clock
/// ([`HAS_CLOCK`] is `false`) each call returns the next tick of a counter, so
/// values stay monotonic but are not microseconds.
pub fn monotonic_us() -> u64 {
    if HAS_CLOCK { since_epoch().as_micros() as u64 } else { next_tick() }
}

/// Nanoseconds since the first clock read in this process. Without a clock each call
/// advances by 1 000 (one tick), so durations stay positive but are not nanoseconds.
pub fn monotonic_ns() -> u64 {
    if HAS_CLOCK { since_epoch().as_nanos() as u64 } else { next_tick() * 1_000 }
}

/// Milliseconds since the Unix epoch; `0` without a clock.
pub fn wall_unix_ms() -> i64 {
    wall_since_unix_epoch().as_millis() as i64
}

/// Nanoseconds since the Unix epoch; `0` without a clock.
pub fn wall_unix_ns() -> u64 {
    wall_since_unix_epoch().as_nanos() as u64
}

fn since_epoch() -> std::time::Duration {
    use std::sync::OnceLock;
    use std::time::Instant;
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed()
}

fn wall_since_unix_epoch() -> std::time::Duration {
    if !HAS_CLOCK {
        return std::time::Duration::ZERO;
    }
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default()
}

fn next_tick() -> u64 {
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
        let (c, d) = (super::monotonic_ns(), super::monotonic_ns());
        assert!(d >= c);
    }

    #[test]
    fn wall_clock_is_after_2020() {
        assert!(super::wall_unix_ms() > 1_577_836_800_000);
    }
}
