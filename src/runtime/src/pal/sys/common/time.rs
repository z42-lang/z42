pub(crate) const HAS_CLOCK: bool = true;
pub(crate) fn monotonic_us() -> u64 {
    if HAS_CLOCK {
        since_epoch().as_micros() as u64
    } else {
        next_tick()
    }
}
pub(crate) fn monotonic_ns() -> u64 {
    if HAS_CLOCK {
        since_epoch().as_nanos() as u64
    } else {
        next_tick() * 1_000
    }
}
pub(crate) fn wall_unix_ms() -> i64 {
    wall_since_unix_epoch().as_millis() as i64
}
pub(crate) fn wall_unix_ns() -> u64 {
    wall_since_unix_epoch().as_nanos() as u64
}
pub(crate) fn since_epoch() -> std::time::Duration {
    use std::sync::OnceLock;
    use std::time::Instant;
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed()
}
pub(crate) fn wall_since_unix_epoch() -> std::time::Duration {
    if !HAS_CLOCK {
        return std::time::Duration::ZERO;
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
}
pub(crate) fn next_tick() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static TICK: AtomicU64 = AtomicU64::new(0);
    TICK.fetch_add(1, Ordering::Relaxed)
}
