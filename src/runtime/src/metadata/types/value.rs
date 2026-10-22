//! Value 枚举 + impl / PartialEq。refactor-split-metadata-types：从 2436 行的 `types.rs` 按职责拆出，
//! 对外路径不变（`metadata::types::*` 经 hub 的 `pub use` 全量再导出）。

#![allow(unused_imports)]
use super::*;
use std::sync::Arc;
use crate::metadata::vstr::Str;
use crate::gc::ArrRef;
use crate::gc::var_region::{BlockType, VarGcRef};
use crate::gc::heap::MagrGC;

#[derive(Debug, Clone, Copy)]
#[repr(C, u8)]
pub enum Value {
    I64(i64)        = 0,
    F64(f64)        = 1,
    Bool(bool)      = 2,
    Char(char)      = 3,
    /// Immutable string primitive.  `s.Length` → virtual field dispatch in FieldGet.
    ///
    /// review.md C1+C3 : `Arc<str>` instead of `String`. Saves
    /// 8 B/instance (Arc<str> = 16 B vs String = 24 B; no `cap` word) AND
    /// turns clone from O(n) byte copy into O(1) atomic refcount — the
    /// hot-path win for string-heavy interp / format / concat loops.
    /// Arc not Rc because `Value: Send + Sync` (see
    /// `gc/arc_heap_tests/send_sync.rs::assert_send_sync::<Value>()`).
    Str(Str)                    = 4,
    Null                        = 5,
    /// Heap-allocated fixed-length array with reference semantics. Object model R9: an
    /// [`ArrRef`](crate::gc::ArrRef) — one variable-length GC block holding the 16 B header (its
    /// kind word the array's descriptor: element type, mode) and the elements.
    Array(ArrRef)      = 6,
    /// Heap-allocated managed class instance with reference semantics.
    /// Object model R6: an [`ObjRef`](crate::gc::ObjRef) — one variable-length GC block holding
    /// the object header and its field cells.
    Object(crate::gc::ObjRef) = 7,
    /// Spec C4 — borrowed view of a `String` / `Array<u8>` for native FFI.
    /// Created by `PinPtr`, released by `UnpinPtr`. The `ptr` is an
    /// untyped raw address — consumers must know the source `kind` to
    /// interpret it. Field access (`.ptr` / `.len`) goes through the
    /// regular `FieldGet` instruction.
    ///
    /// review.md C1 step 1 : payload boxed to shrink the
    /// inline `Value` size — `PinnedView` is created on the rare
    /// `PinPtr` opcode and immediately consumed by the next native
    /// call, so the heap-alloc cost is dominated by the FFI it enables.
    /// make-value-copy: 8B handle into `VmContext::transient_arena` (payload `PinnedViewData`).
    PinnedView { idx: u32, frame_id: u32 } = 8,
    /// Function reference value: a no-capture lambda or a method group bound to a free
    /// function (`LoadFn`). `CallIndirect` dispatches to the function it names.
    ///
    /// P1-2 PR 8 : the payload is the target's [`FnId`](crate::metadata::tokens::FnId),
    /// bound once when the value is created (`LoadFn` — the per-site token makes later
    /// executions of the same site a single load), instead of the function's name as a GC
    /// string resolved again by every `CallIndirect`. The id lives in the space `Call`
    /// tokens use (`interp::exec_call::fn_by_id`): in every production run the VM's
    /// `FuncTable` id (entry functions `0..n`, lazily loaded ones after); against a module
    /// that is not the `FuncTable`'s entry module (unit tests on a bare `VmContext`), only
    /// that module's indices. Not a heap reference — no GC edge, no write barrier. Equal
    /// ids ⇔ the same function, so delegate equality is id equality; the name for
    /// reflection / display comes from the `Function` the id names.
    FuncRef(crate::metadata::tokens::FnId) = 9,
    /// L3 capturing closure value: pairs a heap-allocated env (Vec<Value>)
    /// with the lifted function's id (P1-2 PR 8: [`ClosureData::fn_id`], bound at `MkClos`;
    /// previously its qualified name). CallIndirect on a Closure
    /// passes `env` as the callee's first implicit parameter and copies user
    /// args after it. See docs/internals/src/runtime/escape-analysis.md + impl-closure-l3-core.
    ///
    /// review.md C1 chunk 5 : payload boxed (the last and
    /// biggest cold-path variant — 40 B inline = ArrRef(16 B) + String(24 B)).
    /// Boxing drops Value enum to ~24 B; capturing closures pay one heap
    /// alloc per `MkClos` but that's dwarfed by the env's own GC alloc.
    ///
    /// unify-gc-heap PR-2 : `VarGcRef` (8B) instead of `Box<ClosureData>`. The
    /// `ClosureData` now lives in the GC variable-length region (`region_var`) — a single GC
    /// heap instead of a `Box` outside GC. `ClosureData` is immutable after creation, so the
    /// block needs no per-entry lock; access is a lock-free `&ClosureData` (kept alive by
    /// reachability, like `ArrRef`). Cloning a closure `Value` now shares the same heap closure
    /// (handle copy) instead of deep-cloning the box. See `Value::closure_data`.
    Closure(VarGcRef) = 10,
    // discriminant 11 retired (runtime-audit D6): `StackClosure` — the
    // frame-arena closure form for `MkClos` with its stack-alloc byte set — is gone.
    // z42c never set that byte (its escape analysis covers objects / arrays only), so
    // every closure is a heap `Closure`. 11 stays unused (no renumbering of 12+).
    /// Spec impl-ref-out-in-runtime: `ref` / `out` / `in` 参数运行时表达。
    /// 持有该 Value 的寄存器在 frame.get/set 时被透明 deref（单点 dispatch，
    /// 见 `interp/mod.rs::Frame::get`）。引用永远不离开调用栈帧（前置 spec
    /// design Decision 9 + R1），因此 Stack kind 的 frame_idx 不会 stale。
    ///
    /// review.md C1 chunk 4 : payload boxed because RefKind
    /// is 32 B (Field variant) — biggest cold-path payload after
    /// Closure. Refs only live in registers for a single call's
    /// duration, so the box alloc is a tiny fraction of the call cost.
    /// make-value-copy: 8B handle into `VmContext::transient_arena` (payload `RefKind`).
    Ref { idx: u32, frame_id: u32 } = 12,
    // discriminant 13 retired by unify Phase 2 R3（装箱统一）：基元装箱不再走
    // `Value::Boxed(Box<BoxedPrim>)`——整数标量装进堆 `ScriptObject` 的 `struct_bytes` 并以
    // `Value::BoxedStruct` 承载（与 struct 装箱同一模型 + 引用身份）。判别号 13 留空（14-18 号不
    // 重编，`#[repr(C,u8)]` 显式判别 + JIT 原始布局不受影响）。
    /// add-escape-analysis-stack-alloc: 逃逸分析证明不逃逸的对象，interp 在**每线程
    /// context 的栈 arena**（`VmContext::stack_obj_arena`）里分配，绕过 GC、随创建帧
    /// 退出 LIFO 截断释放。句柄（非堆指针）：
    ///   * `idx` — arena 内条目下标（`ctx.stack_obj_arena[idx]`，任何帧都能直取——ctor
    ///     子帧因此天然可解 `this`，无需跨帧机制）。
    ///   * `frame_id` — 创建帧的单调 id（诊断）：解引用时校验 arena 槽的 frame_id 与之
    ///     相符 + idx 在界内，不符/越界 = 逃逸分析误判、栈句柄活过创建帧 → 明确报错
    ///     （而非静默 use-after-free）。帧退出 truncate 后槽被后续帧复用 → frame_id 不符即抓。
    /// JIT 从不产生本变体（D2：JIT 忽略 stack_alloc、照常堆分配），故 JIT 值路径永不遇到。
    StackObject { idx: u32, frame_id: u32 } = 14,
    /// add-escape-analysis-stack-alloc: 不逃逸数组的栈 arena 句柄（`VmContext::stack_arr_arena`）。
    /// 语义同 `StackObject`。
    StackArray { idx: u32, frame_id: u32 } = 15,
    /// add-struct-value-semantics Phase A: blob 值类型（多字段 struct）句柄。`idx` 索引 per-context
    /// 字节 arena（`VmContext::struct_arena`）里的 blob 条目；`frame_id` = 创建帧单调 id（staleness
    /// guard，同 `StackObject`）。未装箱 struct 值以此句柄在寄存器间流转；字节 blob 存 arena。
    /// blob 内引用叶子由 arena 的 root scanner 按 TypeDesc 引用位图扫描（trace_children 视为叶子，
    /// 避免双计）。
    StructRef { idx: u32, frame_id: u32 } = 16,
    /// add-struct-object-boxing (PR2a): 装箱的 blob 值 struct。`object o = someStruct` 擦除到
    /// `object`/接口时，把帧作用域 arena blob **拷进堆稳定表示**（脱离帧生命周期，修裸拷 `StructRef`
    /// 句柄逃逸帧的 use-after-free）。载荷拥有 `bytes`（基元叶子字节快照）+ `refs`（引用叶子作真 Value
    /// → GC 扫描 + 内存安全，镜像 `struct_arena::StructSlot` 去掉 `frame_id`）+ `type_name`（供
    /// `GetType`/`is`/`as`）。装箱经 `__box_struct` builtin（复用 Builtin opcode，无格式 bump）；`(P)o`
    /// 拆箱把 blob 拷回当前帧 arena `StructRef`。unboxed struct 仍无 vtable——对象协议由本变体的 VM 分支
    /// （身份）+ 编译器合成值方法（Equals 等，PR2b）承载。
    ///
    /// **add-boxed-struct-identity (P4b, 路 B2)**: 装箱 = 一个 struct 类型的共享 `ScriptObject`
    /// （`type_desc.is_struct()`，struct blob 存进对象的 `struct_bytes`/`struct_refs`，`slots` 空）。
    /// 载荷从值语义 `Box<BoxedStructData>` 改为**共享堆句柄** `crate::gc::ObjRef` → 对齐 C# 引用身份
    /// （`object b = a` 别名同盒、反射 `SetValue` 写穿、传参改盒可见）。复用对象块 + 全部 GC 机制
    /// （GC 里与 `Value::Object` 同路标记/追踪；仅 is/as/GetType/vcall/Equals 保持 boxed 值类型特判）。
    BoxedStruct(crate::gc::ObjRef) = 17,
    /// add-struct-heap-inline (P3b, route α): a transient handle to a value-struct
    /// **inlined in a heap array element** (`arr[i]`). Unlike an object field (whose
    /// composite byte offset the compiler bakes → base = `Value::Object`), a struct[]
    /// element's byte offset depends on the runtime index, so `arr[i]` materializes
    /// this handle and a following `StructFieldGetPrim/SetPrim` reads/writes a leaf of
    /// element `index` (routing byte/ref access through the value-struct array's
    /// element). Payload boxed (8 B pointer) so `Value` stays 24 B. GC follows `arr`.
    /// make-value-copy: 8B handle into `VmContext::transient_arena` (payload `StructArrayElem`).
    StructRefHeap { idx: u32, frame_id: u32 } = 18,
}

// unify-object-byte-layout PR-5 : `Value` is the interpreter's
// register-file cell and the JIT strides its register file by
// `size_of::<Value>()` (`jit/translate.rs` `VALUE_STRIDE`/`STRIDE`), so its
// size is an ABI contract, not an incidental detail. After PR-3 (ArrRef 8 B) +
// PR-4 (Str 8 B) + this PR (FuncRef → Str 8 B; P1-2 PR 8: FnId 4 B), every payload is ≤ 8 B, so
// `#[repr(C, u8)]` gives tag(1 B, padded to 8) + 8 B payload = 16 B. This
// assert fails to compile the moment a payload grows past 8 B (e.g. a new
// fat-pointer / two-word variant), forcing it to be boxed before it can
// silently grow the register file / JIT stride back to 24 B.
const _: () = assert!(std::mem::size_of::<Value>() == 16);
impl Value {
    /// interp-typed-superinstr : read the `I64` payload **without**
    /// a discriminant check. The interpreter's typed super-instructions call
    /// this only when the compiler-emitted `reg_types[r] == IrType::I64` — the
    /// **same invariant** the JIT's raw-slot arithmetic already trusts
    /// (`jit::translate::is_i64_typed`). `debug_assert` catches an invariant
    /// violation in debug builds; in release the `unreachable_unchecked` lets
    /// LLVM drop the tag branch entirely.
    ///
    /// # Safety
    /// Undefined behavior if `self` is not `Value::I64`. Callers must have
    /// verified the register's static type is `I64` (via `reg_types`).
    #[inline(always)]
    pub unsafe fn as_i64_unchecked(&self) -> i64 {
        match self {
            Value::I64(x) => *x,
            // reg_types guaranteed I64; any other variant is a compiler bug.
            _ => {
                debug_assert!(false, "as_i64_unchecked on non-I64: {self:?}");
                std::hint::unreachable_unchecked()
            }
        }
    }

    /// interp-typed-superinstr : read the `Bool` payload without a
    /// discriminant check. See [`Value::as_i64_unchecked`] for the safety
    /// contract (here the invariant is `reg_types[r] == IrType::Bool`).
    ///
    /// # Safety
    /// Undefined behavior if `self` is not `Value::Bool`.
    #[inline(always)]
    pub unsafe fn as_bool_unchecked(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            _ => {
                debug_assert!(false, "as_bool_unchecked on non-Bool: {self:?}");
                std::hint::unreachable_unchecked()
            }
        }
    }

    /// **add-write-barriers **: returns `true` iff writing
    /// this value into a heap slot must dispatch a GC write barrier.
    /// Heap-ref variants: `Object` / `Array` / `Closure` (Closure.env is a
    /// `ArrRef`) / `Ref` with `RefKind::Array` or `RefKind::Field`
    /// (the inner `gc_ref` is a real heap edge). All primitives, plus
    /// `FuncRef` (a `FnId`, P1-2 PR 8) / `PinnedView` (raw ptr) /
    /// `Ref::Stack` (stack location)
    /// return `false` — none of them create a strong heap → heap edge
    /// that card-marking or SATB collectors would care about.
    ///
    /// Mirrors the variant selection of [`Value::trace_children`] —
    /// `is_heap_ref` is the predicate, `trace_children` is the traversal.
    /// unify-gc-heap PR-2: access the [`ClosureData`] behind a `Value::Closure`. Returns
    /// `None` for non-closures. The `ClosureData` lives in the GC `region_var`; the block is
    /// alive as long as this closure `Value` is reachable, so the borrow is sound (same
    /// reachability model as `ArrRef`). `ClosureData` is immutable after creation → no lock.
    #[inline]
    pub fn closure_data(&self) -> Option<&ClosureData> {
        match self {
            // SAFETY: a live `Value::Closure` names an alive `Closure` block (reachability);
            // the payload is exactly one immutable `ClosureData`, valid for `&self`'s borrow.
            Value::Closure(vref) => Some(unsafe { &*vref.payload_as_ptr::<ClosureData>() }),
            _ => None,
        }
    }

    /// P1-2 PR 8: the bound target of a callable value — a `FuncRef`'s id or a closure's
    /// `fn_id` (same id space, see [`Value::FuncRef`]); `None` for anything else.
    #[inline]
    pub fn callable_id(&self) -> Option<crate::metadata::tokens::FnId> {
        match self {
            Value::FuncRef(id) => Some(*id),
            Value::Closure(_) => self.closure_data().map(|d| d.fn_id),
            _ => None,
        }
    }

    #[inline]
    pub fn is_heap_ref(&self) -> bool {
        match self {
            Value::Object(_) | Value::Array(_) | Value::Closure(_) => true,
            // unify-gc-heap PR-4: strings are GC blocks now — storing one into a heap
            // slot (object ref field / array element / struct ref leaf) is a heap edge
            // that needs a write barrier (generational card),
            // so the string block is found + kept marked. (`FuncRef` holds a `FnId` since
            // P1-2 PR 8 — not a heap edge.)
            Value::Str(_) => true,
            // add-boxed-struct-identity (P4b, 路 B2): 装箱 struct 现是共享 `ScriptObject` 句柄 →
            // 与 `Value::Object` 同为强堆边（存进堆槽需写屏障）。
            Value::BoxedStruct(_) => true,
            // make-value-copy: `Ref` / `StructRefHeap` are now transient-arena handles
            // (like `StructRef` / `StackObject`) — their payload's GcRefs are kept marked
            // by the arena root scan, and the handles never escape the creating frame into
            // a heap slot, so no write barrier is needed here → fall through to `false`.
            _ => false,
        }
    }

    /// **add-mark-sweep-collector P1 ** / **unify-gc-heap PR-5**: visit every
    /// direct GC-reference child `Value` reachable from `self`. **Single source** for both
    /// the GC mark phase and read-only graph enumeration (heapsnapshot / retention query) —
    /// they differ in one place, gated by `marking`:
    ///
    /// - **`marking = Some(_)`** (mark phase, from `arc_heap`'s mark loop): surfaces a closure's
    ///   env array (`Value::Array`) as a child so the mark loop marks it too.
    /// - **`marking = None`** (enumeration, via `ArcMagrGC::scan_object_refs`, and the
    ///   generational inspections): descends **directly** into a closure's captured refs (the
    ///   env is internal, not surfaced as a graph node).
    ///
    /// Primitives / stack-arena handles / struct-blob refs yield no children (their storage
    /// is scanned directly by the external root scanner, so walking here would double-count).
    /// [`Value::is_heap_ref`] is the matching predicate; this is the traversal.
    #[inline]
    pub fn visit_gc_children(&self, marking: Option<crate::gc::refs::MarkKind>, visit: &mut dyn FnMut(&Value)) {
        match self {
            Value::Object(rc) => {
                // Every 8 B reference word plus the type-parameter cells' tag words.
                rc.borrow().visit_refs(visit);
            }
            // Object model R9: an array is one block — its elements are its children (every
            // mode: REF words, MIXED cells, struct[] leaves, FROZEN values).
            Value::Array(rc) => rc.borrow().visit_refs(visit),
            Value::Closure(vref) => {
                // unify-gc-heap PR-2/PR-5: the closure's `ClosureData` is a GC block in region_var.
                // SAFETY: a reachable closure names an alive block; payload is one ClosureData.
                let data = unsafe { &*vref.payload_as_ptr::<ClosureData>() };
                if marking.is_some() {
                    // Push the env array (so the mark loop marks its block and traces its
                    // elements). The target is a `FnId` (P1-2 PR 8), not a GC edge.
                    visit(&Value::Array(data.env));
                } else {
                    // Enumeration: descend into the captured refs directly — the env is the
                    // closure's internal, not a distinct graph node.
                    data.env.borrow().visit_refs(visit);
                }
            }
            // add-boxed-struct-identity (P4b, 路 B2): 装箱 struct 是共享 `ScriptObject` → 与 Object
            // 同路追踪其 blob 的引用叶子（按 struct 布局）。对象本身由 mark 循环的 BoxedStruct 臂标记。
            Value::BoxedStruct(gc) => gc.borrow().visit_refs(visit),
            // make-value-copy: `Ref` / `StructRefHeap` are transient-arena handles — leaves
            // here, exactly like `StructRef` / `StackObject`. Their payload's GcRefs (a
            // Ref's Array/Field target, a StructRefHeap's backing array) are scanned
            // *directly* by `TransientArena::scan_roots` (a GC root), so tracing through
            // the handle here would double-count.
            // Primitives — no children.
            // add-escape-analysis-stack-alloc: StackObject / StackArray are
            // leaves for the child-traversal — their slots/elems live in the
            // frame arena and are scanned directly as GC roots by the external
            // root scanner, so walking them here would double-count. A stack handle appearing in
            // a heap object's slot would be an escape-analysis bug; the debug
            // asserts in the store paths (FieldSet/ArraySet/StaticSet) catch it.
            // add-struct-value-semantics: StructRef is a leaf here — its blob's
            // reference leaves are scanned directly by the struct-arena root
            // scanner (mirrors StackObject), so walking here would double-count.
            Value::I64(_) | Value::F64(_) | Value::Bool(_) | Value::Char(_)
            | Value::Str(_) | Value::Null | Value::FuncRef(_)
            | Value::PinnedView { .. }
            | Value::Ref { .. } | Value::StructRefHeap { .. }
            | Value::StackObject { .. } | Value::StackArray { .. }
            | Value::StructRef { .. } => {}
        }
    }

    /// GC mark-phase traversal — thin wrapper over [`Value::visit_gc_children`] for a mark of
    /// `kind` (array backings are marked with the same kind). `#[inline]` so the constant
    /// folds away on the hot mark loop.
    #[inline]
    pub fn trace_children(&self, kind: crate::gc::refs::MarkKind, visit: &mut dyn FnMut(&Value)) {
        self.visit_gc_children(Some(kind), visit);
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::I64(a),  Value::I64(b))  => a == b,
            (Value::F64(a),  Value::F64(b))  => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Char(a), Value::Char(b)) => a == b,
            (Value::Str(a),  Value::Str(b))  => a == b,
            (Value::Null,    Value::Null)    => true,
            // Array/Object equality is reference equality (same as C# reference semantics)
            (Value::Array(a),  Value::Array(b))  => ArrRef::ptr_eq(a, b),
            (Value::Object(a), Value::Object(b)) => crate::gc::ObjRef::ptr_eq(a, b),
            // fix-delegate-equality-operator: function values are reference-ish too, and
            // these two arms were simply MISSING — so `==` fell through to `_ => false`
            // and `f == f` answered **false** (a silently wrong answer, not an error).
            // Identity semantics are the ones `DelegateOps.ReferenceEquals` already
            // defines: a FuncRef is its function; a closure is its function plus its
            // captured env. P1-2 PR 8: "its function" = the bound `FnId` (one id per
            // function, so equal ids ⇔ the same function — what equal names meant before). That builtin defers here for these two variants, so the
            // rule has one spelling.
            (Value::FuncRef(a), Value::FuncRef(b)) => a == b,
            (Value::Closure(a), Value::Closure(b)) => {
                if a.ptr_eq(b) {
                    true
                } else {
                    let (da, db) = (super::closure_data_of(a), super::closure_data_of(b));
                    da.fn_id == db.fn_id && ArrRef::ptr_eq(&da.env, &db.env)
                }
            }
            // make-value-copy: `PinnedView` / `Ref` (and `StructRefHeap`)
            // are now transient-arena handles — compare by `{idx, frame_id}` handle
            // identity (same as `StackObject`). These are internal transient values; user
            // code has no by-value-equality dependency on them (they never reach a
            // user-visible `==` — an escape sink would have materialized the heap form).
            (Value::PinnedView { idx: i1, frame_id: g1 },
             Value::PinnedView { idx: i2, frame_id: g2 }) => i1 == i2 && g1 == g2,
            (Value::Ref { idx: i1, frame_id: g1 },
             Value::Ref { idx: i2, frame_id: g2 }) => i1 == i2 && g1 == g2,
            (Value::StructRefHeap { idx: i1, frame_id: g1 },
             Value::StructRefHeap { idx: i2, frame_id: g2 }) => i1 == i2 && g1 == g2,
            // add-primitive-value-boxing → unify Phase 2 R3: 装箱整数 vs 裸整数 —— 透明拆箱按值比较
            // （保留 add-primitive-value-boxing 的混合相等语义）。装箱整数盒是 `BoxedStruct`（整数标量
            // 存 struct_bytes）；非整数盒（多字段 struct 装箱）`boxed_prim_i64` 返 None → 落 `_=>false`
            // （struct 盒 ≠ 裸基元，正确）。装箱整数 vs 装箱整数由下方 BoxedStruct/BoxedStruct 臂按
            // struct_bytes 比（同 wrapper + 同字节 → 值相等），无需单列。
            (Value::BoxedStruct(a), Value::I64(n)) | (Value::I64(n), Value::BoxedStruct(a)) => {
                a.borrow().boxed_prim_i64() == Some(*n)
            }
            // add-struct-object-boxing (PR2a, provisional，design D5)：装箱 struct 值相等——同类型 ∧
            // 字节相等 ∧ 引用叶子逐 Value 相等（refs 的 Value::eq 处理 string 内容 / object 引用）。
            // add-boxed-struct-identity (P4b): 载荷现是共享 `ScriptObject`——先 ptr_eq（同盒必等），
            // 否则比两盒的 blob（保持值相等语义）。
            (Value::BoxedStruct(a), Value::BoxedStruct(b)) => {
                if crate::gc::ObjRef::ptr_eq(a, b) {
                    true
                } else {
                    // Reference leaves are reference words inside the blob: compare the bytes
                    // with those slots zeroed, and the decoded leaves by `Value` equality.
                    let (ao, bo) = (a.borrow(), b.borrow());
                    ao.type_desc.name == bo.type_desc.name && ao.boxed_blob() == bo.boxed_blob()
                }
            }
            // add-escape-analysis-stack-alloc: 栈句柄引用相等 —— 同 (frame_idx, idx,
            // frame_id) = 同一栈对象/数组（Eq 操作数在逃逸分析里是 neutral，故栈句柄
            // 可作 `p1==p2` / `p==null` 操作数；`==null` 落 `_ => false` = 正确「非 null」）。
            (Value::StackObject { idx: i1, frame_id: g1 },
             Value::StackObject { idx: i2, frame_id: g2 }) => i1 == i2 && g1 == g2,
            (Value::StackArray { idx: i1, frame_id: g1 },
             Value::StackArray { idx: i2, frame_id: g2 }) => i1 == i2 && g1 == g2,
            _ => false,
        }
    }
}
