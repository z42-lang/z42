//! Platform Abstraction Layer (PAL) — single home for every `#[cfg(...)]`
//! split in the runtime.
//!
//! review.md Part 1 P2 Phase 1 (2026-06-03, add-pal-system-phase1). See
//! `docs/internals/src/runtime/pal.md` for the long-form design (CoreCLR
//! comparison, Phase 2-N migration plan, invariants every submodule must
//! respect).
//!
//! # Current submodules
//!
//! - [`system`] — hostname / OS version (Phase 1).
//! - [`fs`] — file-system OS calls: `make_executable` / `symlink` (Phase 2).
//! - `signal` (unix) — fatal-signal registration + async-signal-safe write
//!   primitives + signal-name table (Phase 3). The z42 crash *reporter*
//!   (`signal_handler.rs`) drives these; pal owns only the OS primitives.
//!
//! # Future submodules (independent specs)
//!
//! - `thread` — pthread / Windows-thread primitives backing the future
//!   multi-thread runtime (consumer-gated: lands with the multi-thread runtime).
//! - `mem` — page-aligned alloc / mmap / mprotect for the GC bump allocator
//!   (consumer-gated: lands with the bump allocator).

pub mod system;
pub mod fs;

#[cfg(unix)]
pub mod signal;
