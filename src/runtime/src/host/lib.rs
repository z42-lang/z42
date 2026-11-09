// Production links independent crates; unit tests compile the same implementation
// sources here to retain cross-layer white-box coverage without dependency cycles.
extern crate self as z42;
pub(crate) use z42_zpkg as zpkg;
#[cfg(not(test))]
pub use z42_runtime::object;
#[cfg(test)]
#[path = "../object/mod.rs"]
pub mod object;
#[path = "../corelib/mod.rs"]
pub mod corelib;
#[cfg(not(test))]
pub use z42_runtime::gc;
#[cfg(test)]
#[path = "../gc/mod.rs"]
pub mod gc;
#[path = "../profiler/mod.rs"]
pub mod profiler;
// review.md Part 1 P2 Phase 1 (add-pal-system-phase1):
// Platform Abstraction Layer — single home for every `#[cfg(target_os)]`
// split. Phase 1 covers `system` (hostname / os_version);
// Phase 2-N add fs / signal / thread / mem.
#[cfg(not(test))]
pub use z42_runtime::pal;
#[cfg(test)]
#[path = "../pal/mod.rs"]
pub mod pal;
// Public entry for the engine-independent object operations.
pub use vm::exec::ops::object as objops;
#[cfg(all(not(test), feature = "interp"))]
pub use z42_interp::interp;
#[cfg(test)]
#[path = "../../crates/z42-interp/src/interp/mod.rs"]
pub mod interp;
// add-runtime-feature-flags (P4.1): jit / aot are feature-gated.
// `default = ["jit"]` keeps backward compat; platforms (wasm / ios / android)
// build without the JIT backend by activating their preset features.
#[cfg(all(feature = "jit", z42_jit_supported))]
#[cfg(not(test))]
pub use z42_jit::jit;
#[cfg(all(test, feature = "jit", z42_jit_supported))]
#[path = "../../crates/z42-jit/src/jit/mod.rs"]
pub mod jit;
// Portable pin state is always available; interop gates its native ABI modules.
#[path = "../interop/mod.rs"]
pub mod interop;
// Embedding API (Tier 1 C ABI for host applications). Spec:
// docs/internals/src/runtime/embedding.md, 变更 add-embedding-api/.
#[path = "mod.rs"]
pub mod host;
#[cfg(not(test))]
pub use z42_runtime::vm;
#[cfg(test)]
#[path = "../vm/mod.rs"]
pub mod vm;
#[path = "../assembly/mod.rs"]
pub(crate) mod assembly;

// Runtime configuration registry — single source of truth for every
// Z42_* env var the runtime reads (docs/review.md Part 4 D1).
// `RuntimeConfig` carries the 5 startup-consumed knobs; `KNOWN_KNOBS`
// table also lists subsystem-local knobs (Z42_GC_* / Z42_NATIVE_PATH /
// Z42_SAFEPOINT_THROTTLE / Z42_STRESS_ITERS) for `--info` discovery.
#[path = "../config/mod.rs"]
pub mod config;

#[cfg(not(test))]
pub use z42_runtime::{core_identity, CORE_ABI};
#[cfg(test)]
#[path = "../identity.rs"]
mod identity;
#[cfg(test)]
pub use identity::{core_identity, CORE_ABI};

// Rust's shared std owns allocation helpers; every dynamic image uses its
// system allocator. Static CLI profiles retain their selected custom allocator.
#[cfg(all(feature = "shared-core", any(feature = "mimalloc-alloc", feature = "dhat-heap")))]
compile_error!("shared-core runtime packs require the system allocator; build with --no-default-features and omit mimalloc-alloc/dhat-heap");

#[cfg(not(test))]
pub use z42_runtime::components;
#[cfg(test)]
pub use vm::components;
