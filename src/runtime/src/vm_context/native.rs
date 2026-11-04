use super::*;

impl VmContext {
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
