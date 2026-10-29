//! Platform Abstraction Layer (PAL) — single home for every `#[cfg(...)]`
//! split in the runtime.
//!
//! review.md Part 1 P2 Phase 1 (add-pal-system-phase1). See
//! `docs/internals/src/runtime/pal.md` for the long-form design (CoreCLR
//! comparison, Phase 2-N migration plan, invariants every submodule must
//! respect).
//!
//! # Submodules
//!
//! - [`system`] — hostname / OS version / OS name.
//! - [`fs`] — file-system OS calls: `make_executable` / `symlink`, path-list separator.
//! - [`mem`] — page size and decommit / recommit of GC chunk pages.
//! - [`thread`] — stack bounds, thread token, run-on-a-big-stack.
//! - [`time`] — monotonic clock (a tick counter where the target has none).
//! - [`net`] — socket OS bits for `Std.Net`: errno → Winsock code table,
//!   interruptible accept wait, TCP keep-alive knobs.
//! - [`process`] — exit-status decoding, process-tree kill, `PATH` lookup.
//! - [`random`] — OS entropy.
//! - [`dynlib`] — shared-library file names, open, symbol lookup.
//! - [`signal`] — fatal-signal registration + async-signal-safe write
//!   primitives + signal-name table. The z42 crash *reporter*
//!   (`signal_handler.rs`) drives these; pal owns only the OS primitives.

pub mod system;
pub mod fs;
pub mod mem;
pub mod thread;
pub mod time;
pub mod net;
pub mod signal;
pub mod process;
pub mod random;
pub mod dynlib;
