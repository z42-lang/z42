//! Value 枚举 + impl / PartialEq。refactor-split-metadata-types（2026-09-03）：从 2436 行的 `types.rs` 按职责拆出，
//! 对外路径不变（`metadata::types::*` 经 hub 的 `pub use` 全量再导出）。

#![allow(unused_imports)]
use super::*;
use std::sync::Arc;
use crate::metadata::vstr::Str;
use crate::gc::GcRef;
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
    /// review.md C1+C3 (2026-05-27): `Arc<str>` instead of `String`. Saves
    /// 8 B/instance (Arc<str> = 16 B vs String = 24 B; no `cap` word) AND
    /// turns clone from O(n) byte copy into O(1) atomic refcount — the
    /// hot-path win for string-heavy interp / format / concat loops.
    /// Arc not Rc because `Value: Send + Sync` (see
    /// `gc/arc_heap_tests/send_sync.rs::assert_send_sync::<Value>()`).
    Str(Str)                    = 4,
    Null                        = 5,
    /// Heap-allocated dynamic array with reference semantics.
    /// add-reflection-array-element-type (2026-06-11): payload is `ArrayObj`
    /// (element type name + elems) instead of a bare `Vec<Value>`, so the array
    /// carries its element type at runtime (non-erased reflection). `ArrayObj`
    /// derefs to the element `Vec<Value>`, so element access is unchanged.
    Array(GcRef<ArrayObj>)      = 6,
    /// Heap-allocated managed class instance with reference semantics.
    Object(GcRef<ScriptObject>) = 7,
    /// Spec C4 — borrowed view of a `String` / `Array<u8>` for native FFI.
    /// Created by `PinPtr`, released by `UnpinPtr`. The `ptr` is an
    /// untyped raw address — consumers must know the source `kind` to
    /// interpret it. Field access (`.ptr` / `.len`) goes through the
    /// regular `FieldGet` instruction.
    ///
    /// review.md C1 step 1 (2026-05-27): payload boxed to shrink the
    /// inline `Value` size — `PinnedView` is created on the rare
    /// `PinPtr` opcode and immediately consumed by the next native
    /// call, so the heap-alloc cost is dominated by the FFI it enables.
    /// make-value-copy: 8B handle into `VmContext::transient_arena` (payload `PinnedViewData`).
    PinnedView { idx: u32, frame_id: u32 } = 8,
    /// Function reference value. Currently used by L2 no-capture lambda
    /// literals (see docs/design/language/closure.md §6). Indirect call dispatches
    /// to the named function in the loaded module.
    ///
    /// review.md C1 chunk 2 (2026-05-27): `Box<str>` instead of `String`.
    /// Saves 8 B/instance (Box<str> = 16 B vs String = 24 B; no `cap` word).
    /// FuncRef names are write-once at creation and read-only thereafter
    /// (immutable identity → no append/grow operation needed).
    ///
    /// unify-object-byte-layout PR-5 (2026-08-15): `Str` (8 B thin pointer)
    /// instead of `Box<str>` (16 B fat pointer). `Box<str>` was the *last*
    /// 16 B payload keeping `Value` at 24 B; swapping it for the vstr thin
    /// pointer drops the max payload to 8 B → `Value` = 16 B (see the
    /// `size_of::<Value>() == 16` static assert below). Length is read from
    /// the `StrHeader`, so `name.len()` stays O(1).
    FuncRef(Str) = 9,
    /// L3 capturing closure value: pairs a heap-allocated env (Vec<Value>)
    /// with the lifted function's qualified name. CallIndirect on a Closure
    /// passes `env` as the callee's first implicit parameter and copies user
    /// args after it. See docs/design/language/closure.md §6 + impl-closure-l3-core.
    ///
    /// review.md C1 chunk 5 (2026-05-27): payload boxed (the last and
    /// biggest cold-path variant — 40 B inline = GcRef(16 B) + String(24 B)).
    /// Boxing drops Value enum to ~24 B; capturing closures pay one heap
    /// alloc per `MkClos` but that's dwarfed by the env's own GC alloc.
    ///
    /// unify-gc-heap PR-2 (2026-08-15): `VarGcRef` (8B) instead of `Box<ClosureData>`. The
    /// `ClosureData` now lives in the GC variable-length region (`region_var`) — a single GC
    /// heap instead of a `Box` outside GC. `ClosureData` is immutable after creation, so the
    /// block needs no per-entry lock; access is a lock-free `&ClosureData` (kept alive by
    /// reachability, like `GcRef`). Cloning a closure `Value` now shares the same heap closure
    /// (handle copy) instead of deep-cloning the box. See `Value::closure_data`.
    Closure(VarGcRef) = 10,
    /// 2026-05-02 impl-closure-l3-escape-stack: 栈分配的 capturing closure 值。
    /// `env_idx` 索引创建该 closure 的 frame 的 `env_arena: Vec<Vec<Value>>`；
    /// CallIndirect 时由 dispatch 端通过当前帧的 arena 解 env。compiler 经
    /// escape 分析证明 closure 不离开创建 frame 时才发射该 variant；逃逸
    /// 场景仍走 `Value::Closure`。详见
    /// `docs/spec/archive/2026-05-02-impl-closure-l3-escape-stack/`。
    ///
    /// review.md C1 chunk 3 (2026-05-27): payload boxed to shrink the
    /// inline `Value` size — StackClosure is created on the rare
    /// non-escaping closure path and only consumed by the next
    /// `CallIndirect` before the creating frame returns.
    /// make-value-copy: 8B handle into `VmContext::transient_arena` (payload `StackClosureData`).
    StackClosure { idx: u32, frame_id: u32 } = 11,
    /// Spec impl-ref-out-in-runtime: `ref` / `out` / `in` 参数运行时表达。
    /// 持有该 Value 的寄存器在 frame.get/set 时被透明 deref（单点 dispatch，
    /// 见 `interp/mod.rs::Frame::get`）。引用永远不离开调用栈帧（前置 spec
    /// design Decision 9 + R1），因此 Stack kind 的 frame_idx 不会 stale。
    ///
    /// review.md C1 chunk 4 (2026-05-27): payload boxed because RefKind
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
    /// 载荷从值语义 `Box<BoxedStructData>` 改为**共享堆句柄** `GcRef<ScriptObject>` → 对齐 C# 引用身份
    /// （`object b = a` 别名同盒、反射 `SetValue` 写穿、传参改盒可见）。复用 `region_object` + 全部 GC 机制
    /// （GC 里与 `Value::Object` 同路标记/追踪；仅 is/as/GetType/vcall/Equals 保持 boxed 值类型特判）。
    BoxedStruct(GcRef<ScriptObject>) = 17,
    /// add-struct-heap-inline (P3b, route α): a transient handle to a value-struct
    /// **inlined in a heap array element** (`arr[i]`). Unlike an object field (whose
    /// composite byte offset the compiler bakes → base = `Value::Object`), a struct[]
    /// element's byte offset depends on the runtime index, so `arr[i]` materializes
    /// this handle and a following `StructFieldGetPrim/SetPrim` reads/writes a leaf of
    /// element `index` (routing byte/ref access through the array's `StructBytes`
    /// backing). Payload boxed (8 B pointer) so `Value` stays 24 B. GC follows `arr`.
    /// make-value-copy: 8B handle into `VmContext::transient_arena` (payload `StructArrayElem`).
    StructRefHeap { idx: u32, frame_id: u32 } = 18,
}

// unify-object-byte-layout PR-5 (2026-08-15): `Value` is the interpreter's
// register-file cell and the JIT strides its register file by
// `size_of::<Value>()` (`jit/translate.rs` `VALUE_STRIDE`/`STRIDE`), so its
// size is an ABI contract, not an incidental detail. After PR-3 (GcRef 8 B) +
// PR-4 (Str 8 B) + this PR (FuncRef → Str 8 B), every payload is ≤ 8 B, so
// `#[repr(C, u8)]` gives tag(1 B, padded to 8) + 8 B payload = 16 B. This
// assert fails to compile the moment a payload grows past 8 B (e.g. a new
// fat-pointer / two-word variant), forcing it to be boxed before it can
// silently grow the register file / JIT stride back to 24 B.
const _: () = assert!(std::mem::size_of::<Value>() == 16);
impl Value {
    /// interp-typed-superinstr (2026-08-01): read the `I64` payload **without**
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

    /// interp-typed-superinstr (2026-08-01): read the `Bool` payload without a
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

    /// **add-write-barriers (2026-05-21)**: returns `true` iff writing
    /// this value into a heap slot must dispatch a GC write barrier.
    /// Heap-ref variants: `Object` / `Array` / `Closure` (Closure.env is a
    /// `GcRef<Vec<Value>>`) / `Ref` with `RefKind::Array` or `RefKind::Field`
    /// (the inner `gc_ref` is a real heap edge). All primitives, plus
    /// `FuncRef` (string-keyed func table) / `PinnedView` (raw ptr) /
    /// `StackClosure` (stack arena env) / `Ref::Stack` (stack location)
    /// return `false` — none of them create a strong heap → heap edge
    /// that card-marking or SATB collectors would care about.
    ///
    /// Mirrors the variant selection of [`Value::trace_children`] —
    /// `is_heap_ref` is the predicate, `trace_children` is the traversal.
    /// unify-gc-heap PR-2: access the [`ClosureData`] behind a `Value::Closure`. Returns
    /// `None` for non-closures. The `ClosureData` lives in the GC `region_var`; the block is
    /// alive as long as this closure `Value` is reachable, so the borrow is sound (same
    /// reachability model as `GcRef`). `ClosureData` is immutable after creation → no lock.
    #[inline]
    pub fn closure_data(&self) -> Option<&ClosureData> {
        match self {
            // SAFETY: a live `Value::Closure` names an alive `Closure` block (reachability);
            // the payload is exactly one immutable `ClosureData`, valid for `&self`'s borrow.
            Value::Closure(vref) => Some(unsafe { &*vref.payload_as_ptr::<ClosureData>() }),
            _ => None,
        }
    }

    #[inline]
    pub fn is_heap_ref(&self) -> bool {
        match self {
            Value::Object(_) | Value::Array(_) | Value::Closure(_) => true,
            // unify-gc-heap PR-4: strings are GC blocks now — storing one into a heap
            // slot (object ref field / array element / struct ref leaf) is a heap edge
            // that needs a write barrier (generational card / concurrent mark-queue),
            // so the string block is found + kept marked. `FuncRef` carries a `Str`.
            Value::Str(_) | Value::FuncRef(_) => true,
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

    /// **add-mark-sweep-collector P1 (2026-05-21)** / **unify-gc-heap PR-5**: visit every
    /// direct GC-reference child `Value` reachable from `self`. **Single source** for both
    /// the GC mark phase and read-only graph enumeration (heapsnapshot / retention query) —
    /// they differ only on two mark-phase side effects, both gated by `for_marking`:
    ///
    /// - **`for_marking = true`** (mark phase, from `arc_heap`'s mark loop): additionally
    ///   marks the variable-length element backings in place (`mark_backing`, so the
    ///   `region_var` element block stays live this cycle), and surfaces a closure's env
    ///   *array header* (`Value::Array`) + its `fn_name` GC string as children so the mark
    ///   loop marks those blocks too.
    /// - **`for_marking = false`** (enumeration, via `ArcMagrGC::scan_object_refs`): a pure
    ///   read — no mark side effects; descends **directly** into a closure's captured refs
    ///   (the env header and `fn_name` string are internal, not surfaced as graph nodes).
    ///
    /// Primitives / stack-arena handles / struct-blob refs yield no children (their storage
    /// is scanned directly by the external root scanner, so walking here would double-count).
    /// [`Value::is_heap_ref`] is the matching predicate; this is the traversal.
    #[inline]
    pub fn visit_gc_children(&self, for_marking: bool, visit: &mut dyn FnMut(&Value)) {
        match self {
            Value::Object(rc) => {
                let obj = rc.borrow();
                // unify-object-byte-layout: side-table reference leaves (closure/func/
                // string + inline-struct interior refs) live in `refs`; PR-3 chunk 2b
                // additionally inlines direct object/array refs as 8B pointers in `bytes`,
                // scanned via `trace_inline_refs`.
                for r in &obj.refs { visit(r); }
                obj.trace_inline_refs(visit);
            }
            Value::Array(rc) => {
                let arr = rc.borrow();
                if for_marking { arr.mark_backing(); }  // unify-gc-heap PR-3: keep the element block(s) alive
                for elem in arr.gc_refs() { visit(elem); }  // add-struct-heap-inline (P3b): incl struct[] refs
            }
            Value::Closure(vref) => {
                // unify-gc-heap PR-2/PR-5: the closure's `ClosureData` is a GC block in region_var.
                // SAFETY: a reachable closure names an alive block; payload is one ClosureData.
                let data = unsafe { &*vref.payload_as_ptr::<ClosureData>() };
                if for_marking {
                    // Push the env array *header* (so the mark loop marks its region_array entry
                    // and re-traces its elements — one indirection past the pre-PR-2 behaviour)
                    // and the `fn_name` GC string (PR-5, a leaf), so both blocks stay live.
                    visit(&Value::Array(data.env.clone()));
                    visit(&Value::Str(data.fn_name));
                } else {
                    // Enumeration: descend into the captured refs directly — the env header and
                    // fn_name string are the closure's internals, not distinct graph nodes.
                    let arr = data.env.borrow();
                    for elem in arr.gc_refs() { visit(elem); }
                }
            }
            // add-boxed-struct-identity (P4b, 路 B2): 装箱 struct 是共享 `ScriptObject` → 与 Object
            // 同路追踪其 struct_refs 引用叶子（slots 空）。对象本身由 mark 循环的 BoxedStruct 臂标记。
            Value::BoxedStruct(gc) => { let obj = gc.borrow(); for r in &obj.refs { visit(r); } }
            // make-value-copy: `Ref` / `StructRefHeap` are transient-arena handles — leaves
            // here, exactly like `StructRef` / `StackObject`. Their payload's GcRefs (a
            // Ref's Array/Field target, a StructRefHeap's backing array) are scanned
            // *directly* by `TransientArena::scan_roots` (a GC root), so tracing through
            // the handle here would double-count.
            // Primitives — no children.
            // add-escape-analysis-stack-alloc: StackObject / StackArray are
            // leaves for the child-traversal — their slots/elems live in the
            // frame arena and are scanned directly as GC roots by the external
            // root scanner (mirrors StackClosure's env_arena handling), so
            // walking them here would double-count. A stack handle appearing in
            // a heap object's slot would be an escape-analysis bug; the debug
            // asserts in the store paths (FieldSet/ArraySet/StaticSet) catch it.
            // add-struct-value-semantics: StructRef is a leaf here — its blob's
            // reference leaves are scanned directly by the struct-arena root
            // scanner (mirrors StackObject), so walking here would double-count.
            Value::I64(_) | Value::F64(_) | Value::Bool(_) | Value::Char(_)
            | Value::Str(_) | Value::Null | Value::FuncRef(_)
            | Value::PinnedView { .. } | Value::StackClosure { .. }
            | Value::Ref { .. } | Value::StructRefHeap { .. }
            | Value::StackObject { .. } | Value::StackArray { .. }
            | Value::StructRef { .. } => {}
        }
    }

    /// GC mark-phase traversal — thin wrapper over [`Value::visit_gc_children`] with
    /// `for_marking = true`. `#[inline]` so the constant flag folds away on the hot
    /// mark loop (identical codegen to the pre-convergence dedicated match).
    #[inline]
    pub fn trace_children(&self, visit: &mut dyn FnMut(&Value)) {
        self.visit_gc_children(true, visit);
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
            (Value::Array(a),  Value::Array(b))  => GcRef::ptr_eq(a, b),
            (Value::Object(a), Value::Object(b)) => GcRef::ptr_eq(a, b),
            // make-value-copy: `PinnedView` / `Ref` (and `StructRefHeap` / `StackClosure`)
            // are now transient-arena handles — compare by `{idx, frame_id}` handle
            // identity (same as `StackObject`). These are internal transient values; user
            // code has no by-value-equality dependency on them (they never reach a
            // user-visible `==` — an escape sink would have materialized the heap form).
            (Value::PinnedView { idx: i1, frame_id: g1 },
             Value::PinnedView { idx: i2, frame_id: g2 }) => i1 == i2 && g1 == g2,
            (Value::Ref { idx: i1, frame_id: g1 },
             Value::Ref { idx: i2, frame_id: g2 }) => i1 == i2 && g1 == g2,
            (Value::StackClosure { idx: i1, frame_id: g1 },
             Value::StackClosure { idx: i2, frame_id: g2 }) => i1 == i2 && g1 == g2,
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
            // add-boxed-struct-identity (P4b): 载荷现是共享 `ScriptObject`——先 ptr_eq（同盒必等，且避免
            // 对同一 GcRef 二次 borrow 死锁），否则 borrow 两盒比 struct_bytes/struct_refs（保持值相等语义）。
            (Value::BoxedStruct(a), Value::BoxedStruct(b)) => {
                if GcRef::ptr_eq(a, b) {
                    true
                } else {
                    let (ao, bo) = (a.borrow(), b.borrow());
                    ao.type_desc.name == bo.type_desc.name
                        && ao.bytes == bo.bytes
                        && ao.refs == bo.refs
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
