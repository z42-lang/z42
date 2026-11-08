pub mod metadata;
pub mod object;
pub mod corelib;
pub mod gc;
pub mod profiler;
// review.md Part 1 P2 Phase 1 (add-pal-system-phase1):
// Platform Abstraction Layer — single home for every `#[cfg(target_os)]`
// split. Phase 1 covers `system` (hostname / os_version);
// Phase 2-N add fs / signal / thread / mem.
pub mod pal;
// Public entry for the engine-independent object operations.
pub use vm::exec::ops::object as objops;
pub mod interp;
// add-runtime-feature-flags (P4.1): jit / aot are feature-gated.
// `default = ["jit"]` keeps backward compat; platforms (wasm / ios / android)
// build without the JIT backend by activating their preset features.
#[cfg(feature = "jit")]
pub mod jit;
// Portable pin state is always available; interop gates its native ABI modules.
pub mod interop;
// Embedding API (Tier 1 C ABI for host applications). Spec:
// docs/internals/src/runtime/embedding.md, 变更 add-embedding-api/.
pub mod host;
pub mod vm;
pub mod app;
pub(crate) mod assembly;

// Runtime configuration registry — single source of truth for every
// Z42_* env var the runtime reads (docs/review.md Part 4 D1).
// `RuntimeConfig` carries the 5 startup-consumed knobs; `KNOWN_KNOBS`
// table also lists subsystem-local knobs (Z42_GC_* / Z42_NATIVE_PATH /
// Z42_SAFEPOINT_THROTTLE / Z42_STRESS_ITERS) for `--info` discovery.
pub mod config;

// POSIX signal handler — captures z42 call stack on hard crashes
// (SIGSEGV / SIGABRT / SIGFPE / SIGILL / SIGBUS). Phase 2 of D4 panic-hook
// story (Phase 1 = main.rs install_panic_hook). Windows VEH = Phase 2.1.
// Spec: 变更 add-os-signal-handler/.
pub mod signal_handler;
