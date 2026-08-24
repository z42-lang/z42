use super::*;

impl VmContext {
    // ── GC heap ───────────────────────────────────────────────────────────

    /// Borrow the GC heap as a trait object. All script-driven allocations go
    /// through this entry point; see `docs/design/runtime/gc.md`.
    pub fn heap(&self) -> &dyn MagrGC {
        self.core.heap.as_ref()
    }

    // ── Static fields ─────────────────────────────────────────────────────
    //
    // Layout (introduce-method-token, 2026-05-08):
    //   `static_fields: Vec<Value>`           — slot storage by StaticFieldId.0
    //   `static_field_index: HashMap<&str, u32>` — name → id (lazy-allocated)
    //
    // The legacy by-name `static_get` / `static_set` API lazy-allocates an
    // ID on first write and looks up by name on read (returning Null if no
    // ID is yet allocated). The dispatch hot path uses `static_get_by_id`
    // / `static_set_by_id` after `metadata::resolver` has populated
    // per-instruction `StaticFieldId` cache slots.

    /// Resolve (or lazy-allocate) a `StaticFieldId` for the given full-qualified
    /// static-field name. Idempotent. Called by `metadata::resolver` at module
    /// load and by hot paths on cache miss (cross-zpkg lazy fields).
    pub fn resolve_static_field_id(&self, name: &str) -> crate::metadata::tokens::StaticFieldId {
        let mut idx = self.core.static_field_index.lock();
        if let Some(&id) = idx.get(name) {
            return crate::metadata::tokens::StaticFieldId(id);
        }
        let id = idx.len() as u32;
        idx.insert(name.to_string(), id);
        // Extend backing Vec to match index.
        let mut sf = self.core.static_fields.lock();
        if (id as usize) >= sf.len() {
            sf.resize_with((id + 1) as usize, || Value::Null);
        }
        crate::metadata::tokens::StaticFieldId(id)
    }

    /// Read a user-class static field by name. Unset fields read as
    /// `Value::Null`. Lazy fallback for cross-zpkg paths and JIT helpers
    /// not yet threading `StaticFieldId`.
    pub fn static_get(&self, field: &str) -> Value {
        let idx = self.core.static_field_index.lock();
        match idx.get(field) {
            Some(&id) => self
                .core
                .static_fields
                .lock()
                .get(id as usize)
                .cloned()
                .unwrap_or(Value::Null),
            None => Value::Null,
        }
    }

    /// Write a user-class static field by name. Lazy-allocates the id on
    /// first write.
    pub fn static_set(&self, field: &str, val: Value) {
        let id = self.resolve_static_field_id(field);
        self.static_set_by_id(id, val);
    }

    /// Hot-path read by id (no hash). Caller must have a resolved id.
    /// Returns `Value::Null` if id ≥ Vec length (unallocated slot).
    #[inline]
    pub fn static_get_by_id(&self, id: crate::metadata::tokens::StaticFieldId) -> Value {
        self.core
            .static_fields
            .lock()
            .get(id.0 as usize)
            .cloned()
            .unwrap_or(Value::Null)
    }

    /// Hot-path write by id. Caller must have a resolved id; the slot
    /// is auto-extended if id ≥ current Vec length.
    #[inline]
    pub fn static_set_by_id(&self, id: crate::metadata::tokens::StaticFieldId, val: Value) {
        let mut sf = self.core.static_fields.lock();
        if (id.0 as usize) >= sf.len() {
            sf.resize_with((id.0 + 1) as usize, || Value::Null);
        }
        sf[id.0 as usize] = val;
    }

    /// Drop all static fields (used by `run_with_static_init` to ensure a
    /// clean slate before each entry-point run). Resets values to Null but
    /// **keeps the index** so previously-allocated `StaticFieldId`s stay
    /// stable across runs (resolver-populated IDs in `Function.resolved`
    /// remain valid after re-init).
    pub fn static_fields_clear(&self) {
        let mut sf = self.core.static_fields.lock();
        for slot in sf.iter_mut() {
            *slot = Value::Null;
        }
    }

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

    /// runtime-jit-tiering Phase 1.5 (mixed-mode): publish/clear the active
    /// `JitModuleCtx` forward pointer (type-erased `usize`). Set by
    /// `JitModule::run_fn` around each entry call; 0 outside it.
    #[inline]
    pub(crate) fn set_jit_ctx(&self, p: usize) {
        self.jit_ctx.store(p, std::sync::atomic::Ordering::Relaxed);
    }
    #[inline]
    pub(crate) fn jit_ctx_ptr(&self) -> usize {
        self.jit_ctx.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Peek at the pending exception without removing it. Used by JIT catch-type
    /// dispatch (catch-by-generic-type, 2026-05-06): the throw helper has set the
    /// exception, the dispatch helper inspects its class to decide which catch
    /// handler to jump to, and a later `take_exception` (via `jit_install_catch`)
    /// hands the value to the chosen catch register.
    pub fn peek_exception(&self) -> Option<Value> {
        self.pending_exception.lock().clone()
    }
}
