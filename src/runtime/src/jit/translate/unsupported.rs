//! JIT-untranslatable opcode table (single source for prescan + bail arms).
//!
//! Split out of `translate/mod.rs` by refactor-jit-translate-split (H2).

use super::*;

/// Return `Some(reason)` if `func` contains an opcode the JIT cannot yet
/// translate, otherwise `None`. Keep this list in lock-step with the `bail!`
/// arms in `translate_instr` below — every opcode that bails there must be
/// reported here so `compile_module` can skip the function up front rather than
/// abort mid-translation.
///
/// fix-jit-cross-zpkg-transitive-eager (2026-06-20): with `--mode jit` now the
/// default and eager loading pulling the *whole* transitive dep closure into
/// the module, the merged module routinely contains stdlib functions that use
/// `out`/`ref` params (`LoadLocalAddr`) or native interop (`CallNative`). Those
/// must degrade per-function to interp (jit_call misses `fn_entries` → the
/// `cross_zpkg_via_interp` fallback runs the bytecode), not fail the program.
pub(crate) fn jit_unsupported_reason(func: &Function) -> Option<&'static str> {
    for block in &func.blocks {
        for instr in &block.instructions {
            if let Some(reason) = unsupported_reason(instr) {
                return Some(reason);
            }
        }
    }
    None
}
