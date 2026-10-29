//! Page-granular memory operations on GC chunk storage.
//!
//! [`decommit`] / [`recommit`] hand a range's physical pages back to the OS while keeping it
//! mapped (reads stay valid and see zeroes or the old bytes; writes fault fresh pages in) —
//! what the GC does to empty pooled chunks it holds beyond its threshold (`gc::footprint`):
//!
//! - macOS / iOS: `MADV_FREE_REUSABLE` (and `MADV_FREE_REUSE` before the range is used again)
//!   — the pair that takes the pages out of the task's footprint right away, which plain
//!   `MADV_FREE` there does not; mimalloc purges the same way.
//! - Linux / Android: `MADV_FREE`, falling back to `MADV_DONTNEED` on kernels without it.
//! - Windows, wasm: not supported ([`CAN_DECOMMIT`] is `false`) — the GC keeps pooled chunks.

use std::sync::atomic::{AtomicUsize, Ordering};

/// Whether [`decommit`] does anything on this target.
pub(crate) const CAN_DECOMMIT: bool =
    cfg!(any(target_os = "macos", target_os = "ios", target_os = "linux", target_os = "android"));

/// The OS page size (cached).
pub(crate) fn page_size() -> usize {
    static PAGE: AtomicUsize = AtomicUsize::new(0);
    let p = PAGE.load(Ordering::Relaxed);
    if p != 0 {
        return p;
    }
    #[cfg(all(unix, not(target_arch = "wasm32")))]
    // SAFETY: `sysconf` has no preconditions.
    let p = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(4096) as usize;
    #[cfg(not(all(unix, not(target_arch = "wasm32"))))]
    let p = 4096usize;
    PAGE.store(p, Ordering::Relaxed);
    p
}

/// The whole pages inside `[start, end)`: `(first page, byte length)`, or `None` when the range
/// holds no complete page.
pub(crate) fn inner_pages(start: usize, end: usize) -> Option<(usize, usize)> {
    let page = page_size();
    let lo = start.next_multiple_of(page);
    let hi = end / page * page;
    (hi > lo).then(|| (lo, hi - lo))
}

/// Give the physical pages of `[addr, addr + len)` back to the OS; the range stays mapped.
/// No-op where [`CAN_DECOMMIT`] is `false`.
///
/// # Safety
/// `addr` / `len` must be page-aligned and inside memory this process owns, and nothing may
/// rely on the range's contents afterwards.
pub(crate) unsafe fn decommit(addr: usize, len: usize) {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    libc::madvise(addr as *mut libc::c_void, len, libc::MADV_FREE_REUSABLE);
    #[cfg(any(target_os = "linux", target_os = "android"))]
    if libc::madvise(addr as *mut libc::c_void, len, libc::MADV_FREE) != 0 {
        libc::madvise(addr as *mut libc::c_void, len, libc::MADV_DONTNEED);
    }
    let _ = (addr, len);
}

/// Announce that a [`decommit`]ted range is about to be written again (macOS / iOS need this
/// to account the pages back; elsewhere the first write is enough).
///
/// # Safety
/// As [`decommit`].
pub(crate) unsafe fn recommit(addr: usize, len: usize) {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    libc::madvise(addr as *mut libc::c_void, len, libc::MADV_FREE_REUSE);
    let _ = (addr, len);
}

#[cfg(test)]
mod tests {
    #[test]
    fn page_size_is_a_power_of_two() {
        let p = super::page_size();
        assert!(p >= 4096 && p.is_power_of_two());
    }

    #[test]
    fn inner_pages_keeps_only_whole_pages() {
        let p = super::page_size();
        assert_eq!(super::inner_pages(1, 3 * p - 1), Some((p, p)));
        assert_eq!(super::inner_pages(p, 3 * p), Some((p, 2 * p)));
        assert_eq!(super::inner_pages(1, 2 * p - 1), None);
    }
}
