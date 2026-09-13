//! `Std.Threading` builtins — OS-thread spawn + join.
//!
//! Architecture (add-threading-stdlib, 2026-05-20):
//!
//! - `__thread_spawn(action)` validates the callable, allocates a slot id from
//!   `VmCore.threads` (a `ResourceRegistry`), spawns an `std::thread`, and
//!   stores the `JoinHandle` in it keyed by the slot id. Returns the slot
//!   id as `Value::I64`.
//! - `__thread_join(slot_id)` removes the handle from the registry, joins, and
//!   returns a discriminated result array the z42 facade converts to either a
//!   normal return or a `Std.ThreadException`.
//!
//! The spawned worker constructs `VmContext::new_with_core(Arc::clone(core))`
//! so it shares `static_fields` / `heap` / `lazy_loader` / `native_libs` with
//! the parent thread. Its per-thread state (`pending_exception` / `call_stack`
//! / `func_ref_slots`) is private — the worker is registered in
//! `VmCore.vm_contexts` so the GC scanner walks both threads' roots.
//!
//! Cross-thread error semantics (Decision 5 + 6 in design.md):
//! - z42 `throw` inside the action → `ExecOutcome::Thrown(val)` →
//!   discriminator `1` with formatted message
//! - Rust panic inside the action  → `catch_unwind` Err → discriminator `1`
//!   with `"thread panicked"` message
//! - Already-joined / unknown slot  → discriminator `2`
//!
//! ## Return shape (`__thread_join`)
//!
//! `Value::Array` with leading discriminator:
//!
//! ```text
//! [I64(0)]                — success
//! [I64(1), Str(message)]  — thread action threw / panicked
//! [I64(2)]                — slot unknown (already joined or bogus id)
//! ```

use crate::interp::ExecOutcome;
use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{anyhow, bail, Result};
use std::sync::Arc;

const JOIN_OK:           i64 = 0;
const JOIN_ACTION_ERR:   i64 = 1;
const JOIN_UNKNOWN_SLOT: i64 = 2;

/// `__thread_spawn(action) -> i64` — spawn an OS thread executing `action`.
///
/// `action` must be a callable z42 value (`Value::FuncRef` for zero-capture
/// lambdas, `Value::Closure` for capturing lambdas). `StackClosure` cannot
/// be spawned — its env lives in the calling frame's arena and is freed when
/// the caller returns, which would be a use-after-free on the worker thread.
pub fn builtin_thread_spawn(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let callable = args.first()
        .ok_or_else(|| anyhow!("__thread_spawn: missing action argument"))?;

    let (fn_name, env_vec): (String, Option<Vec<Value>>) = match callable {
        Value::FuncRef(name) => (name.to_string(), None),
        Value::Closure(c) => {
            let data = crate::metadata::types::closure_data_of(c);
            // unify-gc-heap PR-5: fn_name is a GC `Str` (a handle into *this* context's heap);
            // materialize an owned `String` so it can cross to the worker thread safely.
            (data.fn_name.to_string(), Some(data.env.borrow().to_boxed_vec()))
        }
        Value::StackClosure { .. } => bail!(
            "__thread_spawn: stack-allocated closure cannot escape to a worker thread \
             (compiler should have promoted it to a heap Closure for cross-thread use)"
        ),
        other => bail!(
            "__thread_spawn: expected callable (Action / FuncRef / Closure), got {:?}",
            other
        ),
    };

    if ctx.core.module.is_none() {
        bail!(
            "__thread_spawn: VmContext has no shared Module — \
             constructed via VmContext::new() instead of VmContext::with_module()"
        );
    }

    let core_for_thread: Arc<crate::vm_context::VmCore> = Arc::clone(&ctx.core);
    let id = ctx.core.threads.alloc_id();

    // 🔴 **fix-spawn-env-gc-root (2026-09-13)**: root the captured environment *here*, on the
    // spawning thread, while the source closure is still live in the caller's frame.
    //
    // `env_vec` is a plain `Vec<Value>` moved into the worker's Rust closure, and a `Value` is
    // an 8-byte tagged pointer — owning one roots nothing. Between this `spawn` and the point
    // `run_spawned_action` gets the environment into a frame register, those objects are
    // reachable from **no GC root at all**: not from the spawner (`Thread.Start` returns and
    // its frame pops — a `Thread` stores only a slot id, never the action), and not from the
    // worker (it is not in `vm_contexts` until `VmContext::new_with_core`, and a Rust local is
    // not a frame register even after it is). A collection landing in that window reclaims the
    // captures, and the worker then reads recycled memory.
    //
    // Measured with the window widened to 5 ms: a captured `string` comes back as the raw
    // bytes of whatever was re-bumped into its block. Unwidened it is rare but real —
    // `Z42_GC_NURSERY_BYTES=1048576 ./xtask test stdlib z42.net` failed **2 of 10** runs with
    // `BrCond expects bool, got Null` inside a thread-spawning HTTP test, against **0 of 10**
    // on the runtime one commit before #606.
    //
    // The hole is older than #606; what #606 changed is that blocked threads now yield
    // safepoints, so a pool sitting in `Recv`/`Join`/`Sleep` no longer keeps GC from running
    // here at all.
    //
    // The pin — not an `alloc_array` the worker could reach — because it is the environment's
    // *contents* that need rooting, and the array `run_spawned_action` builds lives on the
    // worker's own context. `SpawnedEnvRoot::drop` releases it once the action has returned,
    // which is exactly as long as the environment can be live.
    let env_root = env_vec.as_ref().map(|env| SpawnedEnvRoot {
        core:   Arc::clone(&ctx.core),
        handle: ctx.heap().pin_root(ctx.heap().alloc_array(env.clone())),
    });

    let handle = std::thread::spawn(move || -> Result<()> {
        let _env_root = env_root;
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
            let thread_ctx = VmContext::new_with_core(core_for_thread);
            run_spawned_action(&thread_ctx, &fn_name, env_vec)
        }));
        match unwind {
            Ok(r)    => r,
            Err(_)   => Err(anyhow!("thread panicked (Rust panic; not user throw)")),
        }
    });

    ctx.core.threads.lock().insert(id, handle);
    Ok(Value::I64(id as i64))
}

/// **fix-spawn-env-gc-root (2026-09-13)**: keeps a spawned action's captured environment
/// pinned as a GC root for the worker thread's whole life, and unpins it when the worker's
/// Rust closure unwinds — including on a panic, which is why this is a `Drop` guard and not an
/// `unpin_root` call at the end of the closure.
///
/// Holding it for the thread's whole life rather than "until the environment reaches a frame"
/// is deliberate: the environment *is* live for exactly that long, the handle is one entry in
/// the heap's root map, and the alternative needs a handshake with the worker that buys
/// nothing.
struct SpawnedEnvRoot {
    core:   Arc<crate::vm_context::VmCore>,
    handle: crate::gc::RootHandle,
}

impl Drop for SpawnedEnvRoot {
    fn drop(&mut self) {
        self.core.heap.unpin_root(self.handle);
    }
}

/// `__thread_sleep(millis: i64)` — block the current thread for the given
/// duration. add-thread-sleep (2026-05-27). Negative values saturate to 0
/// (matches BCL `Thread.Sleep`). Backed by `std::thread::sleep` (POSIX
/// `nanosleep`); ms precision is sufficient for the scripting use case.
pub fn builtin_thread_sleep(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let millis = match args.first() {
        Some(Value::I64(n)) => *n,
        Some(other) => bail!("__thread_sleep: expected i64 millis, got {:?}", other),
        None        => bail!("__thread_sleep: missing millis argument"),
    };
    let clamped = if millis < 0 { 0u64 } else { millis as u64 };
    // fix-sync-primitives-gc-park：睡眠期间到不了 safepoint ⇒ 并发 GC 的停顿被拉长到**整个睡眠
    // 时长**（`Thread.Sleep(60000)` 就是卡 GC 一分钟；poll 循环里则是持续拖累）。
    // 与 recv/join 不同，这条不是永久死锁，但同属「阻塞期间必须让出 safepoint」。
    {
        let _park = crate::gc::NativeParkGuard::enter(ctx);
        std::thread::sleep(std::time::Duration::from_millis(clamped));
    }
    Ok(Value::Null)
}

/// `__thread_join(slot_id) -> Value::Array` — wait for the spawned thread and
/// return a discriminated outcome (see module-level docs for the shape).
pub fn builtin_thread_join(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let slot_id: u64 = match args.first() {
        Some(Value::I64(n)) if *n >= 0 => *n as u64,
        Some(other) => bail!("__thread_join: expected i64 slot id, got {:?}", other),
        None        => bail!("__thread_join: missing slot id"),
    };

    let handle = match ctx.core.threads.lock().remove(&slot_id) {
        Some(h) => h,
        None    => return Ok(unknown_slot_result(ctx)),
    };

    // fix-blocking-native-calls-round2：阻塞期间必须让出 GC safepoint，否则并发 GC 死锁
    // （同 #598 的网络七处；机制见 gc/safepoint.rs 的 NativeParkGuard）。
    // ⚠️ 这条是最危险的一个：`t.Join()` 是 z42 多线程的主干写法。被 join 的线程只要在结束前
    // 触发 GC，join 方卡在 handle.join()（不在安全点）⇒ GC 等不到它 ⇒ 被 join 的线程也结束不了。
    let joined = { let _park = crate::gc::NativeParkGuard::enter(ctx); handle.join() };
    match joined {
        Ok(Ok(()))     => Ok(ok_result(ctx)),
        Ok(Err(e))     => Ok(action_err_result(ctx, &format!("{e}"))),
        Err(_panic)    => Ok(action_err_result(ctx, "thread panicked")),
    }
}

// ── internal helpers ─────────────────────────────────────────────────────────

fn run_spawned_action(
    thread_ctx: &VmContext,
    fn_name:    &str,
    env_vec:    Option<Vec<Value>>,
) -> Result<()> {
    let module_arc = thread_ctx.core.module.as_ref()
        .ok_or_else(|| anyhow!("__thread_spawn worker: VmCore.module is None"))?
        .clone();
    let module = module_arc.as_ref();

    let arg_vals: Vec<Value> = match env_vec {
        None => Vec::new(),
        Some(env) => {
            let env_val = thread_ctx.heap().alloc_array(env);
            vec![env_val]
        }
    };

    let outcome = match module.func_index.get(fn_name) {
        Some(&idx) => crate::interp::exec_function(
            thread_ctx, module, &module.functions[idx], &arg_vals,
        )?,
        None => {
            let lazy_fn = thread_ctx.try_lookup_function(fn_name)
                .ok_or_else(|| anyhow!(
                    "spawned action: function `{}` not found in module or lazy loader",
                    fn_name
                ))?;
            crate::interp::exec_function(thread_ctx, module, lazy_fn.as_ref(), &arg_vals)?
        }
    };

    match outcome {
        ExecOutcome::Returned(_) => Ok(()),
        ExecOutcome::Thrown(val) => {
            // Prefer the Exception.Message field so the user-visible error
            // text matches what `throw new ...Exception("msg")` set. Fall
            // back to value_to_str for non-Exception thrown values (rare —
            // z42 type-checker normally requires Exception subclasses).
            let msg = crate::exception::read_message(&val, module)
                .unwrap_or_else(|| crate::corelib::convert::value_to_str(&val));
            bail!("{msg}")
        }
    }
}

fn ok_result(ctx: &VmContext) -> Value {
    ctx.heap().alloc_array(vec![Value::I64(JOIN_OK)])
}

fn action_err_result(ctx: &VmContext, msg: &str) -> Value {
    ctx.heap().alloc_array(vec![
        Value::I64(JOIN_ACTION_ERR),
        Value::Str(msg.to_string().into()),
    ])
}

fn unknown_slot_result(ctx: &VmContext) -> Value {
    ctx.heap().alloc_array(vec![Value::I64(JOIN_UNKNOWN_SLOT)])
}

#[cfg(test)]
#[path = "threading_tests.rs"]
mod threading_tests;
