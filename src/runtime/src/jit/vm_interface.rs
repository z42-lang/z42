//! `JitVm` — read-only metadata contract between the JIT backend and the
//! VM module representation.
//!
//! review.md Part 1 P0 / Part 5 E1.P2 Phase 1 (2026-06-02). The original
//! review prescribed routing translate.rs *and* every helper through a
//! single VmInterface trait, but two structural facts cap that aspiration:
//!
//! 1. `translate.rs` pattern-matches on `Instruction` enum across 100+
//!    arms. The IR types ARE the JIT's input language — they cannot be
//!    hidden behind a trait without throwing the visitor pattern away.
//! 2. Helpers reach `Module` via the `*const Module` field in
//!    `JitModuleCtx`. A `*const dyn JitVm` is a fat pointer; replacing
//!    the raw pointer with it would break the extern "C" ABI that the
//!    Cranelift-compiled code uses to call helpers.
//!
//! Phase 1 (this file): define the trait + implement it on `Module` + use
//! it from `compile_module` (the JIT build-time path that's call-site,
//! not raw-pointer-bound) + one helper as exemplar
//! (`jit_obj_new`). Doesn't change `compile_module(&Module)` signature.
//!
//! Phase 2 (separate spec, not started): migrate the remaining 9 helpers
//! to call trait methods through the same raw-pointer-but-typed indirection
//! pattern that `jit_obj_new` will use here. Phase 3 (likely Phase 2.5)
//! would address generic-ifying `compile_module` so AOT can plug in a
//! different `JitVm` impl.
//!
//! # Why introduce the trait at all if Phase 1 is so limited
//!
//! - **Codifies the read surface**: today readers of `compile_module` must
//!   grep for `module.*` accesses to know what JIT touches at build time.
//!   The trait makes it a single sealed list.
//! - **Mockability**: unit tests can construct a tiny mock implementing
//!   only the methods they need, without building a full `Module`.
//! - **Phase 2/3 lever**: when the time comes to make `compile_module`
//!   generic, the trait is already there; we just change a signature.

use crate::metadata::{Function, Module, TypeDesc};
use std::sync::Arc;

/// Read-only metadata contract the JIT backend uses against the source
/// module. Implemented by [`Module`] (the normal path) and — once Phase 2
/// lands — possibly by an AOT-friendly metadata view.
///
/// All methods return borrows of data owned by the implementor; the
/// borrows must remain valid for the implementor's `&self` lifetime.
///
/// `#[allow(dead_code)]`: this trait is forward-looking — the `string_pool`
/// accessor is documented contract but the actual consumers (JIT helpers +
/// future AOT) migrate to call it in Phase 2+.
/// Without the lint suppression the dead_code warning escapes into
/// stderr during release builds; the cross-zpkg test runner parses stderr
/// to compare against expected golden output and treats extraneous
/// warning lines as test failures.
#[allow(dead_code)]
pub trait JitVm {
    /// Every function declared in this module, in declaration order. The
    /// slot index matches `MethodId.0` for module-local functions.
    fn functions(&self) -> &[Function];

    /// String pool — shared across the module. Indexed by
    /// `Instruction::ConstStr.idx` and other `StringId(u32)` references.
    fn string_pool(&self) -> &[String];

    /// Fully-qualified module name (e.g. `"Demo.App"`). Used by JIT
    /// observability events + crash diagnostics.
    fn module_name(&self) -> &str;

    /// Look up a `TypeDesc` by fully-qualified class name in this
    /// module's local registry. Returns `None` if not found.
    ///
    /// **Cross-zpkg semantics**: this does NOT walk the `LazyLoader`.
    /// Helpers that need cross-zpkg fallback continue going through
    /// `VmContext` directly until Phase 2 expands the trait.
    fn type_lookup(&self, class_name: &str) -> Option<&Arc<TypeDesc>>;
}

impl JitVm for Module {
    #[inline]
    fn functions(&self) -> &[Function] {
        &self.functions
    }

    #[inline]
    fn string_pool(&self) -> &[String] {
        &self.string_pool
    }

    #[inline]
    fn module_name(&self) -> &str {
        &self.name
    }

    #[inline]
    fn type_lookup(&self, class_name: &str) -> Option<&Arc<TypeDesc>> {
        self.type_registry.get(class_name)
    }
}

#[cfg(test)]
#[path = "vm_interface_tests.rs"]
mod vm_interface_tests;
