//! `GcRef<T>` —— GC-managed heap reference (opaque handle).
//!
//! # 设计意图
//!
//! `GcRef<T>` 是 `Value::Object` / `Value::Array` 等堆引用类型的**不透明句柄**，
//! 隐藏内部 backing 实现。
//!
//! # 当前 backing（add-custom-allocator P1，2026-05-22）
//!
//! `NonNull<RegionEntry<T>>` —— 直接指向 [`Region<T>`](super::region::Region)
//! 内部的 `RegionEntry<T>`。Region 由 `ArcMagrGC` 拥有，chunks 是 `Box`
//! 单位故 entry 地址在 region 生命周期内稳定。
//!
//! ## Phase 历史
//! - Phase 1（旧）：`Rc<GcAllocation<T>>` + `RefCell<T>`（单线程语义）
//! - Phase 3（旧）：`Arc<GcAllocation<T>>` + `Mutex<T>`（add-multithreading-foundation）
//! - P1（旧）：`NonNull<RegionEntry<T>>`(8B) + `generation: u32`(4B) = 16B
//! - **PR-3 当前**：单个 8B **标记指针**（`unify-object-byte-layout` 路 A）——
//!   低 48 位是 RegionEntry 地址，高 16 位是窄 generation 快照。见下 `GcRef` 类型文档。
//!
//! 关键改动 vs Arc：
//! - **`Clone` 无 atomic op**：handle 是 8B Copy 原语（标记指针），
//!   memcpy 一次完成；之前每次 `Arc::clone` 一次 `fetch_add`（2-4 ns）
//! - **`Drop` 是 no-op**：不再 refcount decrement。Finalizer 在 sweep 触发，
//!   不在 scope 退出触发。配 `Std.GC.Finalize(x)` 显式 API（P2 加）做 RAII
//!   资源回收
//! - **生命周期契约**：`GcRef` 不能 outlive 拥有它所指 `Region` 的 `ArcMagrGC`。
//!   z42 现有架构所有 GcRef 都生活在 VmContext 范围内，契约天然满足
//!
//! # API 契约
//!
//! - `clone()` 是 cheap operation（12 字节 memcpy；零 atomic op）
//! - `borrow()` / `borrow_mut()` 走 RegionEntry 内置 `Mutex<T>`，
//!   **blocking lock**（同 Phase 3 / mutator contention 行为）
//! - `ptr_eq(a, b)` 比较 NonNull 是否指向同一 RegionEntry
//! - `as_ptr(this)` 返回 RegionEntry 内 `value: Mutex<T>` 的稳定地址
//!   （身份哈希 / dedup key；契约：地址在 entry 生命周期内不变）
//! - `mark` / `is_marked` / `clear_mark`：操作 RegionEntry 的 `marked` 字段
//! - `downgrade(this)` 创建 weak handle（同 NonNull + generation；upgrade
//!   走 `alive + generation` 一致性检查）
//! - **`borrow` 在 generation 不匹配时 panic**（detect use-after-finalize）

use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::atomic::Ordering;

use parking_lot::{Mutex, MutexGuard};

use super::region::RegionEntry;
use super::types::FinalizerFn;

/// `Ref<'a, T>` —— immutable borrow guard alias.
/// 等价于 `MutexGuard<'a, T>`（parking_lot Mutex 无 read/write 区分）。
pub type Ref<'a, T> = MutexGuard<'a, T>;

/// `RefMut<'a, T>` —— mutable borrow guard alias. 与 `Ref` 同。
pub type RefMut<'a, T> = MutexGuard<'a, T>;

/// Number of low bits of a 64-bit pointer that hold the actual virtual
/// address; the top `64 - ADDR_BITS` bits are free for tagging on
/// x86-64 (48-bit canonical VA, user-space high bits = 0) and AArch64
/// (48-bit VA / 47-bit on Apple Silicon). See `unify-object-byte-layout`
/// PR-3 (路 A 标记指针) + object-abi.md §2.1.
pub(crate) const GCREF_ADDR_BITS: u32 = 48;
/// Mask isolating the 48-bit address from a tagged pointer.
pub(crate) const GCREF_ADDR_MASK: usize = (1usize << GCREF_ADDR_BITS) - 1;

/// GC-managed heap reference. **PR-3 backing (路 A 标记指针)**: a single
/// 8-byte tagged `NonNull<RegionEntry<T>>` — low 48 bits are the backing
/// `RegionEntry` address, high 16 bits are a **narrow generation snapshot**
/// (ABA guard, was a separate `u32` field). Deref masks off the tag.
///
/// Shrinking from 16B (ptr8 + gen4 + pad4) to 8B is what lets a reference
/// inline directly into an object's `bytes` blob as a raw 8-byte pointer
/// (PR-3 removes the PR-2 `refs` side-table) and, downstream, lets `Value`
/// shrink to 16B (PR-5).
///
/// **Narrow-generation tradeoff**: the ABA window is 2^16 slot reuses
/// instead of 2^32. Accepted per design Decision 2 (Open Question tracked
/// for CI stress eval). The backing `RegionEntry.generation` stays `u32`;
/// only the snapshot carried in the handle narrows.
///
/// **Lifetime contract**: caller must not let `GcRef<T>` outlive the
/// `ArcMagrGC` that owns the backing `Region<T>`. In z42's architecture
/// this is automatic — all `GcRef`s live inside `Value` instances which
/// in turn live inside the heap's pinned roots, registry, or VmContext
/// state. Embedders that hold `GcRef` outside the heap must ensure
/// teardown order.
pub struct GcRef<T> {
    /// Tagged pointer: `[ 16-bit generation | 48-bit RegionEntry address ]`.
    /// Never dereferenced directly — always through [`GcRef::entry_addr`],
    /// which masks the tag off. The backing entry address is stable for the
    /// entry's lifetime (chunks in Region are Box-owned and never relocate).
    tagged: NonNull<RegionEntry<T>>,
    /// Variance + dropck marker: behaves like `&'static RegionEntry<T>`
    /// for the type system (we own neither the entry nor T, just a
    /// reference into shared region storage).
    _phantom: PhantomData<RegionEntry<T>>,
}

// PR-3 invariant: the handle is exactly one machine word. Guards against an
// accidental field re-addition regressing the 8B layout the byte-inlining
// (Chunk 2) and Value-16B (PR-5) work depends on.
const _: () = assert!(std::mem::size_of::<GcRef<u8>>() == 8);
const _: () = assert!(std::mem::size_of::<Option<GcRef<u8>>>() == 8);

// SAFETY: GcRef<T> is conceptually a shared reference into a
// thread-safe storage region. The backing RegionEntry<T> contains
// Mutex<T> (Send+Sync via parking_lot for any T: Send) + atomics
// (Send+Sync). All field accesses go through synchronized primitives.
// Caller upholds the lifetime contract (no use-after-region-free).
unsafe impl<T: Send + Sync> Send for GcRef<T> {}
unsafe impl<T: Send + Sync> Sync for GcRef<T> {}

impl<T> GcRef<T> {
    /// Fold a clean entry pointer + narrow generation into the tagged
    /// pointer representation. Uses strict-provenance `map_addr` so the
    /// result keeps `entry`'s provenance (Miri-clean; the tag lives only
    /// in the never-dereferenced high bits).
    #[inline]
    fn pack(entry: NonNull<RegionEntry<T>>, generation: u32) -> NonNull<RegionEntry<T>> {
        let gen16 = generation as u16 as usize;
        let tagged = entry
            .as_ptr()
            .map_addr(|a| (a & GCREF_ADDR_MASK) | (gen16 << GCREF_ADDR_BITS));
        // SAFETY: the low 48 bits come from a valid non-null pointer, so the
        // masked address is non-zero regardless of the generation tag.
        unsafe { NonNull::new_unchecked(tagged) }
    }

    /// The backing `RegionEntry` pointer with the generation tag masked off.
    #[inline]
    fn entry_addr(&self) -> NonNull<RegionEntry<T>> {
        let clean = self.tagged.as_ptr().map_addr(|a| a & GCREF_ADDR_MASK);
        // SAFETY: masking the tag off a valid tagged pointer yields the
        // original non-null entry address.
        unsafe { NonNull::new_unchecked(clean) }
    }

    /// The 16-bit narrow generation snapshot carried in the tag.
    #[inline]
    fn gen16(&self) -> u16 {
        (self.tagged.as_ptr().addr() >> GCREF_ADDR_BITS) as u16
    }

    /// **add-custom-allocator P1 (2026-05-22)**: construct a `GcRef`
    /// pointing at an existing `RegionEntry`. Caller is responsible for
    /// supplying a valid pointer + matching generation. Used by
    /// `ArcMagrGC::alloc_object` / `alloc_array` after `Region::alloc`
    /// returns a handle.
    ///
    /// SAFETY: `entry` must be a stable pointer to a live `RegionEntry<T>`
    /// inside a `Region<T>` that outlives this `GcRef`. `generation`
    /// must match the entry's current generation at the time of call.
    /// **PR-3**: the `u32` generation is truncated to a 16-bit snapshot
    /// (narrow-generation ABA tradeoff, see the type docs).
    pub(crate) unsafe fn from_region_entry(
        entry: NonNull<RegionEntry<T>>,
        generation: u32,
    ) -> Self {
        Self { tagged: Self::pack(entry, generation), _phantom: PhantomData }
    }

    /// **Transitional standalone constructor**: allocates a `RegionEntry`
    /// outside any `Region<T>` (via `Box::leak`). Memory is intentionally
    /// leaked — the entry stays alive for the rest of the process. Used
    /// only by tests + the rare callsite that constructs a `GcRef` without
    /// a heap context (e.g., `corelib/array.rs::builtin_array_clone` will
    /// be migrated to `ctx.heap().alloc_array` in a follow-up commit).
    ///
    /// This is the only allocation path that doesn't go through a Region;
    /// such GcRefs participate in identity / borrow / mark APIs but are
    /// invisible to GC sweep (not in any heap registry → never reclaimed
    /// while the process lives).
    pub fn new(value: T) -> Self
    where
        T: 'static,
    {
        let entry: &'static mut RegionEntry<T> =
            Box::leak(Box::new(RegionEntry::new_for_test(value)));
        Self {
            tagged: Self::pack(NonNull::from(entry), 0),
            _phantom: PhantomData,
        }
    }

    /// Resolve to the inner `&RegionEntry<T>`. Panics if generation
    /// mismatches (use-after-finalize per design D5).
    fn entry_ref(&self) -> &RegionEntry<T> {
        // SAFETY: entry pointer is stable for the entry's lifetime;
        // caller upholds the GcRef-not-outlive-Region contract.
        let e = unsafe { self.entry_addr().as_ref() };
        debug_assert!(
            e.generation.load(Ordering::Acquire) as u16 == self.gen16()
                && e.alive.load(Ordering::Acquire),
            "GcRef::entry_ref: generation/alive mismatch — use-after-finalize"
        );
        e
    }

    /// Immutably borrow the inner value (blocking lock on the
    /// `RegionEntry`'s `Mutex<T>`).
    pub fn borrow(&self) -> Ref<'_, T> {
        self.entry_ref().value.lock()
    }

    /// Mutably borrow the inner value (blocking lock).
    pub fn borrow_mut(&self) -> RefMut<'_, T> {
        self.entry_ref().value.lock()
    }

    /// **extract-typedesc-from-mutex (2026-05-31)**: lockless raw
    /// pointer to the inner value. Used by typed accessor wrappers
    /// (e.g. `ScriptObject::type_desc_unlocked`) for fields that are
    /// write-once-at-alloc and never mutated — reading them through
    /// the Mutex would pay the lock cost for no benefit.
    ///
    /// # Safety
    ///
    /// Caller must guarantee the read targets a field that is **never
    /// mutated after `alloc_object` returns the GcRef**. Currently
    /// applies to `ScriptObject::type_desc` and `ScriptObject::type_args`
    /// (write-once verified by `grep -rn '.type_desc *=' src/runtime/`).
    /// Reading mutable fields (slots / native) this way is undefined
    /// behavior — use `borrow()` / `borrow_mut()` for those.
    #[inline]
    pub fn data_ptr_unlocked(&self) -> *mut T {
        self.entry_ref().value.data_ptr()
    }

    /// True iff `a` and `b` point to the same `RegionEntry`. Both
    /// pointer-equality AND generation-equality (so a stale GcRef
    /// pointing at a tombstoned slot does not equal a fresh GcRef
    /// pointing at the reused slot — that would be a spurious match).
    pub fn ptr_eq(a: &Self, b: &Self) -> bool {
        // The tag encodes both address and generation, so comparing the
        // whole tagged pointer covers pointer- AND generation-equality.
        a.tagged == b.tagged
    }

    /// Stable pointer to the inner `Mutex<T>` — used for identity
    /// hashing / dedup keying. Address stable for entry lifetime.
    pub fn as_ptr(this: &Self) -> *const Mutex<T> {
        // SAFETY: entry pointer stable for lifetime; field access is
        // a direct offset, no atomic. Returned pointer is the
        // identity key for the entry.
        unsafe { &(*this.entry_addr().as_ptr()).value as *const Mutex<T> }
    }

    /// **add-mark-sweep-collector P1 (2026-05-21)**: atomically set
    /// the mark bit. Returns `true` on 0→1 transition (CAS won).
    pub fn mark(this: &Self) -> bool {
        this.entry_ref().mark()
    }

    /// Read current mark state.
    pub fn is_marked(this: &Self) -> bool {
        this.entry_ref().is_marked()
    }

    /// Reset mark to 0 (sweep on survivors).
    pub fn clear_mark(this: &Self) {
        this.entry_ref().clear_mark();
    }

    /// Create a weak reference (does not extend liveness).
    pub fn downgrade(this: &Self) -> WeakGcRef<T> {
        // Weak handle carries the same tagged pointer (address + narrow gen).
        WeakGcRef {
            tagged: this.tagged,
            _phantom: PhantomData,
        }
    }

    /// Register a one-shot finalizer. Fires at next `sweep_phase` if
    /// the entry is unreachable, or immediately via `Std.GC.Finalize(x)`
    /// (P2 API).
    pub(crate) fn set_finalizer(this: &Self, fin: FinalizerFn) {
        *this.entry_ref().finalizer.lock() = Some(fin);
    }

    /// Cancel + take the registered finalizer. Returns true if one
    /// was previously registered.
    pub(crate) fn cancel_finalizer(this: &Self) -> bool {
        this.entry_ref().finalizer.lock().take().is_some()
    }

    /// Take the finalizer (one-shot). Used by sweep when firing the
    /// finalizer at object death.
    #[allow(dead_code)] // becomes used by P2 `Std.GC.Finalize(x)` builtin
    pub(crate) fn take_finalizer(this: &Self) -> Option<FinalizerFn> {
        this.entry_ref().finalizer.lock().take()
    }

    /// Query whether a finalizer is registered.
    pub(crate) fn has_finalizer(this: &Self) -> bool {
        this.entry_ref().finalizer.lock().is_some()
    }

    /// **add-custom-allocator P1 (2026-05-22)**: expose the underlying
    /// `RegionEntry` pointer + generation for `ArcMagrGC` internal
    /// bookkeeping (sweep walks regions directly; this is the inverse
    /// — given a Value-held GcRef, find which entry to tombstone).
    pub(crate) fn entry_ptr(&self) -> NonNull<RegionEntry<T>> {
        self.entry_addr()
    }

    /// **add-generational-gc P0 (2026-05-22)**: read the entry's
    /// current `gen_age`. Used by write barrier override under
    /// `GcMode::GenerationalMarkSweep` to detect cross-gen writes.
    /// Lock-free atomic load (`Relaxed`).
    pub fn gen_age(this: &Self) -> u8 {
        this.entry_ref().gen_age()
    }

    #[allow(dead_code)] // becomes used by P2 `Std.GC.Finalize(x)` builtin
    pub(crate) fn generation(&self) -> u32 {
        self.gen16() as u32
    }
}

impl<T> Clone for GcRef<T> {
    /// **D8 (no atomic op on clone)**: 8-byte memcpy of the tagged pointer.
    /// No `fetch_add`, no `Arc::clone`. Hot path on every Value passing
    /// through interp / JIT / closure capture / field access.
    fn clone(&self) -> Self {
        Self { tagged: self.tagged, _phantom: PhantomData }
    }
}

impl<T> Drop for GcRef<T> {
    /// **D8 cont.**: no-op. No refcount to decrement. Finalizer fires
    /// at sweep_phase (or via `Std.GC.Finalize(x)`), never here.
    fn drop(&mut self) {}
}

impl<T: std::fmt::Debug> std::fmt::Debug for GcRef<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // SAFETY: entry pointer stable; we're just reading metadata.
        let e = unsafe { self.entry_addr().as_ref() };
        if !e.alive.load(Ordering::Acquire) {
            return f.debug_tuple("GcRef").field(&"<tombstoned>").finish();
        }
        if e.generation.load(Ordering::Acquire) as u16 != self.gen16() {
            return f.debug_tuple("GcRef").field(&"<stale generation>").finish();
        }
        match e.value.try_lock() {
            Some(b) => f.debug_tuple("GcRef").field(&*b).finish(),
            None    => f.debug_tuple("GcRef").field(&"<borrowed>").finish(),
        }
    }
}

/// Weak GC reference. Does not extend liveness; `upgrade` returns
/// `None` if the target was tombstoned (alive=false) or the slot was
/// reused with a different generation.
pub struct WeakGcRef<T> {
    /// Same tagged-pointer layout as `GcRef` (address + narrow generation).
    tagged: NonNull<RegionEntry<T>>,
    _phantom: PhantomData<RegionEntry<T>>,
}

unsafe impl<T: Send + Sync> Send for WeakGcRef<T> {}
unsafe impl<T: Send + Sync> Sync for WeakGcRef<T> {}

impl<T> WeakGcRef<T> {
    /// The backing `RegionEntry` pointer with the generation tag masked off.
    #[inline]
    fn entry_addr(&self) -> NonNull<RegionEntry<T>> {
        let clean = self.tagged.as_ptr().map_addr(|a| a & GCREF_ADDR_MASK);
        // SAFETY: masking the tag off a valid tagged pointer yields the
        // original non-null entry address.
        unsafe { NonNull::new_unchecked(clean) }
    }

    /// The 16-bit narrow generation snapshot carried in the tag.
    #[inline]
    fn gen16(&self) -> u16 {
        (self.tagged.as_ptr().addr() >> GCREF_ADDR_BITS) as u16
    }

    /// Try to recover a strong reference. Returns `None` if the
    /// entry has been tombstoned by sweep, or if the slot was reused
    /// (generation mismatch).
    pub fn upgrade(&self) -> Option<GcRef<T>> {
        // SAFETY: entry pointer stable for the region's lifetime
        // (chunks are Box-owned, never freed individually). The
        // tombstone/generation check below is the correctness
        // boundary against use-after-tombstone.
        let e = unsafe { self.entry_addr().as_ref() };
        let cur_gen = e.generation.load(Ordering::Acquire) as u16;
        if !e.alive.load(Ordering::Acquire) || cur_gen != self.gen16() {
            return None;
        }
        Some(GcRef { tagged: self.tagged, _phantom: PhantomData })
    }
}

impl<T> Clone for WeakGcRef<T> {
    fn clone(&self) -> Self {
        Self { tagged: self.tagged, _phantom: PhantomData }
    }
}

impl<T> std::fmt::Debug for WeakGcRef<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let e = unsafe { self.entry_addr().as_ref() };
        let dropped = !e.alive.load(Ordering::Acquire)
            || e.generation.load(Ordering::Acquire) as u16 != self.gen16();
        f.debug_struct("WeakGcRef").field("dropped", &dropped).finish()
    }
}

