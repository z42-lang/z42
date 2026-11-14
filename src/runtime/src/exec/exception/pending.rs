//! Thread-owned managed exception handoff across execution and native boundaries.
use crate::{exec::ThreadExecState, object::Value};

impl ThreadExecState {
    // ── JIT exception bridge ──────────────────────────────────────────────

    /// JIT helpers store a thrown user value here; the JIT entry sees the
    /// `extern "C"` return code = 1 and pulls the value via
    /// `take_exception()` to propagate as `ExecOutcome::Thrown`.
    pub fn set_exception(&self, val: Value) {
        *self.pending_exception.lock() = Some(val);
    }

    /// Pop the pending exception (called once per `extern "C"` failure).
    pub fn take_exception(&self) -> Option<Value> {
        self.pending_exception.lock().take()
    }

    /// Peek at the pending exception without removing it. Used by JIT catch-type
    /// dispatch (catch-by-generic-type): the throw helper has set the
    /// exception, the dispatch helper inspects its class to decide which catch
    /// handler to jump to, and a later `take_exception` (via `jit_install_catch`)
    /// hands the value to the chosen catch register.
    pub fn peek_exception(&self) -> Option<Value> {
        self.pending_exception.lock().clone()
    }
    // ── Pending reflected throw (add-method-invoke-non-generic) ──────────────

    /// Stash a z42 exception value to propagate through a callback builtin's
    /// error channel (see `pending_thrown`). Set immediately before the builtin
    /// returns `Err`; consumed once by `take_pending_thrown`.
    pub fn set_pending_thrown(&self, val: Value) {
        *self.pending_thrown.lock() = Some(val);
    }

    /// Take (and clear) a pending thrown exception value, if any. Called by
    /// `exec_call::builtin` in its error handler so the ORIGINAL thrown value
    /// (with its real type) re-enters z42 exception handling.
    pub fn take_pending_thrown(&self) -> Option<Value> {
        self.pending_thrown.lock().take()
    }

}

#[cfg(all(test, z42_test_facade))]
#[path = "pending_tests.rs"]
mod tests;
