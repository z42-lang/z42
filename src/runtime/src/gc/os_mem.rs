//! Page-granular memory for GC chunk storage, straight from the OS where there is one.
//!
//! **perf-gc-chunk-fit (2026-10-07)**: region chunks used to be one `Box` each, and their sizes
//! (`CHUNK_SIZE` × entry size: 18 432 B for objects, 26 624 B for arrays) fall just past a
//! mimalloc size class — 20 480 / 28 672 — so every chunk carried 2 KB of slack, 8 B per
//! object. Chunks now come out of slabs mapped here (see `region::slab`), whose sizes are whole
//! pages, so nothing is rounded.
//!
//! - unix (not wasm): anonymous `mmap` / `munmap`. Pages are demand-zero: a slab's chunks that
//!   are never carved, or never touched, cost address space but no RSS.
//! - elsewhere (Windows, wasm): the global allocator with page alignment. Same contract, no
//!   demand paging guarantee.
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

use std::ptr::NonNull;
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

/// Alignment (and size granule) slabs are asked for on the allocator fallback. The unix path
/// gets the real page size from `mmap` regardless.
#[cfg(not(all(unix, not(target_arch = "wasm32"))))]
const FALLBACK_PAGE: usize = 4096;

/// Map `bytes` (> 0) of zero-or-garbage, read-write memory. Aborts on failure, like a failed
/// `Box` allocation would.
#[cfg(all(unix, not(target_arch = "wasm32")))]
pub(crate) fn alloc_pages(bytes: usize) -> NonNull<u8> {
    // SAFETY: an anonymous private mapping with no address hint; the result is checked.
    let p = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            bytes,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANON,
            -1,
            0,
        )
    };
    if p == libc::MAP_FAILED {
        oom(bytes);
    }
    NonNull::new(p.cast()).unwrap_or_else(|| oom(bytes))
}

/// Give back a mapping from [`alloc_pages`].
///
/// # Safety
/// `p` / `bytes` must be exactly what one `alloc_pages` call returned / was given, and nothing
/// may reference the memory afterwards.
#[cfg(all(unix, not(target_arch = "wasm32")))]
pub(crate) unsafe fn free_pages(p: NonNull<u8>, bytes: usize) {
    libc::munmap(p.as_ptr().cast(), bytes);
}

#[cfg(not(all(unix, not(target_arch = "wasm32"))))]
pub(crate) fn alloc_pages(bytes: usize) -> NonNull<u8> {
    let layout = std::alloc::Layout::from_size_align(bytes, FALLBACK_PAGE).expect("slab layout");
    // SAFETY: `bytes > 0` (callers allocate whole chunks).
    let p = unsafe { std::alloc::alloc(layout) };
    NonNull::new(p).unwrap_or_else(|| std::alloc::handle_alloc_error(layout))
}

/// # Safety
/// As the unix twin: `p` / `bytes` from one `alloc_pages` call, unreferenced afterwards.
#[cfg(not(all(unix, not(target_arch = "wasm32"))))]
pub(crate) unsafe fn free_pages(p: NonNull<u8>, bytes: usize) {
    let layout = std::alloc::Layout::from_size_align(bytes, FALLBACK_PAGE).expect("slab layout");
    std::alloc::dealloc(p.as_ptr(), layout);
}

#[cfg(all(unix, not(target_arch = "wasm32")))]
fn oom(bytes: usize) -> ! {
    std::alloc::handle_alloc_error(std::alloc::Layout::from_size_align(bytes, 1).expect("layout"))
}
