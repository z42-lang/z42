//! Static storage and generation invalidation, independent of execution.
use crate::object::Value;
use super::{StaticTable, CctorRegistry};
pub struct StaticFields<'a> { table: &'a StaticTable, cctors: &'a CctorRegistry }
impl<'a> StaticFields<'a> {
    #[inline]
    pub fn new(table: &'a StaticTable, cctors: &'a CctorRegistry) -> Self { Self { table, cctors } }
}


impl StaticFields<'_> {
    // ── Static fields ─────────────────────────────────────────────────────
    //
    // Storage is `core.runtime.statics` (`static_table`): one stable-address cell per
    // `StaticFieldId`, read and written without a lock; the name → id registration is
    // lazy. The dispatch hot path uses `get` / `set` with the
    // per-site id `metadata::resolve` resolved; the by-name `get_named` / `set_named`
    // serve sites without one (and the REPL).

    /// Resolve (or lazy-allocate) a `StaticFieldId` for the given full-qualified
    /// static-field name. Idempotent. Called by `metadata::resolve` at module
    /// load and by hot paths on cache miss (cross-zpkg lazy fields).
    pub fn resolve(&self, name: &str) -> crate::metadata::ids::tokens::StaticFieldId {
        self.table.resolve(name)
    }

    /// Read a user-class static field by name. Unset fields read as `Value::Null`.
    pub fn get_named(&self, field: &str) -> Value {
        self.table.id_of(field).map_or(Value::Null, |id| self.table.get(id))
    }

    /// Write a user-class static field by name. Lazy-allocates the id on first write.
    pub fn set_named(&self, field: &str, val: Value) -> anyhow::Result<()> {
        let id = self.resolve(field);
        self.set(id, val)
    }

    /// Hot-path read by id: one acquire load of the cell's tag word (and a relaxed load of
    /// its payload for a primitive). `Value::Null` for an id never allocated.
    #[inline]
    pub fn get(&self, id: crate::metadata::ids::tokens::StaticFieldId) -> Value {
        self.table.get(id)
    }

    /// Hot-path write by id (no lock). A value of another kind than the cell already holds is
    /// boxed into it (`tparam_cell`), which needs the thread's active heap — the only way this
    /// fails (a context with no frame running).
    #[inline]
    pub fn set(&self, id: crate::metadata::ids::tokens::StaticFieldId, val: Value) -> anyhow::Result<()> {
        self.table.set(id, &val)
    }

    /// Drop every cached static-field owner answer and barrier verdict (an explicit module (re)load
    /// can redefine a type).
    pub(crate) fn forget_owners(&self) {
        self.table.forget_owners();
        self.cctors.invalidate_barriers();
    }

    /// What JIT-compiled code bakes for an access to static field `id`: the cell's address (stable —
    /// `SegVec` never moves a cell) and the barrier epoch's. `None` for an id without a cell.
    #[cfg(feature = "jit")]
    pub fn jit_cell(
        &self, id: u32,
    ) -> Option<(*const crate::metadata::statics::StaticCell, *const std::sync::atomic::AtomicU32)> {
        let cell = self.table.cell(crate::metadata::ids::tokens::StaticFieldId(id))?;
        Some((cell as *const _, self.cctors.barrier_epoch_ptr()))
    }

    /// Lazy default-initialisation of a declared value-type static field read as never written:
    /// install `zero` unless something was written meanwhile, and return the field's value
    /// (`StaticTable::init_default`). `field_id` `None` = by name (allocating the id).
    pub fn init_default(
        &self, field: &str, field_id: Option<crate::metadata::ids::tokens::StaticFieldId>, zero: &Value,
    ) -> anyhow::Result<Value> {
        let id = field_id.unwrap_or_else(|| self.resolve(field));
        self.table.init_default(id, zero)
    }

    /// Drop all static fields (used by `run_with_static_init` to ensure a
    /// clean slate before each entry-point run). Resets values to Null but
    /// **keeps the index** so previously-allocated `StaticFieldId`s stay
    /// stable across runs (resolver-populated IDs in `Function.resolved`
    /// remain valid after re-init).
    pub fn clear_for_rerun(&self) {
        self.table.clear();
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
        self.cctors.reset_for_rerun();
    }

}
