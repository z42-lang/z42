use super::*;

impl VmContext {
    // ── GC heap ───────────────────────────────────────────────────────────

    /// Borrow the GC heap as a trait object. All script-driven allocations go
    /// through this entry point; see `docs/internals/src/runtime/gc.md`.
    pub fn heap(&self) -> &dyn MagrGC {
        self.core.heap.as_ref()
    }

    // ── Static fields ─────────────────────────────────────────────────────
    //
    // Storage is `core.statics` (`static_table`): one stable-address cell per
    // `StaticFieldId`, read and written without a lock; the name → id registration is
    // lazy. The dispatch hot path uses `static_get_by_id` / `static_set_by_id` with the
    // per-site id `metadata::resolver` resolved; the by-name `static_get` / `static_set`
    // serve sites without one (and the REPL).

    /// Resolve (or lazy-allocate) a `StaticFieldId` for the given full-qualified
    /// static-field name. Idempotent. Called by `metadata::resolver` at module
    /// load and by hot paths on cache miss (cross-zpkg lazy fields).
    pub fn resolve_static_field_id(&self, name: &str) -> crate::metadata::tokens::StaticFieldId {
        self.core.statics.resolve(name)
    }

    /// Read a user-class static field by name. Unset fields read as `Value::Null`.
    pub fn static_get(&self, field: &str) -> Value {
        self.core.statics.id_of(field).map_or(Value::Null, |id| self.core.statics.get(id))
    }

    /// Write a user-class static field by name. Lazy-allocates the id on first write.
    pub fn static_set(&self, field: &str, val: Value) -> anyhow::Result<()> {
        let id = self.resolve_static_field_id(field);
        self.static_set_by_id(id, val)
    }

    /// Hot-path read by id: one acquire load of the cell's tag word (and a relaxed load of
    /// its payload for a primitive). `Value::Null` for an id never allocated.
    #[inline]
    pub fn static_get_by_id(&self, id: crate::metadata::tokens::StaticFieldId) -> Value {
        self.core.statics.get(id)
    }

    /// Hot-path write by id (no lock). A value of another kind than the cell already holds is
    /// boxed into it (`tparam_cell`), which needs the thread's active heap — the only way this
    /// fails (a context with no frame running).
    #[inline]
    pub fn static_set_by_id(&self, id: crate::metadata::tokens::StaticFieldId, val: Value) -> anyhow::Result<()> {
        self.core.statics.set(id, &val)
    }

    /// **The static-field barrier** — package initializers, then the owner type's static constructor —
    /// for an access to `field` (`field_id` = its resolved id, `None` = a site without one). The one
    /// implementation: the interpreter and the JIT helper call it, and JIT-compiled code inlines its
    /// first line (`jit::translate::static_field`).
    ///
    /// Steady state: the field's cell records the barrier epoch at which the barrier was last found
    /// a no-op (`StaticCell::barrier_ready`) — one relaxed load of the epoch plus one acquire load of
    /// the cell. Any event that can make a passing barrier act again (a static constructor or package
    /// initializer registered, a package initializer failing, a re-run generation, an explicit module
    /// (re)load) moves the epoch on, and the next access re-runs the full barrier.
    #[inline]
    pub fn ensure_static_field_init(&self, field: &str, field_id: Option<u32>) -> Result<(), String> {
        let cell = field_id.and_then(|id| self.core.statics.cell(crate::metadata::tokens::StaticFieldId(id)));
        if let Some(c) = cell {
            if c.barrier_ready(self.core.cctors.barrier_epoch()) {
                return Ok(());
            }
        }
        self.ensure_static_field_init_slow(field, field_id, cell)
    }

    #[inline(never)]
    fn ensure_static_field_init_slow(
        &self, field: &str, field_id: Option<u32>, cell: Option<&super::static_table::StaticCell>,
    ) -> Result<(), String> {
        // Read before the barrier runs (acquire: an invalidating event published before this epoch
        // is visible below), so an event while it runs leaves the record stale instead of wrong.
        let epoch = self.core.cctors.barrier_epoch_acquire();
        // Reading / writing a cross-package static may have just loaded that package: package
        // initializers run before the type initializer.
        self.ensure_module_inits(Some(field))?;
        self.ensure_static_owner_init(field, field_id)?;
        if let Some(c) = cell {
            if self.static_barrier_settled(c) {
                c.mark_barrier_ready(epoch);
            }
        }
        Ok(())
    }

    /// The barrier just passed for `cell`'s field and keeps passing, on every thread, until the epoch
    /// moves: no package initializer left to run (one running on this thread re-entrantly passes but
    /// is not settled), and no static constructor pending at all, or the owner is exempt, or its
    /// initializer is done in the current generation (a re-entrant one passes but is not done).
    fn static_barrier_settled(&self, cell: &super::static_table::StaticCell) -> bool {
        use super::static_table::StaticOwner;
        let cc = &self.core.cctors;
        if cc.any_module_init_pending() {
            return false;
        }
        if !cc.any_pending() {
            return true;
        }
        match cell.owner() {
            StaticOwner::Exempt => true,
            StaticOwner::Check(td) => td.init_done_in(cc.generation()),
            StaticOwner::Unknown => false,
        }
    }

    /// Drop every cached static-field owner answer and barrier verdict (an explicit module (re)load
    /// can redefine a type).
    pub(crate) fn forget_static_owners(&self) {
        self.core.statics.forget_owners();
        self.core.cctors.invalidate_barriers();
    }

    /// What JIT-compiled code bakes for an access to static field `id`: the cell's address (stable —
    /// `SegVec` never moves a cell) and the barrier epoch's. `None` for an id without a cell.
    #[cfg(feature = "jit")]
    pub(crate) fn jit_static_cell(
        &self, id: u32,
    ) -> Option<(*const super::static_table::StaticCell, *const std::sync::atomic::AtomicU32)> {
        let cell = self.core.statics.cell(crate::metadata::tokens::StaticFieldId(id))?;
        Some((cell as *const _, self.core.cctors.barrier_epoch_ptr()))
    }

    /// Drop all static fields (used by `run_with_static_init` to ensure a
    /// clean slate before each entry-point run). Resets values to Null but
    /// **keeps the index** so previously-allocated `StaticFieldId`s stay
    /// stable across runs (resolver-populated IDs in `Function.resolved`
    /// remain valid after re-init).
    pub fn static_fields_clear(&self) {
        self.core.statics.clear();
        // 槽位清零后，**已跑过的初始化器必须重跑** —— 否则那些类型已是终态、不会再被
        // 任何访问重新触发，静态字段永远停在 Null/零值。
        //
        // 历史：defer-class-initialization 时期这里把已跑过的 `__static_init__` 名字倒回
        // 待跑队列（实测诱因：z42b 先加载依赖跑完 43 个初始化器，随后为目标模块调
        // `init_static_fields` → 清零 → `Sha256._roundConstants` 变 Null，崩在
        // `Sha256._processBlock`）。
        //
        // unify-static-init-into-cctor：静态字段初始化器已全部并入**每类型的类型初始化器**，
        // 那条队列随 7.4 删除；重跑改由 `reset_for_rerun()` 统一负责 —— 递增代际使所有
        // `TypeDesc` 上的快路标记一次性失效，比逐个去清散落各处的标记可靠
        // （漏一个就是静默错值：单模块 zbc 里 `class C { static int X = 7; }` 读到 0）。
        self.core.cctors.reset_for_rerun();
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

    /// The VM's JIT runtime, once `jit::run` installed it (`None` in interp
    /// mode). Every thread of the VM sees the same one; the interpreter's
    /// mixed-mode diverts route already-hot callees to native code through it.
    /// One relaxed load of this context's cached pointer (`JitHead::jit`); a
    /// context that predates the install picks it up at its next bottom frame push.
    #[cfg(feature = "jit")]
    #[inline]
    pub(crate) fn jit(&self) -> Option<&crate::jit::runtime::JitRuntime> {
        let p = self.jit_head.jit.load(std::sync::atomic::Ordering::Relaxed);
        // SAFETY: non-null only once it points into `self.core.jit`, which the
        // core — kept alive by this context — never clears.
        if p.is_null() { None } else { Some(unsafe { &*p }) }
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
