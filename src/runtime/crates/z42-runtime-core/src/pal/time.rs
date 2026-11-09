//! Monotonic time.
//!
//! `wasm32-unknown-unknown` has no clock (`std::time::Instant::now` panics
//! "time not implemented on this platform"), so callers that must run there
//! check [`HAS_CLOCK`] and [`monotonic_us`] degrades to a counter.

/// Whether `std::time::Instant::now` works on this target.
pub const HAS_CLOCK: bool = super::sys::time::HAS_CLOCK;

/// Microseconds since the first clock read in this process. Without a clock
/// ([`HAS_CLOCK`] is `false`) each call returns the next tick of a counter, so
/// values stay monotonic but are not microseconds.
pub fn monotonic_us() -> u64 {
    super::sys::time::monotonic_us()
}

/// Nanoseconds since the first clock read in this process. Without a clock each call
/// advances by 1 000 (one tick), so durations stay positive but are not nanoseconds.
pub fn monotonic_ns() -> u64 {
    super::sys::time::monotonic_ns()
}

/// Milliseconds since the Unix epoch; `0` without a clock.
pub fn wall_unix_ms() -> i64 {
    super::sys::time::wall_unix_ms()
}

/// Nanoseconds since the Unix epoch; `0` without a clock.
pub fn wall_unix_ns() -> u64 {
    super::sys::time::wall_unix_ns()
}

#[cfg(test)]
#[path = "time_tests.rs"]
mod tests;
