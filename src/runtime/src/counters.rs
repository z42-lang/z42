//! `RuntimeCounters` — atomic counters for VM runtime observability.
//!
//! Roslyn / CoreCLR parallel: EventCounters / `dotnet-counters monitor`.
//! docs/review.md Part 4 D6 (2026-05-26) — last remaining Part 4 ops/devex
//! item; was P0 because production environments had zero visibility into
//! runtime activity (JIT compiles / builtin calls / exception traffic).
//!
//! # Increment sites (current)
//!
//! - `builtin_calls`         ← `corelib::exec_builtin`
//! - `native_calls`          ← `interp::exec_native::call_native` (2026-05-26)
//! - `jit_methods_compiled`  ← `jit::frame` tier-up compile (2026-05-26)
//! - `jit_native_from_interp`← `interp::exec_call` / `exec_vcall` native routing
//! - `exceptions_thrown`     ← `interp::fire_exception_thrown` (2026-05-26)
//! - `exceptions_caught`     ← `interp::fire_exception_caught` (2026-05-26)
//!
//! Still at 0 (not yet wired): `jit_compile_us_total` — per-compile wall time
//! is tracked as a diagnostics.md §4.2 span item, deferred.
//!
//! Heap-derived numbers (`allocations`, GC minor/major/reclaimed) live in
//! `gc::HeapStats`, not here; the profile snapshot ([`ProfileSnapshot`])
//! merges both at the `--print-stats-on-exit` output point.
//!
//! # Concurrency
//!
//! All counters are `AtomicU64` with `Ordering::Relaxed`. Counters never
//! drive control flow — they're observation-only — so weak ordering is
//! fine. Single `RuntimeCounters` instance per `VmCore`, shared across
//! all threads on that core via the `Arc<VmCore>` they all hold.
//!
//! # Snapshot semantics
//!
//! [`Snapshot`] is a frozen view (non-atomic u64 values) captured at one
//! instant. Because each counter is loaded independently, the values are
//! NOT a consistent point-in-time tuple — `jit_methods_compiled` may be
//! one cycle ahead of `jit_compile_us_total` etc. For observation use,
//! the skew is irrelevant; for billing / SLA reporting it would matter
//! (deferred).

use std::sync::atomic::{AtomicU64, Ordering};

/// Atomic counters incremented by hot-path code. One instance per VmCore.
#[derive(Debug, Default)]
pub struct RuntimeCounters {
    /// Builtin functions invoked (e.g. `__str_length`, `__print`, ...).
    /// Incremented at top of `corelib::exec_builtin`.
    pub builtin_calls:        AtomicU64,

    /// Native FFI calls dispatched (e.g. user `[Native("...")]` extern methods).
    /// Incremented at top of `interp::exec_native::call_native` (2026-05-26).
    pub native_calls:         AtomicU64,

    /// Methods JIT-compiled. Incremented on tier-up compile in `jit::frame`
    /// (2026-05-26).
    pub jit_methods_compiled: AtomicU64,

    /// Total wallclock JIT compile time, microseconds. Not yet wired (0);
    /// per-compile timing tracked as a diagnostics.md §4.2 span item (deferred).
    pub jit_compile_us_total: AtomicU64,

    /// runtime-jit-tiering Phase 1.5 (mixed-mode): calls where an INTERP frame
    /// routed an already-compiled callee to its native code instead of
    /// interpreting it. Incremented in `interp::exec_call::try_native_static_call`
    /// / `exec_vcall::try_native_method_call`. >0 confirms mixed-mode is active.
    pub jit_native_from_interp: AtomicU64,

    /// User exceptions thrown (z42 `throw expr` statements + VM-raised
    /// arithmetic / type errors that bubble as exceptions).
    /// Incremented in `interp::fire_exception_thrown` (2026-05-26).
    pub exceptions_thrown:    AtomicU64,

    /// User exceptions caught by `try { ... } catch` blocks.
    /// Incremented in `interp::fire_exception_caught` (2026-05-26).
    pub exceptions_caught:    AtomicU64,
}

impl RuntimeCounters {
    pub fn new() -> Self {
        Self::default()
    }

    /// Capture a frozen snapshot of all counters. See module doc for note
    /// about per-counter skew (not a consistent tuple).
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            builtin_calls:        self.builtin_calls.load(Ordering::Relaxed),
            native_calls:         self.native_calls.load(Ordering::Relaxed),
            jit_methods_compiled: self.jit_methods_compiled.load(Ordering::Relaxed),
            jit_compile_us_total: self.jit_compile_us_total.load(Ordering::Relaxed),
            jit_native_from_interp: self.jit_native_from_interp.load(Ordering::Relaxed),
            exceptions_thrown:    self.exceptions_thrown.load(Ordering::Relaxed),
            exceptions_caught:    self.exceptions_caught.load(Ordering::Relaxed),
        }
    }
}

/// Frozen view of all counters at one instant. Returned by
/// [`RuntimeCounters::snapshot`]. Implements `Display` for `--print-
/// stats-on-exit` output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Snapshot {
    pub builtin_calls:        u64,
    pub native_calls:         u64,
    pub jit_methods_compiled: u64,
    pub jit_compile_us_total: u64,
    pub jit_native_from_interp: u64,
    pub exceptions_thrown:    u64,
    pub exceptions_caught:    u64,
}

impl Snapshot {
    /// Render this snapshot as a single-line JSON object for machine consumption
    /// (`--print-stats-on-exit --stats-format=json`; `xtask profile` scrapes it).
    /// Hand-rolled (no serde) — a flat map of the same u64 fields `Display` lists;
    /// the `z42vm_counters` sentinel key lets scrapers disambiguate it from any
    /// `{`-leading program stderr on the same stream.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"z42vm_counters\":1,\
\"builtin_calls\":{},\
\"native_calls\":{},\
\"jit_methods_compiled\":{},\
\"jit_compile_us_total\":{},\
\"jit_native_from_interp\":{},\
\"exceptions_thrown\":{},\
\"exceptions_caught\":{}}}",
            self.builtin_calls,
            self.native_calls,
            self.jit_methods_compiled,
            self.jit_compile_us_total,
            self.jit_native_from_interp,
            self.exceptions_thrown,
            self.exceptions_caught,
        )
    }
}

impl Snapshot {
    /// Writes the counter field lines (no header / trailer). Shared by
    /// [`Snapshot`]'s and [`ProfileSnapshot`]'s `Display` so the counter
    /// block never drifts between the two.
    fn fmt_fields(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "builtin_calls:        {}", self.builtin_calls)?;
        writeln!(f, "native_calls:         {}", self.native_calls)?;
        writeln!(f, "jit_methods_compiled: {}", self.jit_methods_compiled)?;
        writeln!(f, "jit_compile_us_total: {}", self.jit_compile_us_total)?;
        writeln!(f, "jit_native_from_interp: {}", self.jit_native_from_interp)?;
        writeln!(f, "exceptions_thrown:    {}", self.exceptions_thrown)?;
        writeln!(f, "exceptions_caught:    {}", self.exceptions_caught)
    }
}

impl std::fmt::Display for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "--- z42vm runtime counters ---")?;
        self.fmt_fields(f)?;
        write!(f, "---")
    }
}

/// Combined runtime + heap-derived counter view for the profile snapshot
/// (`--print-stats-on-exit`). Wraps the counter [`Snapshot`] and adds the
/// heap-owned numbers (`gc::HeapStats`) that live outside `RuntimeCounters`,
/// so a single JSON line / text block carries the full picture for
/// `xtask profile`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProfileSnapshot {
    pub counters:          Snapshot,
    /// Total object allocations (from `HeapStats.allocations`).
    pub allocations:       u64,
    /// Generational minor (young-gen) collections (from `HeapStats`).
    pub minor_collections: u64,
    /// Major (whole-heap) collections (from `HeapStats`).
    pub major_collections: u64,
    /// Cumulative reclaimed bytes (from `HeapStats.reclaimed_bytes`).
    pub reclaimed_bytes:   u64,
}

impl ProfileSnapshot {
    /// Assemble from a counter snapshot + the heap-derived fields. Kept as
    /// plain `u64` so `counters.rs` stays free of a `gc` dependency — the
    /// caller (`app.rs`) extracts them from `HeapStats`.
    pub fn new(
        counters: Snapshot, allocations: u64,
        minor_collections: u64, major_collections: u64, reclaimed_bytes: u64,
    ) -> Self {
        Self { counters, allocations, minor_collections, major_collections, reclaimed_bytes }
    }

    /// Single-line JSON: a strict **superset** of [`Snapshot::to_json`] (same
    /// `z42vm_counters` sentinel + all counter keys) plus `allocations` /
    /// `minor_collections` / `major_collections` / `reclaimed_bytes`. Built by
    /// splicing the counter JSON so the counter keys never drift; only adds
    /// keys → back-compatible for `xtask profile`'s scraper.
    pub fn to_json(&self) -> String {
        let base = self.counters.to_json();
        let head = base.strip_suffix('}').unwrap_or(base.as_str());
        format!(
            "{head},\"allocations\":{},\"minor_collections\":{},\
\"major_collections\":{},\"reclaimed_bytes\":{}}}",
            self.allocations, self.minor_collections,
            self.major_collections, self.reclaimed_bytes,
        )
    }
}

impl std::fmt::Display for ProfileSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "--- z42vm runtime counters ---")?;
        self.counters.fmt_fields(f)?;
        writeln!(f, "allocations:          {}", self.allocations)?;
        writeln!(f, "gc_minor_collections: {}", self.minor_collections)?;
        writeln!(f, "gc_major_collections: {}", self.major_collections)?;
        writeln!(f, "gc_reclaimed_bytes:   {}", self.reclaimed_bytes)?;
        write!(f, "---")
    }
}

#[cfg(test)]
#[path = "counters_tests.rs"]
mod counters_tests;
