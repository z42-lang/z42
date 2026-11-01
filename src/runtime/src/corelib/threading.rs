//! `Std.Threading` builtins — OS-thread spawn + join.
//!
//! Architecture (add-threading-stdlib):
//!
//! - `__thread_spawn(action)` validates the callable, allocates a slot id from
//!   `VmCore.threads` (a `ResourceRegistry`), spawns an `std::thread`, and
//!   stores the `JoinHandle` in it keyed by the slot id. Returns the slot
//!   id as `Value::I64`.
//! - `__thread_wait(slot_id)` removes the handle from the registry, joins (parked),
//!   and throws `Std.Threading.ThreadException` when the worker threw / panicked or
//!   the slot is unknown.
//!
//! The spawned worker constructs `VmContext::new_with_core(Arc::clone(core))`
//! so it shares `static_fields` / `heap` / `lazy_loader` / `native_libs` with
//! the parent thread. Its per-thread state (`pending_exception` / `call_stack`)
//! is private — the worker is registered in
//! `VmCore.vm_contexts` so the GC scanner walks both threads' roots.

use crate::interp::ExecOutcome;
use crate::object::Value;
use crate::vm_context::VmContext;
use anyhow::{anyhow, bail, Result};
use std::sync::Arc;

/// `__thread_spawn(action)` — spawn an OS thread executing `action`.
///
/// `action` must be a callable z42 value (`Value::FuncRef` for zero-capture
/// lambdas, `Value::Closure` for capturing lambdas).
pub fn thread_spawn(ctx: &VmContext, action: &Value) -> Result<i64, crate::corelib::abi::BuiltinError> {
    // P1-2 PR 8: the target is the value's bound `FnId` — a plain integer that crosses to
    // the worker as is (the worker shares this VM's `FuncTable`).
    let (fn_id, env_vec): (crate::metadata::tokens::FnId, Option<Vec<Value>>) = match action {
        Value::FuncRef(id) => (*id, None),
        Value::Closure(c) => {
            let data = crate::object::closure_data_of(c);
            (data.fn_id, Some(data.env.borrow().to_boxed_vec()))
        }
        // The stdlib passes `object action`; anything but a delegate value is the
        // caller's argument (a user value), so a typed argument error, not an internal one.
        other => return Err(crate::corelib::abi::BuiltinError::Argument(format!(
            "__thread_spawn: expected callable (Action / FuncRef / Closure), got {:?}",
            other
        ))),
    };

    if ctx.core.module.is_none() {
        return Err(crate::corelib::abi::BuiltinError::Internal(
            "__thread_spawn: VmContext has no shared Module — \
             constructed via VmContext::new() instead of VmContext::with_module()".into()));
    }

    let core_for_thread: Arc<crate::vm_context::VmCore> = Arc::clone(&ctx.core);
    let id = ctx.core.threads.alloc_id();

    // 🔴 **fix-spawn-env-gc-root **: root the captured environment *here*, on the
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
        handle: ctx.heap().pin_root(crate::object::ArrayObj::alloc_frozen(ctx.heap(), &env)),
    });

    // runtime-audit P0-4: z42 calls recurse on the native stack and running out
    // is fatal, so worker threads get the configured size, not std's 2 MiB.
    let stack = crate::config::runtime_config().vm_thread_stack_size();
    let handle = std::thread::Builder::new().stack_size(stack).spawn(move || -> Result<()> {
        let _env_root = env_root;
        // **测试钩子**：`Z42_SPAWN_ENV_DELAY_MS` 把上面那个窗口人为撑大，
        // 让 `gc_spawn_env_root` 这条竞态在门禁里变成确定性用例。
        //
        // 没有它抓不住：真实窗口是「几微秒的线程启动」。本机实测，撑到 5 ms 时
        // 19 200 次 spawn 才出 10 次误回收（命中率 0.05%），而一个门禁用例只 spawn 上百次
        // —— 也就是说不带钩子的回归测试**在未修复的运行时上照样全绿**（验证过），
        // 那种测试没有价值。撑到 60 ms 就每次必红。
        // 每次 spawn 读一次 `var`，而这条路径本来就要建一个 OS 线程，代价可忽略。
        if let Some(ms) = std::env::var("Z42_SPAWN_ENV_DELAY_MS")
            .ok().and_then(|v| v.parse::<u64>().ok())
        {
            std::thread::sleep(std::time::Duration::from_millis(ms));
        }
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
            let thread_ctx = VmContext::new_with_core(core_for_thread);
            run_spawned_action(&thread_ctx, fn_id, env_vec)
        }));
        match unwind {
            Ok(r)    => r,
            Err(_)   => Err(anyhow!("thread panicked (Rust panic; not user throw)")),
        }
    }).map_err(|e| crate::corelib::abi::BuiltinError::Exception {
        class: "Std.Threading.ThreadException",
        message: format!("__thread_spawn: cannot create thread: {e}"),
    })?;

    ctx.core.threads.lock().insert(id, handle);
    Ok(id as i64)
}

/// **fix-spawn-env-gc-root **: keeps a spawned action's captured environment
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

/// The blocking thread builtins (builtin ABI v2): their waits run with the context
/// parked, so a collection another thread starts meanwhile does not wait for them.
#[z42_macros::builtins]
pub mod typed {
    use crate::corelib::abi::{Blocking, BuiltinError};
    use crate::vm_context::VmContext;

    /// `__thread_sleep(millis)` — block the current thread. Negative values saturate to
    /// 0 (BCL `Thread.Sleep`). The whole body runs parked (`blocking`): a sleeping thread
    /// cannot hold up a collection for the length of its sleep (`Thread.Sleep(60000)`
    /// would otherwise stall every other thread's GC for a minute).
    #[builtin(name = "__thread_sleep", blocking)]
    pub fn thread_sleep(millis: i64) {
        std::thread::sleep(std::time::Duration::from_millis(millis.max(0) as u64));
    }

    /// `__thread_spawn(action)` — OS 线程执行 `action`,返回句柄槽号。线程创建失败 →
    /// `Std.Threading.ThreadException`(与 `__thread_wait` 的失败同类);非 callable 实参 →
    /// `ArgumentException`。spawn 本身不等(worker 跑在别的线程上,不占调用线程)。
    #[builtin(name = "__thread_spawn")]
    pub fn thread_spawn(ctx: &VmContext, action: &crate::object::Value) -> Result<i64, BuiltinError> {
        super::thread_spawn(ctx, action)
    }

    /// `__thread_wait(slot)` — `Thread.Join()`: wait for a spawned thread (parked).
    /// The outcome throws instead of coming back as an array: the
    /// worker threw or panicked, or the thread was already joined →
    /// `Std.Threading.ThreadException` (its message as before). .NET's `Join()` has no
    /// failure of its own (an unhandled worker exception ends the process there), so the
    /// z42 class stays; there is no timed join, hence no `TimeoutException`.
    #[builtin(name = "__thread_wait")]
    pub fn thread_wait(ctx: &VmContext, slot: i64) -> Blocking<()> {
        let handle = ctx.core.threads.lock().remove(&(slot as u64));
        let fail = |message: String| crate::corelib::abi::BuiltinError::Exception {
            class: "Std.Threading.ThreadException", message,
        };
        Blocking::new(move || match handle {
            None => Err(fail("thread already joined".to_string())),
            Some(h) => match h.join() {
                Ok(Ok(()))  => Ok(()),
                Ok(Err(e))  => Err(fail(format!("{e}"))),
                Err(_panic) => Err(fail("thread panicked".to_string())),
            },
        })
    }

}

// ── internal helpers ─────────────────────────────────────────────────────────

fn run_spawned_action(
    thread_ctx: &VmContext,
    fn_id:      crate::metadata::tokens::FnId,
    env_vec:    Option<Vec<Value>>,
) -> Result<()> {
    let module_arc = thread_ctx.core.module.as_ref()
        .ok_or_else(|| anyhow!("__thread_spawn worker: VmCore.module is None"))?
        .clone();
    let module = module_arc.as_ref();

    let arg_vals: Vec<Value> = match env_vec {
        None => Vec::new(),
        Some(env) => {
            let env_val = crate::object::ArrayObj::alloc_frozen(thread_ctx.heap(), &env);
            vec![env_val]
        }
    };

    let target = thread_ctx.callable_fn(fn_id).ok_or_else(|| anyhow!(
        "spawned action: function id {} is not registered in this VM", fn_id.0
    ))?;
    let outcome = crate::interp::exec_function(thread_ctx, module, target, &arg_vals)?;

    match outcome {
        ExecOutcome::Returned(_) => Ok(()),
        ExecOutcome::Thrown(val) => {
            // Prefer the Exception.Message field so the user-visible error
            // text matches what `throw new ...Exception("msg")` set. Fall
            // back to value_to_str for non-Exception thrown values (rare —
            // z42 type-checker normally requires Exception subclasses).
            let msg = crate::exception::read_message(&val, thread_ctx, module)
                .unwrap_or_else(|| crate::corelib::convert::value_to_str(&val));
            bail!("{msg}")
        }
    }
}

#[cfg(test)]
#[path = "threading_tests.rs"]
mod threading_tests;
