use super::*;

impl VmContext {
    // ── Native interop (Tier 1, spec C2) ──────────────────────────────────
    //
    // 2026-05-12 add-platform-wasm Stage 0: entire interop API gated on
    // `native-interop` feature. wasm builds drop these methods (and the
    // backing fields) entirely.

    /// Register a native type with this VM. Returns `false` (with [`crate::native::error::set`]
    /// already populated) on duplicate `(module, name)`. Internally invoked
    /// from `z42_register_type`; tests may also call this directly with a
    /// pre-built [`RegisteredType`].
    #[cfg(feature = "native-interop")]
    pub fn register_native_type(
        &self,
        ty: Arc<crate::native::RegisteredType>,
    ) -> bool {
        let key = (ty.module().to_string(), ty.type_name().to_string());
        let mut map = self.core.native_types.write();
        if map.contains_key(&key) {
            return false;
        }
        map.insert(key, ty);
        true
    }

    /// Look up a previously registered native type. Returns `None` when the
    /// `(module, name)` pair is unknown.
    #[cfg(feature = "native-interop")]
    pub fn resolve_native_type(
        &self,
        module: &str,
        name: &str,
    ) -> Option<Arc<crate::native::RegisteredType>> {
        let key = (module.to_string(), name.to_string());
        self.core.native_types.read().get(&key).cloned()
    }

    /// Total number of registered native types — primarily for tests.
    #[cfg(feature = "native-interop")]
    pub fn native_type_count(&self) -> usize {
        self.core.native_types.read().len()
    }

    // ── Pinned owned buffers (spec C10 — Array<u8> pin) ──────────────────

    /// Register an owned byte buffer (the snapshot of an `Array<u8>` taken
    /// during `PinPtr`). Returns the buffer's data pointer for storage in
    /// the `Value::PinnedView`. The buffer remains alive until
    /// [`release_owned_buffer`] is called from a matching `UnpinPtr`.
    pub fn pin_owned_buffer(&self, buf: Box<[u8]>) -> u64 {
        let ptr = buf.as_ptr() as u64;
        self.core.pinned_owned_buffers.lock().insert(ptr, buf);
        ptr
    }

    /// Drop an owned buffer previously registered via [`pin_owned_buffer`].
    /// Idempotent: silently no-ops if `ptr` isn't registered (e.g. `Str`
    /// pins which never enter the table).
    pub fn release_owned_buffer(&self, ptr: u64) {
        let _ = self.core.pinned_owned_buffers.lock().remove(&ptr);
    }

    /// Total number of currently-pinned owned buffers — exposed for
    /// tests asserting that UnpinPtr cleaned up.
    pub fn pinned_owned_buffer_count(&self) -> usize {
        self.core.pinned_owned_buffers.lock().len()
    }

    // ── Pending reflected throw (add-method-invoke-non-generic) ──────────────

    /// Stash a z42 exception value to propagate through a callback builtin's
    /// error channel (see `pending_thrown`). Set immediately before the builtin
    /// returns `Err`; consumed once by `take_pending_thrown`.
    pub fn set_pending_thrown(&self, val: Value) {
        *self.core.pending_thrown.lock() = Some(val);
    }

    /// Take (and clear) a pending thrown exception value, if any. Called by
    /// `exec_call::builtin` in its error handler so the ORIGINAL thrown value
    /// (with its real type) re-enters z42 exception handling.
    pub fn take_pending_thrown(&self) -> Option<Value> {
        self.core.pending_thrown.lock().take()
    }

    /// Load a native library and invoke its `<basename>_register` entry point.
    /// The library handle is stored on `self` until VM drop. Errors are
    /// returned as `anyhow::Error` and mirrored into the thread-local
    /// last-error slot so C callers see the same diagnostic via
    /// [`z42_last_error`](z42_abi::z42_last_error).
    #[cfg(feature = "native-interop")]
    pub fn load_native_library(
        &self,
        path: impl AsRef<std::path::Path>,
    ) -> anyhow::Result<()> {
        crate::native::loader::load_library(self, path.as_ref())
    }
}
