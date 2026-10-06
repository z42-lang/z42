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

/// The retention queries force a collection and walk every thread's frames for
/// roots, so they must stop the world first: while another mutator is running
/// (not parked, not at a safepoint) the query waits for it to park.
#[test]
fn retention_query_waits_for_running_mutators_to_park() {
    use crate::metadata::Value;
    use crate::vm_context::VmContext;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc};
    use std::time::{Duration, Instant};

    let ctx = VmContext::new();
    let target = ctx.heap().alloc_array(vec![Value::I64(1)]);
    ctx.core.static_fields.lock().push(target.clone());   // keep it reachable

    let stop = Arc::new(AtomicBool::new(false));
    let (registered_tx, registered_rx) = mpsc::channel();
    let worker = {
        let (core, stop) = (ctx.core_arc(), Arc::clone(&stop));
        std::thread::spawn(move || {
            let w = VmContext::new_with_core(core);
            registered_tx.send(()).unwrap();
            // Running z42 code: no safepoint for a while.
            std::thread::sleep(Duration::from_millis(200));
            while !stop.load(Ordering::Acquire) {
                w.safepoint_skip.store(1, Ordering::Relaxed);
                crate::gc::safepoint::check_safepoint(&w);
                std::thread::yield_now();
            }
        })
    };
    registered_rx.recv().unwrap();

    let start = Instant::now();
    // The result projection needs stdlib types a bare VmContext lacks; only the
    // timing matters here.
    let _ = super::builtin_heap_retaining_roots(&ctx, &[target]);
    assert!(start.elapsed() >= Duration::from_millis(150),
        "the query ran while another mutator was still running");

    stop.store(true, Ordering::Release);
    worker.join().unwrap();
}
