//! `RuntimeCounters` — atomic counters for VM runtime observability.
//!
//! Roslyn / CoreCLR parallel: EventCounters / `dotnet-counters monitor`.
//! docs/review.md Part 4 D6 (2026-05-26) — last remaining Part 4 ops/devex
//! item; was P0 because production environments had zero visibility into
//! runtime activity (JIT compiles / builtin calls / exception traffic).
//!
//! # Phase 1 scope (this commit)
//!
//! Lands the infrastructure: struct + atomic increments + CLI `--print-
//! stats-on-exit` flag + one demo increment site (`builtin_calls`,
//! incremented from `corelib::exec_builtin`). The framework is wired but
//! most fields stay at 0 until Phase 2 migrates their respective
//! increment sites:
//!
//! - `jit_methods_compiled` / `jit_compile_us_total` ← `jit::compile_module`
//! - `native_calls` ← `interp::exec_native`
//! - `exceptions_thrown` / `exceptions_caught` ← `exception::*`
//!
//! Each Phase 2 migration is a tiny standalone refactor (1 file + add
//! `ctx.core().counters.<field>.fetch_add(1, Ordering::Relaxed)` in the
//! hot path).
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
    /// Phase 2: increment in `interp::exec_native`.
    pub native_calls:         AtomicU64,

    /// Methods JIT-compiled. Phase 2: increment in `jit::compile_module`.
    pub jit_methods_compiled: AtomicU64,

    /// Total wallclock JIT compile time, microseconds. Phase 2: ditto.
    pub jit_compile_us_total: AtomicU64,

    /// runtime-jit-tiering Phase 1.5 (mixed-mode): calls where an INTERP frame
    /// routed an already-compiled callee to its native code instead of
    /// interpreting it. Incremented in `interp::exec_call::try_native_static_call`
    /// / `exec_vcall::try_native_method_call`. >0 confirms mixed-mode is active.
    pub jit_native_from_interp: AtomicU64,

    /// User exceptions thrown (z42 `throw expr` statements + VM-raised
    /// arithmetic / type errors that bubble as exceptions).
    /// Phase 2: increment in `exception::*`.
    pub exceptions_thrown:    AtomicU64,

    /// User exceptions caught by `try { ... } catch` blocks.
    /// Phase 2: increment in `exception::*` handler-found path.
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

impl std::fmt::Display for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "--- z42vm runtime counters ---")?;
        writeln!(f, "builtin_calls:        {}", self.builtin_calls)?;
        writeln!(f, "native_calls:         {}", self.native_calls)?;
        writeln!(f, "jit_methods_compiled: {}", self.jit_methods_compiled)?;
        writeln!(f, "jit_compile_us_total: {}", self.jit_compile_us_total)?;
        writeln!(f, "jit_native_from_interp: {}", self.jit_native_from_interp)?;
        writeln!(f, "exceptions_thrown:    {}", self.exceptions_thrown)?;
        writeln!(f, "exceptions_caught:    {}", self.exceptions_caught)?;
        write!(f,   "---")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_all_zero() {
        let c = RuntimeCounters::new();
        let s = c.snapshot();
        assert_eq!(s, Snapshot::default());
    }

    #[test]
    fn fetch_add_observable_via_snapshot() {
        let c = RuntimeCounters::new();
        c.builtin_calls.fetch_add(42, Ordering::Relaxed);
        c.exceptions_thrown.fetch_add(3, Ordering::Relaxed);
        c.exceptions_caught.fetch_add(2, Ordering::Relaxed);

        let s = c.snapshot();
        assert_eq!(s.builtin_calls,     42);
        assert_eq!(s.exceptions_thrown, 3);
        assert_eq!(s.exceptions_caught, 2);
        assert_eq!(s.native_calls,      0);
    }

    #[test]
    fn snapshot_is_copy_independent_of_source() {
        let c = RuntimeCounters::new();
        c.builtin_calls.fetch_add(5, Ordering::Relaxed);
        let s1 = c.snapshot();

        // Subsequent mutations of c don't affect captured snapshot
        c.builtin_calls.fetch_add(10, Ordering::Relaxed);
        let s2 = c.snapshot();

        assert_eq!(s1.builtin_calls, 5);
        assert_eq!(s2.builtin_calls, 15);
    }

    #[test]
    fn display_lists_all_fields() {
        let s = Snapshot {
            builtin_calls:        100,
            native_calls:         50,
            jit_methods_compiled: 10,
            jit_compile_us_total: 12345,
            jit_native_from_interp: 42,
            exceptions_thrown:    5,
            exceptions_caught:    3,
        };
        let out = format!("{s}");
        // Every field name appears, every value appears.
        for needle in [
            "builtin_calls:        100",
            "native_calls:         50",
            "jit_methods_compiled: 10",
            "jit_compile_us_total: 12345",
            "exceptions_thrown:    5",
            "exceptions_caught:    3",
        ] {
            assert!(out.contains(needle), "Snapshot display missing `{needle}`; got:\n{out}");
        }
    }

    #[test]
    fn to_json_is_single_line_with_all_fields() {
        let s = Snapshot {
            builtin_calls:        100,
            native_calls:         50,
            jit_methods_compiled: 10,
            jit_compile_us_total: 12345,
            jit_native_from_interp: 42,
            exceptions_thrown:    5,
            exceptions_caught:    3,
        };
        let j = s.to_json();
        assert!(!j.contains('\n'), "JSON stats must be single-line; got:\n{j}");
        assert!(j.starts_with("{\"z42vm_counters\":1,"), "missing sentinel key; got:\n{j}");
        for needle in [
            "\"builtin_calls\":100",
            "\"native_calls\":50",
            "\"jit_methods_compiled\":10",
            "\"jit_compile_us_total\":12345",
            "\"jit_native_from_interp\":42",
            "\"exceptions_thrown\":5",
            "\"exceptions_caught\":3",
        ] {
            assert!(j.contains(needle), "JSON stats missing `{needle}`; got:\n{j}");
        }
    }

    #[test]
    fn concurrent_increments_are_lossless() {
        use std::sync::Arc;
        use std::thread;

        let c = Arc::new(RuntimeCounters::new());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let c = Arc::clone(&c);
            handles.push(thread::spawn(move || {
                for _ in 0..1000 {
                    c.builtin_calls.fetch_add(1, Ordering::Relaxed);
                }
            }));
        }
        for h in handles { h.join().unwrap(); }

        assert_eq!(c.snapshot().builtin_calls, 8000);
    }
}
