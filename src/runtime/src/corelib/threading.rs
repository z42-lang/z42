//! `Std.Threading` builtins — OS-thread spawn + join.
//!
//! Architecture (add-threading-stdlib, 2026-05-20):
//!
//! - `__thread_spawn(action)` validates the callable, allocates a slot id from
//!   `VmCore.next_thread_id`, spawns an `std::thread`, and stores the
//!   `JoinHandle` in `VmCore.threads` keyed by the slot id. Returns the slot
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
use std::sync::atomic::Ordering;

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
            (data.fn_name.clone(), Some(data.env.borrow().to_boxed_vec()))
        }
        Value::StackClosure(_) => bail!(
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
    let id = ctx.core.next_thread_id.fetch_add(1, Ordering::Relaxed);

    let handle = std::thread::spawn(move || -> Result<()> {
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

/// `__thread_sleep(millis: i64)` — block the current thread for the given
/// duration. add-thread-sleep (2026-05-27). Negative values saturate to 0
/// (matches BCL `Thread.Sleep`). Backed by `std::thread::sleep` (POSIX
/// `nanosleep`); ms precision is sufficient for the scripting use case.
pub fn builtin_thread_sleep(_ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let millis = match args.first() {
        Some(Value::I64(n)) => *n,
        Some(other) => bail!("__thread_sleep: expected i64 millis, got {:?}", other),
        None        => bail!("__thread_sleep: missing millis argument"),
    };
    let clamped = if millis < 0 { 0u64 } else { millis as u64 };
    std::thread::sleep(std::time::Duration::from_millis(clamped));
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

    match handle.join() {
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
