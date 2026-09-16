//! AOT (ahead-of-time) backend — compiles z42 bytecode to standalone native
//! binaries.
//!
//! **STATUS: STUB** — `run()` returns an error; no compilation logic exists.
//!
//! Planned implementation: **cranelift-AOT** — reuse the JIT's
//! `jit/translate.rs` (generic over `cranelift_module::Module`) emitting via
//! `cranelift-object` (`.o`), NOT LLVM. Design: `docs/internals/src/runtime/aot.md`.
//! Tracked as `docs/roadmap.md` milestone **M9** (L3 phase, 📋 planned).
//!
//! Until M9 lands, attempting to execute a function with `ExecMode::Aot`
//! returns the error below; the user-visible message in `vm.rs::Vm::run`
//! reproduces the same milestone hint.

use crate::metadata::{Function, Module};
use anyhow::Result;

pub fn run(_module: &Module, _func: &Function) -> Result<()> {
    anyhow::bail!(
        "AOT backend is not implemented yet (planned: roadmap M9 — L3 phase, \
         cranelift-AOT; see docs/internals/src/runtime/aot.md). Switch to `--mode interp` or `--mode jit`."
    )
}
