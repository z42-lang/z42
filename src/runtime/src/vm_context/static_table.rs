//! 静态字段的存储：每个字段一个**地址稳定**的 16 B 单元，读写不拿锁。
//!
//! 单元就是型参字段的 16 B 单元（`metadata::types::tparam_cell`）：标签字（自描述，acquire 读 /
//! release 写）+ 负载字（基元原始位，relaxed）。静态字段按名字登记、首次用到才分配 id，登记时不知道
//! 它的声明类型，而这个单元能装下任何 `Value`：
//!
//! - 基元 —— 第一次写基元时 CAS 认领种类，之后只改负载字（`static int` / `double` / `bool` / `char`）；
//! - 引用 —— 标签字本身就是 8 B 引用字（`string` / 类 / 数组 / 委托 / 装箱 struct）；
//! - 种类不符的写（`static object o` 先放 `int` 再放 `string`）装箱进标签字，对用户透明。
//!
//! 内存序与实例字段相同：引用 release 写 / acquire 读，基元 relaxed；标签字的每次改写都把被替换的引用
//! 交给 SATB。**不需要 seqlock**：每个静态字段的值都是一个 `Value`，单元协议保证读不撕裂（负载字一生
//! 只装一种基元，读者先 acquire 标签字再读负载字）；值 struct 类型的静态字段由编译器装箱存放
//! （`_boxIfStaticStruct`），单元里只有一个引用字，struct 的各叶子在盒子里各自原子读写。
//!
//! 单元在 [`SegVec`] 里，分段只追加、从不移动 ⇒ `&StaticCell` 在表的生命周期内有效（id 即下标）。
//! 名字 → id 的登记表仍是一把锁，但它只在解析期（每个站点一次）走到。
//!
//! GC：静态字段是根 —— 每次回收（minor / major）都经 [`StaticTable::scan_roots`] 扫一遍标签字，
//! 所以写入不需要分代写屏障；SATB 由单元协议负责。

use std::sync::atomic::{AtomicPtr, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use rustc_hash::FxHashMap;

use crate::metadata::seg_vec::SegVec;
use crate::metadata::tokens::StaticFieldId;
use crate::metadata::types::{tparam_cell, Cells};
use crate::metadata::{TypeDesc, Value};

/// Tag word at byte 0, payload word at byte 8 (`tparam_cell`).
const W0: usize = 0;
const W1: usize = 8;

/// One static field's storage. All-zero = `null`, never written.
///
/// Next to the value: the field's static-constructor owner, as the type-init barrier
/// (`VmContext::ensure_static_owner_init`) resolved it — so a hot static access does not re-derive
/// the owner from the name. Owner state `0` = not looked up, [`OWNER_EXEMPT`] = no owner type or
/// one without a static constructor, [`OWNER_CHECK`] = check `owner` (published before the state,
/// release / acquire; the descriptor is kept alive by [`StaticTable::owners`]).
#[repr(C, align(8))]
#[derive(Default)]
pub(crate) struct StaticCell {
    words: [AtomicU64; 2],
    owner: AtomicPtr<TypeDesc>,
    owner_state: AtomicU8,
}

const OWNER_EXEMPT: u8 = 1;
const OWNER_CHECK: u8 = 2;

/// The cached owner answer of a static field (see [`StaticCell`]).
pub(crate) enum StaticOwner<'a> {
    Unknown,
    Exempt,
    Check(&'a TypeDesc),
}

impl StaticCell {
    #[inline(always)]
    fn cells(&self) -> Cells<'_> {
        // SAFETY: 16 bytes of this cell, 8-aligned, only ever accessed atomically (through this
        // view); the `AtomicU64`s make it interior-mutable.
        unsafe { Cells::from_raw(self.words.as_ptr() as *mut u8, 16) }
    }

    #[inline(always)]
    pub(crate) fn load(&self) -> Value {
        tparam_cell::load(&self.cells(), W0, W1)
    }

    /// Write `v`. Nothing to hand to a write barrier: statics are roots.
    #[inline(always)]
    pub(crate) fn store(&self, v: &Value) -> anyhow::Result<()> {
        tparam_cell::store(&self.cells(), W0, W1, v).map(|_| ())
    }

    /// The cached type-init owner answer (one acquire load; `Check` adds one relaxed load).
    #[inline(always)]
    pub(crate) fn owner(&self) -> StaticOwner<'_> {
        match self.owner_state.load(Ordering::Acquire) {
            OWNER_EXEMPT => StaticOwner::Exempt,
            OWNER_CHECK => {
                // SAFETY: published before `OWNER_CHECK` (release, paired with the acquire above);
                // the pointee is an `Arc` the table keeps in `owners` for its whole life.
                StaticOwner::Check(unsafe { &*self.owner.load(Ordering::Relaxed) })
            }
            _ => StaticOwner::Unknown,
        }
    }

    /// Back to the never-written state (`null`, no kind claimed). The overwritten reference goes
    /// to SATB. Only for a full re-initialisation (`StaticTable::clear`): a reader racing with it
    /// could pair an old kind with a new payload, so nothing may be running.
    fn reset(&self) {
        let old = self.words[0].swap(0, Ordering::AcqRel);
        self.words[1].store(0, Ordering::Relaxed);
        if old & crate::metadata::types::ref_word::KIND_MASK != 0 {
            // SAFETY: `old` was this cell's tag word an instant ago — a live reference word.
            crate::gc::satb::record_overwrite(&unsafe { crate::metadata::types::ref_word::decode_for_trace(old) });
        }
    }
}

/// The VM's static fields: id → cell (lock-free), name → id (registration).
#[derive(Default)]
pub(crate) struct StaticTable {
    cells: SegVec<StaticCell>,
    index: Mutex<FxHashMap<String, u32>>,
    /// Every owner descriptor a cell has pointed at — kept alive (and never dropped before the
    /// table) so a reader holding the pointer an instant before [`Self::forget_owners`] stays valid.
    owners: Mutex<Vec<Arc<TypeDesc>>>,
}

impl StaticTable {
    /// The id of `name`, allocating it (and its cell) on first use. Idempotent.
    pub(crate) fn resolve(&self, name: &str) -> StaticFieldId {
        let mut idx = self.index.lock();
        if let Some(&id) = idx.get(name) {
            return StaticFieldId(id);
        }
        // Ids are dense and allocated under the index lock, so the new cell's index is the id.
        let id = self.cells.push_with(|_| StaticCell::default()).expect("static field table full") as u32;
        idx.insert(name.to_string(), id);
        StaticFieldId(id)
    }

    /// The id of `name` if it has one.
    pub(crate) fn id_of(&self, name: &str) -> Option<StaticFieldId> {
        self.index.lock().get(name).map(|&id| StaticFieldId(id))
    }

    /// The cell of `id` (`None` for an id never allocated).
    #[inline(always)]
    pub(crate) fn cell(&self, id: StaticFieldId) -> Option<&StaticCell> {
        self.cells.get(id.0 as usize)
    }

    /// Read `id`; `null` for an id never allocated.
    #[inline]
    pub(crate) fn get(&self, id: StaticFieldId) -> Value {
        self.cell(id).map_or(Value::Null, StaticCell::load)
    }

    /// Write `id`, growing the table when the id was handed out without a cell (ids only come from
    /// [`Self::resolve`], so that is a defensive path).
    pub(crate) fn set(&self, id: StaticFieldId, v: &Value) -> anyhow::Result<()> {
        match self.cell(id) {
            Some(c) => c.store(v),
            None => self.grown(id).store(v),
        }
    }

    #[cold]
    fn grown(&self, id: StaticFieldId) -> &StaticCell {
        let _idx = self.index.lock(); // allocation stays serialized with `resolve`
        while self.cells.len() <= id.0 as usize {
            self.cells.push_with(|_| StaticCell::default()).expect("static field table full");
        }
        self.cell(id).expect("just grown")
    }

    /// Cache the type-init barrier's owner answer for `id` (`None` = exempt). No-op for an id
    /// without a cell.
    pub(crate) fn set_owner(&self, id: StaticFieldId, owner: Option<Arc<TypeDesc>>) {
        let Some(cell) = self.cell(id) else { return };
        match owner {
            None => cell.owner_state.store(OWNER_EXEMPT, Ordering::Release),
            Some(td) => {
                let mut keep = self.owners.lock();
                cell.owner.store(Arc::as_ptr(&td) as *mut TypeDesc, Ordering::Relaxed);
                cell.owner_state.store(OWNER_CHECK, Ordering::Release);
                keep.push(td);
            }
        }
    }

    /// Drop every cached owner answer (an explicit module (re)load can redefine a type); the next
    /// access re-derives it.
    pub(crate) fn forget_owners(&self) {
        for i in 0..self.cells.len() {
            if let Some(c) = self.cells.get(i) {
                c.owner_state.store(0, Ordering::Release);
            }
        }
    }

    /// Every field back to `null` / never written; ids (and so every resolved site) stay valid.
    pub(crate) fn clear(&self) {
        for i in 0..self.cells.len() {
            if let Some(c) = self.cells.get(i) {
                c.reset();
            }
        }
    }

    /// GC roots: every reference a static field holds (a boxed value yields its box).
    pub(crate) fn scan_roots(&self, visit: &mut dyn FnMut(&Value)) {
        for i in 0..self.cells.len() {
            if let Some(c) = self.cells.get(i) {
                tparam_cell::visit(&c.cells(), W0, visit);
            }
        }
    }

    /// Number of allocated ids (tests).
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.cells.len()
    }
}

#[cfg(test)]
#[path = "static_table_tests.rs"]
mod static_table_tests;
