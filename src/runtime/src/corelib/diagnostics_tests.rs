//! Tests for diagnostics builtins registration.
//!
//! The `builtin_diag_counters` *projection* (snapshot → z42 object) needs a
//! loaded `Std.Diagnostics.RuntimeCounters` type, which a bare `VmContext`
//! test can't provide — that path is covered end-to-end by the z42 `[Test]`
//! dogfood in `src/libraries/z42.diagnostics/tests/runtime_counters.z42`
//! (run under `xtask test stdlib`). Here we assert the registration
//! discipline that Rust *can* check without stdlib types.

use crate::corelib::{builtin_id_of, BUILTINS};

#[test]
fn diag_counters_is_registered() {
    // Name resolves to a valid static BuiltinId.
    let id = builtin_id_of("__diag_counters").expect("__diag_counters must be registered");
    assert!((id.0 as usize) < BUILTINS.len());
    // Points at the diagnostics builtin entry (name matches at that index).
    assert_eq!(BUILTINS[id.0 as usize].0, "__diag_counters");
}

#[test]
fn diag_counters_appended_last_preserves_ids() {
    // expose-diagnostics-counters appended `__diag_counters` at the END of
    // BUILTINS to keep every prior BuiltinId stable (append-only discipline).
    // If a later change inserts *before* it, this test still passes as long as
    // ids stay stable; the invariant we lock here is that the entry exists and
    // its id equals its array position (positional == BuiltinId).
    let id = builtin_id_of("__diag_counters").unwrap();
    let pos = BUILTINS.iter().position(|(n, _)| *n == "__diag_counters").unwrap();
    assert_eq!(id.0 as usize, pos, "BuiltinId must equal BUILTINS array position");
}
