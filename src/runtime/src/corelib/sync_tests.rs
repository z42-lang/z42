//! Unit tests for `corelib/sync.rs`. Cross-thread end-to-end coverage
//! is in `runtime/tests/cross_thread_smoke.rs`; this file exercises the
//! single-thread happy paths + argument validation.

use super::*;
use crate::metadata::Value;

fn ctx() -> std::pin::Pin<Box<VmContext>> {
    VmContext::new()
}

// ── Mutex builtins ───────────────────────────────────────────────────────────

#[test]
fn mutex_new_missing_arg_errors() {
    let c = ctx();
    let err = builtin_mutex_new(&c, &[]).unwrap_err();
    assert!(err.to_string().contains("missing initial value"));
}

#[test]
fn mutex_new_returns_monotonic_slot_ids() {
    let c = ctx();
    let v1 = builtin_mutex_new(&c, &[Value::I64(1)]).unwrap();
    let v2 = builtin_mutex_new(&c, &[Value::I64(2)]).unwrap();
    let id1 = match v1 { Value::I64(n) => n, _ => panic!() };
    let id2 = match v2 { Value::I64(n) => n, _ => panic!() };
    assert!(id2 > id1, "second mutex id should be > first");
}

#[test]
fn mutex_acquire_returns_initial_value() {
    let c = ctx();
    let id = match builtin_mutex_new(&c, &[Value::I64(7)]).unwrap() {
        Value::I64(n) => n,
        _ => panic!(),
    };
    let v = builtin_mutex_lock_acquire(&c, &[Value::I64(id)]).unwrap();
    assert!(matches!(v, Value::I64(7)));
    // Clean up — unlock so we don't leave a held lock behind.
    builtin_mutex_unlock(&c, &[Value::I64(id)]).unwrap();
}

#[test]
fn mutex_store_then_acquire_observes_new_value() {
    let c = ctx();
    let id = match builtin_mutex_new(&c, &[Value::I64(0)]).unwrap() {
        Value::I64(n) => n,
        _ => panic!(),
    };
    builtin_mutex_lock_acquire(&c, &[Value::I64(id)]).unwrap();
    builtin_mutex_store(&c, &[Value::I64(id), Value::I64(42)]).unwrap();
    builtin_mutex_unlock(&c, &[Value::I64(id)]).unwrap();
    let v = builtin_mutex_lock_acquire(&c, &[Value::I64(id)]).unwrap();
    assert!(matches!(v, Value::I64(42)));
    builtin_mutex_unlock(&c, &[Value::I64(id)]).unwrap();
}

#[test]
fn mutex_store_without_acquire_errors() {
    let c = ctx();
    let id = match builtin_mutex_new(&c, &[Value::I64(0)]).unwrap() {
        Value::I64(n) => n,
        _ => panic!(),
    };
    let err = builtin_mutex_store(&c, &[Value::I64(id), Value::I64(99)]).unwrap_err();
    assert!(err.to_string().contains("not currently locked"));
}

#[test]
fn mutex_unlock_without_acquire_errors() {
    let c = ctx();
    let id = match builtin_mutex_new(&c, &[Value::I64(0)]).unwrap() {
        Value::I64(n) => n,
        _ => panic!(),
    };
    let err = builtin_mutex_unlock(&c, &[Value::I64(id)]).unwrap_err();
    assert!(err.to_string().contains("not currently locked"));
}

#[test]
fn mutex_lock_unknown_slot_errors() {
    let c = ctx();
    let err = builtin_mutex_lock_acquire(&c, &[Value::I64(9_999)]).unwrap_err();
    assert!(err.to_string().contains("unknown slot id"));
}

#[test]
fn mutex_slot_id_wrong_type_errors() {
    let c = ctx();
    let err = builtin_mutex_lock_acquire(&c, &[Value::Str("not an id".into())]).unwrap_err();
    assert!(err.to_string().contains("expected i64 slot id"));
}

// ── Channel builtins ─────────────────────────────────────────────────────────

#[test]
fn channel_new_returns_monotonic_slot_ids() {
    let c = ctx();
    let v1 = builtin_channel_new(&c, &[]).unwrap();
    let v2 = builtin_channel_new(&c, &[]).unwrap();
    let id1 = match v1 { Value::I64(n) => n, _ => panic!() };
    let id2 = match v2 { Value::I64(n) => n, _ => panic!() };
    assert!(id2 > id1);
}

/// Helper: extract the value from a `[I64(0), value]` recv result.
/// Panics on disconnect (used in happy-path tests).
fn expect_recv_ok(result: Value) -> Value {
    let arr = match result {
        Value::Array(rc) => rc,
        other => panic!("expected Array, got {other:?}"),
    };
    let borrowed = arr.borrow();
    assert!(matches!(borrowed.get_boxed(0), Value::I64(0)),
        "expected discriminator 0 (ok), got {:?}", borrowed.get_boxed(0));
    borrowed.get_boxed(1).clone()
}

#[test]
fn channel_send_then_recv_round_trip() {
    let c = ctx();
    let id = match builtin_channel_new(&c, &[]).unwrap() {
        Value::I64(n) => n,
        _ => panic!(),
    };
    builtin_channel_send(&c, &[Value::I64(id), Value::I64(101)]).unwrap();
    let v = expect_recv_ok(builtin_channel_recv(&c, &[Value::I64(id)]).unwrap());
    assert!(matches!(v, Value::I64(101)));
}

#[test]
fn channel_multi_send_recv_fifo_order() {
    let c = ctx();
    let id = match builtin_channel_new(&c, &[]).unwrap() {
        Value::I64(n) => n,
        _ => panic!(),
    };
    for n in 0..5_i64 {
        builtin_channel_send(&c, &[Value::I64(id), Value::I64(n)]).unwrap();
    }
    for expected in 0..5_i64 {
        let v = expect_recv_ok(builtin_channel_recv(&c, &[Value::I64(id)]).unwrap());
        match v {
            Value::I64(n) => assert_eq!(n, expected),
            _ => panic!("expected I64, got {:?}", v),
        }
    }
}

#[test]
fn channel_try_recv_empty_returns_discriminator_1() {
    let c = ctx();
    let id = match builtin_channel_new(&c, &[]).unwrap() {
        Value::I64(n) => n,
        _ => panic!(),
    };
    let result = builtin_channel_try_recv(&c, &[Value::I64(id)]).unwrap();
    let arr = match result {
        Value::Array(rc) => rc,
        _ => panic!("expected Array"),
    };
    let borrowed = arr.borrow();
    assert_eq!(borrowed.len(), 1);
    assert!(matches!(borrowed.get_boxed(0), Value::I64(1)),
        "empty try_recv should be discriminator 1, got {:?}", borrowed.get_boxed(0));
}

#[test]
fn channel_try_recv_ok_returns_value() {
    let c = ctx();
    let id = match builtin_channel_new(&c, &[]).unwrap() {
        Value::I64(n) => n,
        _ => panic!(),
    };
    builtin_channel_send(&c, &[Value::I64(id), Value::Str("hi".into())]).unwrap();
    let result = builtin_channel_try_recv(&c, &[Value::I64(id)]).unwrap();
    let arr = match result {
        Value::Array(rc) => rc,
        _ => panic!("expected Array"),
    };
    let borrowed = arr.borrow();
    assert_eq!(borrowed.len(), 2);
    assert!(matches!(borrowed.get_boxed(0), Value::I64(0)),
        "ok try_recv should be discriminator 0, got {:?}", borrowed.get_boxed(0));
    match &borrowed.get_boxed(1) {
        Value::Str(s) => assert_eq!(&**s, "hi"),
        other => panic!("expected Str, got {other:?}"),
    }
}

#[test]
fn channel_close_then_recv_returns_discriminator_2() {
    let c = ctx();
    let id = match builtin_channel_new(&c, &[]).unwrap() {
        Value::I64(n) => n,
        _ => panic!(),
    };
    builtin_channel_close(&c, &[Value::I64(id)]).unwrap();
    let result = builtin_channel_recv(&c, &[Value::I64(id)]).unwrap();
    let arr = match result {
        Value::Array(rc) => rc,
        _ => panic!("expected Array"),
    };
    let borrowed = arr.borrow();
    assert!(matches!(borrowed.get_boxed(0), Value::I64(2)),
        "disconnected recv should be discriminator 2, got {:?}", borrowed.get_boxed(0));
}

#[test]
fn channel_close_drains_remaining_then_disconnects() {
    let c = ctx();
    let id = match builtin_channel_new(&c, &[]).unwrap() {
        Value::I64(n) => n,
        _ => panic!(),
    };
    builtin_channel_send(&c, &[Value::I64(id), Value::I64(1)]).unwrap();
    builtin_channel_send(&c, &[Value::I64(id), Value::I64(2)]).unwrap();
    builtin_channel_close(&c, &[Value::I64(id)]).unwrap();
    // Queued values still drain before disconnect.
    let v1 = expect_recv_ok(builtin_channel_recv(&c, &[Value::I64(id)]).unwrap());
    let v2 = expect_recv_ok(builtin_channel_recv(&c, &[Value::I64(id)]).unwrap());
    assert!(matches!(v1, Value::I64(1)));
    assert!(matches!(v2, Value::I64(2)));
    // Now the channel is empty + disconnected.
    let result = builtin_channel_recv(&c, &[Value::I64(id)]).unwrap();
    let arr = match result {
        Value::Array(rc) => rc,
        _ => panic!("expected Array"),
    };
    let borrowed = arr.borrow();
    assert!(matches!(borrowed.get_boxed(0), Value::I64(2)),
        "third recv should be discriminator 2, got {:?}", borrowed.get_boxed(0));
}

#[test]
fn channel_try_recv_after_close_drained_returns_discriminator_2() {
    let c = ctx();
    let id = match builtin_channel_new(&c, &[]).unwrap() {
        Value::I64(n) => n,
        _ => panic!(),
    };
    builtin_channel_close(&c, &[Value::I64(id)]).unwrap();
    let result = builtin_channel_try_recv(&c, &[Value::I64(id)]).unwrap();
    let arr = match result {
        Value::Array(rc) => rc,
        _ => panic!("expected Array"),
    };
    let borrowed = arr.borrow();
    assert!(matches!(borrowed.get_boxed(0), Value::I64(2)),
        "disconnected try_recv should be discriminator 2, got {:?}", borrowed.get_boxed(0));
}

#[test]
fn channel_send_to_closed_errors() {
    let c = ctx();
    let id = match builtin_channel_new(&c, &[]).unwrap() {
        Value::I64(n) => n,
        _ => panic!(),
    };
    builtin_channel_close(&c, &[Value::I64(id)]).unwrap();
    let err = builtin_channel_send(&c, &[Value::I64(id), Value::I64(7)]).unwrap_err();
    assert!(err.to_string().contains("is closed"));
}

#[test]
fn channel_send_missing_value_errors() {
    let c = ctx();
    let id = match builtin_channel_new(&c, &[]).unwrap() {
        Value::I64(n) => n,
        _ => panic!(),
    };
    let err = builtin_channel_send(&c, &[Value::I64(id)]).unwrap_err();
    assert!(err.to_string().contains("missing value argument"));
}

#[test]
fn channel_recv_unknown_slot_errors() {
    let c = ctx();
    let err = builtin_channel_recv(&c, &[Value::I64(9_999)]).unwrap_err();
    assert!(err.to_string().contains("unknown slot id"));
}

// ── add-sync-primitives-bounded-channel Phase 4 (2026-05-20) ─────────────────

#[test]
fn channel_new_bounded_returns_slot_id() {
    let c = ctx();
    let v = builtin_channel_new_bounded(&c, &[Value::I64(4)]).unwrap();
    let id = match v { Value::I64(n) => n, _ => panic!("expected I64, got {v:?}") };
    assert!(id > 0);
}

#[test]
fn channel_new_bounded_missing_arg_errors() {
    let c = ctx();
    let err = builtin_channel_new_bounded(&c, &[]).unwrap_err();
    assert!(err.to_string().contains("missing capacity argument"));
}

#[test]
fn channel_new_bounded_negative_capacity_errors() {
    let c = ctx();
    let err = builtin_channel_new_bounded(&c, &[Value::I64(-1)]).unwrap_err();
    assert!(err.to_string().contains("capacity must be >= 0"));
}

#[test]
fn channel_new_bounded_wrong_type_errors() {
    let c = ctx();
    let err = builtin_channel_new_bounded(&c, &[Value::Str("not a number".into())]).unwrap_err();
    assert!(err.to_string().contains("expected i64 capacity"));
}

#[test]
fn channel_bounded_send_recv_round_trip() {
    let c = ctx();
    let id = match builtin_channel_new_bounded(&c, &[Value::I64(4)]).unwrap() {
        Value::I64(n) => n,
        _ => panic!(),
    };
    builtin_channel_send(&c, &[Value::I64(id), Value::I64(77)]).unwrap();
    let v = expect_recv_ok(builtin_channel_recv(&c, &[Value::I64(id)]).unwrap());
    assert!(matches!(v, Value::I64(77)));
}

#[test]
fn channel_bounded_multi_send_recv_fifo_order() {
    let c = ctx();
    let id = match builtin_channel_new_bounded(&c, &[Value::I64(8)]).unwrap() {
        Value::I64(n) => n,
        _ => panic!(),
    };
    for n in 0..5_i64 {
        builtin_channel_send(&c, &[Value::I64(id), Value::I64(n)]).unwrap();
    }
    for expected in 0..5_i64 {
        let v = expect_recv_ok(builtin_channel_recv(&c, &[Value::I64(id)]).unwrap());
        match v {
            Value::I64(n) => assert_eq!(n, expected),
            other => panic!("expected I64, got {other:?}"),
        }
    }
}

#[test]
fn channel_bounded_close_then_recv_returns_discriminator_2() {
    let c = ctx();
    let id = match builtin_channel_new_bounded(&c, &[Value::I64(4)]).unwrap() {
        Value::I64(n) => n,
        _ => panic!(),
    };
    builtin_channel_close(&c, &[Value::I64(id)]).unwrap();
    let result = builtin_channel_recv(&c, &[Value::I64(id)]).unwrap();
    let arr = match result {
        Value::Array(rc) => rc,
        _ => panic!("expected Array"),
    };
    let borrowed = arr.borrow();
    assert!(matches!(borrowed.get_boxed(0), Value::I64(2)),
        "disconnected recv on bounded should be discriminator 2, got {:?}", borrowed.get_boxed(0));
}

#[test]
fn channel_bounded_send_to_closed_errors() {
    let c = ctx();
    let id = match builtin_channel_new_bounded(&c, &[Value::I64(4)]).unwrap() {
        Value::I64(n) => n,
        _ => panic!(),
    };
    builtin_channel_close(&c, &[Value::I64(id)]).unwrap();
    let err = builtin_channel_send(&c, &[Value::I64(id), Value::I64(7)]).unwrap_err();
    assert!(err.to_string().contains("is closed"));
}

// ── add-sync-primitives-rwlock Phase 3 (2026-05-20) ──────────────────────────

fn make_rwlock(ctx: &VmContext, initial: Value) -> i64 {
    match builtin_rwlock_new(ctx, &[initial]).unwrap() {
        Value::I64(n) => n,
        other => panic!("expected I64 slot id, got {other:?}"),
    }
}

#[test]
fn rwlock_new_missing_arg_errors() {
    let c = ctx();
    let err = builtin_rwlock_new(&c, &[]).unwrap_err();
    assert!(err.to_string().contains("missing initial value"));
}

#[test]
fn rwlock_new_returns_monotonic_slot_ids() {
    let c = ctx();
    let id1 = make_rwlock(&c, Value::I64(1));
    let id2 = make_rwlock(&c, Value::I64(2));
    assert!(id2 > id1, "second slot id should be > first");
}

#[test]
fn rwlock_read_acquire_returns_initial_value_and_releases() {
    let c = ctx();
    let id = make_rwlock(&c, Value::I64(123));
    let v = builtin_rwlock_read_acquire(&c, &[Value::I64(id)]).unwrap();
    assert!(matches!(v, Value::I64(123)));
    builtin_rwlock_read_release(&c, &[Value::I64(id)]).unwrap();
}

#[test]
fn rwlock_write_store_then_read_observes_new_value() {
    let c = ctx();
    let id = make_rwlock(&c, Value::I64(0));
    let cur = builtin_rwlock_write_acquire(&c, &[Value::I64(id)]).unwrap();
    assert!(matches!(cur, Value::I64(0)));
    builtin_rwlock_write_store(&c, &[Value::I64(id), Value::I64(42)]).unwrap();
    builtin_rwlock_write_release(&c, &[Value::I64(id)]).unwrap();

    let observed = builtin_rwlock_read_acquire(&c, &[Value::I64(id)]).unwrap();
    builtin_rwlock_read_release(&c, &[Value::I64(id)]).unwrap();
    assert!(matches!(observed, Value::I64(42)));
}

#[test]
fn rwlock_write_store_without_write_acquire_errors() {
    let c = ctx();
    let id = make_rwlock(&c, Value::I64(0));
    let err = builtin_rwlock_write_store(&c, &[Value::I64(id), Value::I64(99)]).unwrap_err();
    assert!(err.to_string().contains("not currently locked"));
}

#[test]
fn rwlock_write_store_under_read_acquire_errors() {
    let c = ctx();
    let id = make_rwlock(&c, Value::I64(0));
    builtin_rwlock_read_acquire(&c, &[Value::I64(id)]).unwrap();
    let err = builtin_rwlock_write_store(&c, &[Value::I64(id), Value::I64(99)]).unwrap_err();
    assert!(err.to_string().contains("held in read mode"));
    builtin_rwlock_read_release(&c, &[Value::I64(id)]).unwrap();
}

#[test]
fn rwlock_release_mismatched_kind_errors() {
    let c = ctx();
    let id = make_rwlock(&c, Value::I64(0));
    builtin_rwlock_read_acquire(&c, &[Value::I64(id)]).unwrap();
    let err = builtin_rwlock_write_release(&c, &[Value::I64(id)]).unwrap_err();
    assert!(err.to_string().contains("held in read mode"));
    // Verify the read still holds — clean release path.
    builtin_rwlock_read_release(&c, &[Value::I64(id)]).unwrap();
}

#[test]
fn rwlock_read_release_when_held_write_errors() {
    let c = ctx();
    let id = make_rwlock(&c, Value::I64(0));
    builtin_rwlock_write_acquire(&c, &[Value::I64(id)]).unwrap();
    let err = builtin_rwlock_read_release(&c, &[Value::I64(id)]).unwrap_err();
    assert!(err.to_string().contains("held in write mode"));
    // Clean release.
    builtin_rwlock_write_release(&c, &[Value::I64(id)]).unwrap();
}

#[test]
fn rwlock_unknown_slot_errors() {
    let c = ctx();
    let err = builtin_rwlock_read_acquire(&c, &[Value::I64(9_999)]).unwrap_err();
    assert!(err.to_string().contains("unknown slot id"));
    let err = builtin_rwlock_write_acquire(&c, &[Value::I64(9_999)]).unwrap_err();
    assert!(err.to_string().contains("unknown slot id"));
}

// ── add-sync-primitives-try-variants Phase 2 (2026-05-20) ────────────────────

/// Helper: extract the i64 discriminator from a Value::Array result.
fn try_lock_discriminator(result: Value) -> i64 {
    let arr = match result {
        Value::Array(rc) => rc,
        other => panic!("expected Array, got {other:?}"),
    };
    let borrowed = arr.borrow();
    match borrowed.get_boxed(0) {
        Value::I64(n) => n,
        ref other => panic!("expected I64 discriminator, got {other:?}"),
    }
}

#[test]
fn channel_try_send_unbounded_succeeds() {
    let c = ctx();
    let id = match builtin_channel_new(&c, &[]).unwrap() {
        Value::I64(n) => n,
        _ => panic!(),
    };
    let result = builtin_channel_try_send(&c, &[Value::I64(id), Value::I64(42)]).unwrap();
    assert!(matches!(result, Value::I64(0)), "expected TRY_SEND_OK (0)");
    // Verify the value actually went through
    let v = expect_recv_ok(builtin_channel_recv(&c, &[Value::I64(id)]).unwrap());
    assert!(matches!(v, Value::I64(42)));
}

#[test]
fn channel_try_send_bounded_full_returns_1() {
    let c = ctx();
    let id = match builtin_channel_new_bounded(&c, &[Value::I64(2)]).unwrap() {
        Value::I64(n) => n,
        _ => panic!(),
    };
    // Fill the buffer.
    builtin_channel_send(&c, &[Value::I64(id), Value::I64(1)]).unwrap();
    builtin_channel_send(&c, &[Value::I64(id), Value::I64(2)]).unwrap();
    // Third TrySend should fail with FULL discriminator.
    let result = builtin_channel_try_send(&c, &[Value::I64(id), Value::I64(3)]).unwrap();
    assert!(matches!(result, Value::I64(1)), "expected TRY_SEND_FULL (1)");
}

#[test]
fn channel_try_send_closed_returns_2() {
    let c = ctx();
    let id = match builtin_channel_new(&c, &[]).unwrap() {
        Value::I64(n) => n,
        _ => panic!(),
    };
    builtin_channel_close(&c, &[Value::I64(id)]).unwrap();
    let result = builtin_channel_try_send(&c, &[Value::I64(id), Value::I64(42)]).unwrap();
    assert!(matches!(result, Value::I64(2)), "expected TRY_SEND_DISCONNECTED (2)");
}

#[test]
fn rwlock_try_read_uncontended_succeeds() {
    let c = ctx();
    let id = make_rwlock(&c, Value::I64(100));
    let result = builtin_rwlock_try_read(&c, &[Value::I64(id)]).unwrap();
    let kind = try_lock_discriminator(result.clone());
    assert_eq!(kind, 0);
    // Value present at index 1.
    let arr = match result {
        Value::Array(rc) => rc,
        _ => panic!(),
    };
    let borrowed = arr.borrow();
    assert!(matches!(borrowed.get_boxed(1), Value::I64(100)));
    drop(borrowed);
    // Must release.
    builtin_rwlock_read_release(&c, &[Value::I64(id)]).unwrap();
}

#[test]
fn rwlock_try_read_same_thread_reentrancy_rejected() {
    let c = ctx();
    let id = make_rwlock(&c, Value::I64(0));
    builtin_rwlock_try_read(&c, &[Value::I64(id)]).unwrap();
    // Second try on the same slot from the same thread must NOT acquire
    // (HELD_RWLOCK_GUARDS reentrancy check).
    let result = builtin_rwlock_try_read(&c, &[Value::I64(id)]).unwrap();
    assert_eq!(try_lock_discriminator(result), 1);
    builtin_rwlock_read_release(&c, &[Value::I64(id)]).unwrap();
}

#[test]
fn rwlock_try_write_uncontended_succeeds() {
    let c = ctx();
    let id = make_rwlock(&c, Value::I64(0));
    let result = builtin_rwlock_try_write(&c, &[Value::I64(id)]).unwrap();
    assert_eq!(try_lock_discriminator(result), 0);
    builtin_rwlock_write_store(&c, &[Value::I64(id), Value::I64(99)]).unwrap();
    builtin_rwlock_write_release(&c, &[Value::I64(id)]).unwrap();

    // Verify via blocking read.
    let observed = builtin_rwlock_read_acquire(&c, &[Value::I64(id)]).unwrap();
    builtin_rwlock_read_release(&c, &[Value::I64(id)]).unwrap();
    assert!(matches!(observed, Value::I64(99)));
}

#[test]
fn rwlock_try_write_during_outstanding_read_returns_contended() {
    let c = ctx();
    let id = make_rwlock(&c, Value::I64(0));
    // Acquire read on the same thread.
    builtin_rwlock_read_acquire(&c, &[Value::I64(id)]).unwrap();
    // try_write should fail because thread-local reentrancy check (we
    // hold the slot, even in read mode).
    let result = builtin_rwlock_try_write(&c, &[Value::I64(id)]).unwrap();
    assert_eq!(try_lock_discriminator(result), 1);
    builtin_rwlock_read_release(&c, &[Value::I64(id)]).unwrap();
}

#[test]
fn rwlock_try_read_unknown_slot_errors() {
    let c = ctx();
    let err = builtin_rwlock_try_read(&c, &[Value::I64(9_999)]).unwrap_err();
    assert!(err.to_string().contains("unknown slot id"));
}
