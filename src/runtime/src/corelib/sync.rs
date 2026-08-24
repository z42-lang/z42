//! `Std.Threading.Mutex<T>` / `Channel<T>` builtins
//! (add-sync-primitives, 2026-05-20).
//!
//! Layered on top of `add-threading-stdlib`:
//! - **Mutex** wraps `parking_lot::Mutex<Value>` keyed by monotonic slot id
//!   in `VmCore.mutexes`. RAII callback API: z42's `Mutex.Lock(Func<T,T>)`
//!   decomposes into 3 native calls — `__mutex_lock_acquire`, `__mutex_store`,
//!   `__mutex_unlock` — bracketing the user callback. Acquired guards are
//!   parked in a thread-local map keyed by slot id so the matching unlock
//!   on the same thread can find them.
//! - **Channel** wraps `std::sync::mpsc` keyed by slot id in `VmCore.channels`.
//!   Multi-producer is supported by `Sender::clone()` per `Send` call; the
//!   single-consumer constraint is preserved by an internal `Mutex` around
//!   the `Receiver`.
//!
//! Cross-thread error semantics:
//! - `__mutex_*` parameter validation → anyhow Err (worker dies via
//!   `__thread_join` discriminator 1)
//! - `__channel_recv` after all senders closed → throws
//!   `Std.ChannelDisconnectedException` (encoded via the existing exception
//!   protocol in the z42 facade)
//!
//! See [`docs/spec/changes/add-sync-primitives/`].

use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{anyhow, bail, Result};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc;

// ── ChannelSlot ───────────────────────────────────────────────────────────────

/// Sender variant — unbounded mpsc or bounded (sync) mpsc. The two share
/// the same Receiver type so all consumer paths (`__channel_recv` /
/// `__channel_try_recv` / `__channel_close`) are agnostic; only
/// `__channel_send` cares about the variant for back-pressure semantics
/// (Bounded::send blocks when full; Unbounded::send never blocks).
///
/// add-sync-primitives-bounded-channel (2026-05-20).
pub(crate) enum ChannelSender {
    Unbounded(mpsc::Sender<Value>),
    Bounded(mpsc::SyncSender<Value>),
}

/// One live channel pair. `sender` is `Option` so `__channel_close` can
/// drop it (cause subsequent `Recv` to fail with `RecvError`). `receiver`
/// is wrapped in `Arc<Mutex<...>>` so callers can clone the Arc out of
/// the registry, drop the outer registry lock, and only then block on
/// `recv()` — otherwise a concurrent `__channel_send` would deadlock
/// trying to take the registry lock while the receiver thread blocks
/// holding it. mpsc::Receiver is `Send` but `!Sync`, so the inner Mutex
/// serialises concurrent `__channel_recv` callers on the same slot.
pub(crate) struct ChannelSlot {
    pub(crate) sender:   Option<ChannelSender>,
    pub(crate) receiver: Arc<std::sync::Mutex<mpsc::Receiver<Value>>>,
}

// ── thread-local held-guard parking ───────────────────────────────────────────

thread_local! {
    /// `slot_id` → leaked `parking_lot::MutexGuard`'s underlying raw pointer
    /// to the live `Mutex<Value>`. Held between `__mutex_lock_acquire` and
    /// the matching `__mutex_unlock` on the same OS thread.
    ///
    /// The Arc<Mutex<Value>> stored in `VmCore.mutexes` keeps the underlying
    /// `parking_lot::Mutex` alive while we hold the lock; `__mutex_unlock`
    /// calls `Mutex::force_unlock_*` on the same raw mutex pointer.
    ///
    /// Tracked here per-thread because parking_lot Mutex is non-reentrant
    /// and a thread holding the lock is the only one allowed to release it.
    static HELD_MUTEX_GUARDS: RefCell<HashMap<u64, Arc<parking_lot::Mutex<Value>>>>
        = RefCell::new(HashMap::new());

    /// add-sync-primitives-rwlock (2026-05-20): per-thread map from RwLock
    /// slot id to the held variant. Read variants may co-exist across
    /// threads (parking_lot RwLock allows multiple shared holders); write
    /// variants are exclusive per slot.
    static HELD_RWLOCK_GUARDS: RefCell<HashMap<u64, RwLockHeld>>
        = RefCell::new(HashMap::new());
}

/// Per-thread RwLock acquire mode tracking. Held entries are removed by
/// the matching `*_release` builtin which dispatches on this variant to
/// pick the correct parking_lot raw unlock path.
#[derive(Clone)]
pub(crate) enum RwLockHeld {
    Read(Arc<parking_lot::RwLock<Value>>),
    Write(Arc<parking_lot::RwLock<Value>>),
}

// ── const discriminators ─────────────────────────────────────────────────────

const TRY_RECV_OK:           i64 = 0;
const TRY_RECV_EMPTY:        i64 = 1;
const TRY_RECV_DISCONNECTED: i64 = 2;

// add-sync-primitives-try-variants (2026-05-20): discriminators for
// __channel_try_send + __rwlock_try_{read,write}.
const TRY_SEND_OK:           i64 = 0;
const TRY_SEND_FULL:         i64 = 1;
const TRY_SEND_DISCONNECTED: i64 = 2;

const TRY_LOCK_OK:           i64 = 0;
const TRY_LOCK_CONTENDED:    i64 = 1;

// add-concurrency-probes (P1b): user-lock contention probes live in a sibling
// module (keeps this file under the 500-line limit). `#[cfg]`-gated on
// `profile-contention`; the default build's helpers are zero-cost forwards.
#[path = "sync_contention.rs"]
mod sync_contention;
use sync_contention::{contended_lock, contended_read, contended_write};

// ── Mutex builtins ────────────────────────────────────────────────────────────

/// `__mutex_new(initial: Value) -> i64 slot_id` — allocate a new Mutex
/// wrapping `initial` and register it in `VmCore.mutexes`.
pub fn builtin_mutex_new(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let initial = args.first()
        .ok_or_else(|| anyhow!("__mutex_new: missing initial value"))?
        .clone();

    let id = ctx.core.mutexes.alloc_id();
    let mutex = Arc::new(parking_lot::Mutex::new(initial));
    ctx.core.mutexes.lock().insert(id, mutex);
    Ok(Value::I64(id as i64))
}

/// `__mutex_lock_acquire(slot: i64) -> Value` — block until the Mutex is
/// available, return a clone of the current stored value. The acquired
/// guard is parked in the thread-local `HELD_MUTEX_GUARDS` map; the matching
/// `__mutex_unlock` releases it.
pub fn builtin_mutex_lock_acquire(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot = slot_id_arg(args, 0, "__mutex_lock_acquire")?;
    let arc = ctx.core.mutexes.lock().get(&slot).cloned()
        .ok_or_else(|| anyhow!("__mutex_lock_acquire: unknown slot id {slot}"))?;
    // SAFETY (block-on-acquire): parking_lot::Mutex::lock is the standard
    // blocking acquire. We immediately drop the MutexGuard via mem::forget
    // after cloning the stored Value, then store the Arc in the thread-local
    // map. The Arc keeps the Mutex alive; we release the lock by calling
    // `unsafe { Mutex::force_unlock() }` from `__mutex_unlock` on the same
    // thread. parking_lot's lock_api permits this pattern — it's the
    // documented escape hatch when MutexGuard lifetime can't be expressed
    // in plain Rust (cross-FFI / cross-builtin call here).
    //
    // add-concurrency-probes (2026-08-23, script-profiling P1b): under the
    // `profile-contention` feature, a `try_lock` first tells us whether the
    // acquire was contended (lock already held) and, if so, times the blocking
    // wait. The default build compiles this out entirely → plain `arc.lock()`.
    let guard = contended_lock(ctx, &arc);
    let cloned = (*guard).clone();
    // Forget the guard so it does NOT drop and unlock at end of scope.
    std::mem::forget(guard);
    HELD_MUTEX_GUARDS.with(|cell| {
        cell.borrow_mut().insert(slot, Arc::clone(&arc));
    });
    Ok(cloned)
}

/// `__mutex_store(slot: i64, new_val: Value) -> Null` — overwrite the
/// stored Value. Caller must currently hold the lock on the same thread
/// (enforced via `HELD_MUTEX_GUARDS` presence).
pub fn builtin_mutex_store(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot = slot_id_arg(args, 0, "__mutex_store")?;
    let new_val = args.get(1)
        .ok_or_else(|| anyhow!("__mutex_store: missing new value"))?
        .clone();

    let arc = HELD_MUTEX_GUARDS.with(|cell| cell.borrow().get(&slot).cloned())
        .ok_or_else(|| anyhow!(
            "__mutex_store: slot {slot} not currently locked on this thread"
        ))?;
    // SAFETY: caller currently owns the lock on this thread (we acquired
    // and forgot the MutexGuard in `__mutex_lock_acquire`; only this
    // thread can unlock it). data_ptr() returns a raw pointer to the
    // protected value — writing through it is the same as writing through
    // the MutexGuard would have been.
    unsafe {
        *arc.data_ptr() = new_val;
    }
    Ok(Value::Null)
}

/// `__mutex_unlock(slot: i64) -> Null` — release a previously acquired
/// lock. Must be called on the same OS thread that did the acquire;
/// non-paired unlock is an error (returns Err rather than panicking, so
/// the worker thread surfaces it as a ThreadException).
pub fn builtin_mutex_unlock(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot = slot_id_arg(args, 0, "__mutex_unlock")?;
    let arc = HELD_MUTEX_GUARDS.with(|cell| cell.borrow_mut().remove(&slot))
        .ok_or_else(|| anyhow!(
            "__mutex_unlock: slot {slot} not currently locked on this thread"
        ))?;
    // SAFETY: this thread acquired the lock via `lock()` + `mem::forget`
    // in `__mutex_lock_acquire`. parking_lot's `force_unlock` documents
    // exactly this pattern — caller asserts ownership of the locked state.
    unsafe {
        arc.force_unlock();
    }
    Ok(Value::Null)
}

// ── Channel builtins ─────────────────────────────────────────────────────────

/// `__channel_new() -> i64 slot_id` — create an unbounded mpsc channel
/// and register it in `VmCore.channels`.
pub fn builtin_channel_new(ctx: &VmContext, _args: &[Value]) -> Result<Value> {
    let (tx, rx) = mpsc::channel::<Value>();
    let id = ctx.core.channels.alloc_id();
    ctx.core.channels.lock().insert(id, ChannelSlot {
        sender:   Some(ChannelSender::Unbounded(tx)),
        receiver: Arc::new(std::sync::Mutex::new(rx)),
    });
    Ok(Value::I64(id as i64))
}

/// `__channel_new_bounded(capacity: i64) -> i64 slot_id` — create a
/// bounded sync channel (add-sync-primitives-bounded-channel, 2026-05-20).
/// `Send` on the resulting channel blocks when the queue reaches `capacity`
/// values; `capacity == 0` gives a rendezvous channel (every Send blocks
/// until paired with a Recv).
pub fn builtin_channel_new_bounded(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let cap = match args.first() {
        Some(Value::I64(n)) if *n >= 0 => *n as usize,
        Some(Value::I64(_)) => bail!("__channel_new_bounded: capacity must be >= 0"),
        Some(other) => bail!("__channel_new_bounded: expected i64 capacity, got {:?}", other),
        None        => bail!("__channel_new_bounded: missing capacity argument"),
    };
    let (tx, rx) = mpsc::sync_channel::<Value>(cap);
    let id = ctx.core.channels.alloc_id();
    ctx.core.channels.lock().insert(id, ChannelSlot {
        sender:   Some(ChannelSender::Bounded(tx)),
        receiver: Arc::new(std::sync::Mutex::new(rx)),
    });
    Ok(Value::I64(id as i64))
}

/// `__channel_send(slot: i64, v: Value) -> Null` — send `v` on the
/// channel. Returns `Err` if the channel slot is unknown or closed.
///
/// Dispatches on `ChannelSender` variant: unbounded send is non-blocking;
/// bounded send may block until the queue has room (back-pressure).
pub fn builtin_channel_send(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot = slot_id_arg(args, 0, "__channel_send")?;
    let val  = args.get(1)
        .ok_or_else(|| anyhow!("__channel_send: missing value argument"))?
        .clone();

    enum SendHandle {
        Unbounded(mpsc::Sender<Value>),
        Bounded(mpsc::SyncSender<Value>),
    }
    let handle: SendHandle = {
        let map = ctx.core.channels.lock();
        let chan = map.get(&slot)
            .ok_or_else(|| anyhow!("__channel_send: unknown slot id {slot}"))?;
        let sender = chan.sender.as_ref()
            .ok_or_else(|| anyhow!("__channel_send: channel {slot} is closed"))?;
        match sender {
            ChannelSender::Unbounded(tx) => SendHandle::Unbounded(tx.clone()),
            ChannelSender::Bounded(tx)   => SendHandle::Bounded(tx.clone()),
        }
    };
    // Registry lock released before potentially-blocking send (bounded
    // case must not hold the registry lock or concurrent recv/close paths
    // would deadlock).
    let send_result = match handle {
        SendHandle::Unbounded(tx) => tx.send(val).map_err(|_| ()),
        SendHandle::Bounded(tx)   => tx.send(val).map_err(|_| ()),
    };
    send_result.map_err(|_| anyhow!("__channel_send: channel {slot} disconnected"))?;
    Ok(Value::Null)
}

/// `__channel_recv(slot: i64) -> Value::Array` — block until a value
/// arrives. Returns the same discriminated shape as `__channel_try_recv`
/// minus the `EMPTY` case (which doesn't apply to a blocking recv):
///   `[I64(0), value]`  — ok
///   `[I64(2)]`         — disconnected (all senders closed, queue drained)
///
/// The discriminator-rather-than-anyhow protocol lets the z42 `Channel.Recv()`
/// facade `throw new Std.ChannelDisconnectedException(...)` so user code
/// can `catch (ChannelDisconnectedException e)` cleanly.
pub fn builtin_channel_recv(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot = slot_id_arg(args, 0, "__channel_recv")?;
    let rx_arc = {
        let map = ctx.core.channels.lock();
        let chan = map.get(&slot)
            .ok_or_else(|| anyhow!("__channel_recv: unknown slot id {slot}"))?;
        Arc::clone(&chan.receiver)
    };
    // Registry lock released. Inner Mutex serialises concurrent
    // `__channel_recv` callers on the same slot (Receiver is !Sync);
    // a concurrent `__channel_send` takes the registry lock, clones the
    // Sender, drops the registry lock, and sends — so we don't block
    // each other.
    let rx_guard = rx_arc.lock()
        .map_err(|_| anyhow!("__channel_recv: receiver mutex poisoned"))?;
    let arr = match rx_guard.recv() {
        Ok(v)  => vec![Value::I64(TRY_RECV_OK), v],
        Err(_) => vec![Value::I64(TRY_RECV_DISCONNECTED)],
    };
    Ok(ctx.heap().alloc_array(arr))
}

/// `__channel_try_recv(slot: i64) -> Value::Array` — non-blocking recv,
/// returns a discriminated array (see module docs).
pub fn builtin_channel_try_recv(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot = slot_id_arg(args, 0, "__channel_try_recv")?;
    let rx_arc = {
        let map = ctx.core.channels.lock();
        let chan = map.get(&slot)
            .ok_or_else(|| anyhow!("__channel_try_recv: unknown slot id {slot}"))?;
        Arc::clone(&chan.receiver)
    };
    let rx_guard = rx_arc.lock()
        .map_err(|_| anyhow!("__channel_try_recv: receiver mutex poisoned"))?;
    let arr = match rx_guard.try_recv() {
        Ok(v) => vec![Value::I64(TRY_RECV_OK), v],
        Err(mpsc::TryRecvError::Empty) => vec![Value::I64(TRY_RECV_EMPTY)],
        Err(mpsc::TryRecvError::Disconnected) => vec![Value::I64(TRY_RECV_DISCONNECTED)],
    };
    Ok(ctx.heap().alloc_array(arr))
}

/// `__channel_close(slot: i64) -> Null` — drop the sender half so
/// pending and future `Recv` calls return `Disconnected` after the
/// already-queued values are drained.
pub fn builtin_channel_close(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot = slot_id_arg(args, 0, "__channel_close")?;
    let mut map = ctx.core.channels.lock();
    let chan = map.get_mut(&slot)
        .ok_or_else(|| anyhow!("__channel_close: unknown slot id {slot}"))?;
    chan.sender = None;
    Ok(Value::Null)
}

/// `__channel_try_send(slot: i64, v: Value) -> i64` — non-blocking send
/// (add-sync-primitives-try-variants, 2026-05-20). Returns the same
/// discriminator family as `__channel_try_recv`:
///   `0` ok / `1` full (bounded only) / `2` disconnected
///
/// Unbounded channels never report "full" — they fall back to `Sender::send`
/// which never blocks; they only return `2` when the channel is closed.
pub fn builtin_channel_try_send(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot = slot_id_arg(args, 0, "__channel_try_send")?;
    let val  = args.get(1)
        .ok_or_else(|| anyhow!("__channel_try_send: missing value argument"))?
        .clone();

    enum SendHandle {
        Unbounded(mpsc::Sender<Value>),
        Bounded(mpsc::SyncSender<Value>),
    }
    let handle: SendHandle = {
        let map = ctx.core.channels.lock();
        let chan = map.get(&slot)
            .ok_or_else(|| anyhow!("__channel_try_send: unknown slot id {slot}"))?;
        let sender = match chan.sender.as_ref() {
            Some(s) => s,
            None    => return Ok(Value::I64(TRY_SEND_DISCONNECTED)),
        };
        match sender {
            ChannelSender::Unbounded(tx) => SendHandle::Unbounded(tx.clone()),
            ChannelSender::Bounded(tx)   => SendHandle::Bounded(tx.clone()),
        }
    };
    let kind = match handle {
        SendHandle::Unbounded(tx) => match tx.send(val) {
            Ok(())  => TRY_SEND_OK,
            Err(_)  => TRY_SEND_DISCONNECTED,
        },
        SendHandle::Bounded(tx) => match tx.try_send(val) {
            Ok(())                                    => TRY_SEND_OK,
            Err(mpsc::TrySendError::Full(_))          => TRY_SEND_FULL,
            Err(mpsc::TrySendError::Disconnected(_))  => TRY_SEND_DISCONNECTED,
        },
    };
    Ok(Value::I64(kind))
}

// ── RwLock builtins (add-sync-primitives-rwlock, 2026-05-20) ─────────────────

/// `__rwlock_new(initial: Value) -> i64 slot_id` — allocate a new
/// RwLock wrapping `initial` and register it in `VmCore.rwlocks`.
pub fn builtin_rwlock_new(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let initial = args.first()
        .ok_or_else(|| anyhow!("__rwlock_new: missing initial value"))?
        .clone();
    let id = ctx.core.rwlocks.alloc_id();
    let lock = Arc::new(parking_lot::RwLock::new(initial));
    ctx.core.rwlocks.lock().insert(id, lock);
    Ok(Value::I64(id as i64))
}

/// `__rwlock_read_acquire(slot: i64) -> Value` — acquire shared lock
/// (blocks while a writer holds it; multiple readers may proceed
/// concurrently), return a clone of the current stored value.
pub fn builtin_rwlock_read_acquire(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot = slot_id_arg(args, 0, "__rwlock_read_acquire")?;
    let arc = ctx.core.rwlocks.lock().get(&slot).cloned()
        .ok_or_else(|| anyhow!("__rwlock_read_acquire: unknown slot id {slot}"))?;
    // SAFETY: parking_lot::RwLock::read is the blocking shared acquire.
    // We mem::forget the guard so it doesn't unlock at scope end and
    // pair release with `force_unlock_read` via the thread-local map.
    // add-concurrency-probes: `profile-contention` feature times contended reads.
    let guard = contended_read(ctx, &arc);
    let cloned = (*guard).clone();
    std::mem::forget(guard);
    HELD_RWLOCK_GUARDS.with(|cell| {
        cell.borrow_mut().insert(slot, RwLockHeld::Read(Arc::clone(&arc)));
    });
    Ok(cloned)
}

/// `__rwlock_read_release(slot: i64) -> Null` — release a previously
/// acquired shared lock. Errors if the slot wasn't held in Read mode
/// on this thread (e.g. attempting to read-release a write-acquired slot).
pub fn builtin_rwlock_read_release(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot = slot_id_arg(args, 0, "__rwlock_read_release")?;
    let held = HELD_RWLOCK_GUARDS.with(|cell| cell.borrow_mut().remove(&slot))
        .ok_or_else(|| anyhow!("__rwlock_read_release: slot {slot} not currently locked on this thread"))?;
    match held {
        RwLockHeld::Read(arc) => {
            // SAFETY: this thread holds the shared lock (acquired via
            // arc.read() + mem::forget above). force_unlock_read decrements
            // the shared counter on the same parking_lot::RwLock.
            unsafe { arc.force_unlock_read(); }
            Ok(Value::Null)
        }
        RwLockHeld::Write(arc) => {
            // Put it back so the caller can recover.
            HELD_RWLOCK_GUARDS.with(|cell| {
                cell.borrow_mut().insert(slot, RwLockHeld::Write(arc));
            });
            bail!("__rwlock_read_release: slot {slot} held in write mode; call __rwlock_write_release instead")
        }
    }
}

/// `__rwlock_write_acquire(slot: i64) -> Value` — acquire exclusive
/// lock (blocks all readers and other writers), return a clone of the
/// current stored value. Pair with `__rwlock_write_store` + `__rwlock_write_release`.
pub fn builtin_rwlock_write_acquire(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot = slot_id_arg(args, 0, "__rwlock_write_acquire")?;
    let arc = ctx.core.rwlocks.lock().get(&slot).cloned()
        .ok_or_else(|| anyhow!("__rwlock_write_acquire: unknown slot id {slot}"))?;
    // add-concurrency-probes: `profile-contention` feature times contended writes.
    let guard = contended_write(ctx, &arc);
    let cloned = (*guard).clone();
    std::mem::forget(guard);
    HELD_RWLOCK_GUARDS.with(|cell| {
        cell.borrow_mut().insert(slot, RwLockHeld::Write(Arc::clone(&arc)));
    });
    Ok(cloned)
}

/// `__rwlock_write_store(slot: i64, new_val: Value) -> Null` — overwrite
/// the stored Value. Caller must currently hold the lock in *Write* mode
/// on the same thread (this is checked; calling under a Read acquire errs).
pub fn builtin_rwlock_write_store(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot = slot_id_arg(args, 0, "__rwlock_write_store")?;
    let new_val = args.get(1)
        .ok_or_else(|| anyhow!("__rwlock_write_store: missing new value"))?
        .clone();
    let held = HELD_RWLOCK_GUARDS.with(|cell| cell.borrow().get(&slot).cloned())
        .ok_or_else(|| anyhow!("__rwlock_write_store: slot {slot} not currently locked on this thread"))?;
    let arc = match held {
        RwLockHeld::Write(arc) => arc,
        RwLockHeld::Read(_) => bail!("__rwlock_write_store: slot {slot} held in read mode (cannot store via read lock)"),
    };
    // SAFETY: caller currently owns the exclusive write lock on this thread
    // (we acquired via arc.write() + mem::forget; only this thread may
    // release). data_ptr() is the protected slot; writing through it under
    // exclusive ownership matches the parking_lot raw API contract.
    unsafe { *arc.data_ptr() = new_val; }
    Ok(Value::Null)
}

/// `__rwlock_write_release(slot: i64) -> Null` — release a previously
/// acquired exclusive lock. Errors if the slot wasn't held in Write mode.
pub fn builtin_rwlock_write_release(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot = slot_id_arg(args, 0, "__rwlock_write_release")?;
    let held = HELD_RWLOCK_GUARDS.with(|cell| cell.borrow_mut().remove(&slot))
        .ok_or_else(|| anyhow!("__rwlock_write_release: slot {slot} not currently locked on this thread"))?;
    match held {
        RwLockHeld::Write(arc) => {
            unsafe { arc.force_unlock_write(); }
            Ok(Value::Null)
        }
        RwLockHeld::Read(arc) => {
            HELD_RWLOCK_GUARDS.with(|cell| {
                cell.borrow_mut().insert(slot, RwLockHeld::Read(arc));
            });
            bail!("__rwlock_write_release: slot {slot} held in read mode; call __rwlock_read_release instead")
        }
    }
}

/// `__rwlock_try_read(slot: i64) -> Value::Array` — non-blocking shared
/// acquire (add-sync-primitives-try-variants, 2026-05-20). Returns a
/// discriminated array:
///   `[I64(0), value]` — ok, caller now holds read lock (must release)
///   `[I64(1)]`        — contended (writer holds, or same-thread reentrancy)
///
/// Rejects same-thread same-slot reentrancy upfront to keep
/// `HELD_RWLOCK_GUARDS` map invariants simple (single entry per slot
/// per thread → matched by single release call).
pub fn builtin_rwlock_try_read(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    use parking_lot::lock_api::RawRwLock;
    let slot = slot_id_arg(args, 0, "__rwlock_try_read")?;
    let already_held = HELD_RWLOCK_GUARDS.with(|cell| cell.borrow().contains_key(&slot));
    if already_held {
        return Ok(ctx.heap().alloc_array(vec![Value::I64(TRY_LOCK_CONTENDED)]));
    }
    let arc = ctx.core.rwlocks.lock().get(&slot).cloned()
        .ok_or_else(|| anyhow!("__rwlock_try_read: unknown slot id {slot}"))?;
    // SAFETY: bypass the guard type entirely (its lifetime borrows from arc
    // and the borrow checker can't see that mem::forget releases). Raw
    // API matches what __rwlock_read_acquire's force_unlock_read pairs with.
    // SAFETY: parking_lot's .raw() is unsafe because raw lock/unlock skips
    // the type-system guard. We pair it with HELD_RWLOCK_GUARDS bookkeeping
    // + matching force_unlock_read in __rwlock_read_release.
    if !unsafe { arc.raw() }.try_lock_shared() {
        return Ok(ctx.heap().alloc_array(vec![Value::I64(TRY_LOCK_CONTENDED)]));
    }
    // Lock acquired; data_ptr access is safe.
    let cloned = unsafe { (*arc.data_ptr()).clone() };
    HELD_RWLOCK_GUARDS.with(|cell| {
        cell.borrow_mut().insert(slot, RwLockHeld::Read(Arc::clone(&arc)));
    });
    Ok(ctx.heap().alloc_array(vec![Value::I64(TRY_LOCK_OK), cloned]))
}

/// `__rwlock_try_write(slot: i64) -> Value::Array` — non-blocking exclusive
/// acquire. Same discriminator shape as `__rwlock_try_read`. Rejects
/// reentrancy upfront.
pub fn builtin_rwlock_try_write(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    use parking_lot::lock_api::RawRwLock;
    let slot = slot_id_arg(args, 0, "__rwlock_try_write")?;
    let already_held = HELD_RWLOCK_GUARDS.with(|cell| cell.borrow().contains_key(&slot));
    if already_held {
        return Ok(ctx.heap().alloc_array(vec![Value::I64(TRY_LOCK_CONTENDED)]));
    }
    let arc = ctx.core.rwlocks.lock().get(&slot).cloned()
        .ok_or_else(|| anyhow!("__rwlock_try_write: unknown slot id {slot}"))?;
    // SAFETY: see __rwlock_try_read; same raw-API pattern. force_unlock_write
    // (used by __rwlock_write_release) is the matching release.
    // SAFETY: see __rwlock_try_read; matched by force_unlock_write in
    // __rwlock_write_release.
    if !unsafe { arc.raw() }.try_lock_exclusive() {
        return Ok(ctx.heap().alloc_array(vec![Value::I64(TRY_LOCK_CONTENDED)]));
    }
    let cloned = unsafe { (*arc.data_ptr()).clone() };
    HELD_RWLOCK_GUARDS.with(|cell| {
        cell.borrow_mut().insert(slot, RwLockHeld::Write(Arc::clone(&arc)));
    });
    Ok(ctx.heap().alloc_array(vec![Value::I64(TRY_LOCK_OK), cloned]))
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn slot_id_arg(args: &[Value], idx: usize, ctx_name: &str) -> Result<u64> {
    match args.get(idx) {
        Some(Value::I64(n)) if *n >= 0 => Ok(*n as u64),
        Some(other) => bail!("{ctx_name}: arg {idx} expected i64 slot id, got {:?}", other),
        None        => bail!("{ctx_name}: missing arg {idx}"),
    }
}

#[cfg(test)]
#[path = "sync_tests.rs"]
mod sync_tests;
