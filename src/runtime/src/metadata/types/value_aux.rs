//! Value 周边：StructArrayElem / RefKind / Pin / Closure 数据 / ExecMode / ObjectData。refactor-split-metadata-types（2026-09-03）：从 2436 行的 `types.rs` 按职责拆出，
//! 对外路径不变（`metadata::types::*` 经 hub 的 `pub use` 全量再导出）。

#![allow(unused_imports)]
use super::*;
use std::sync::Arc;
use crate::metadata::vstr::Str;
use crate::gc::GcRef;
use crate::gc::var_region::{BlockType, VarGcRef};
use crate::gc::heap::MagrGC;

/// add-struct-heap-inline (P3b): payload of [`Value::StructRefHeap`] — a value-struct
/// array element identity (`arr[index]`). Holds the array `GcRef` (so the handle keeps
/// the array alive + GC can reach the element's reference leaves) + the element index.
#[derive(Debug, Clone)]
pub struct StructArrayElem {
    pub arr: GcRef<ArrayObj>,
    pub index: u32,
}

// add-boxed-struct-identity (P4b, 路 B2): `BoxedStructData` 已删——装箱 struct 的 blob 现内联在其
// 共享 `ScriptObject` 的 `struct_bytes`/`struct_refs`（`type_desc.is_struct()` 的对象）。装箱经
// `corelib::convert::builtin_box_struct`（alloc struct 类型 ScriptObject）；拆箱经
// `interp::exec_struct::unbox_struct`（读对象 struct_bytes/refs → arena StructRef）。

// add-primitive-value-boxing → unify Phase 2 R3: `BoxedPrim` 已删——基元装箱统一到堆
// `ScriptObject`（整数标量存 `struct_bytes`）+ `Value::BoxedStruct`，见
// `ScriptObject::boxed_prim_i64` / `corelib::convert::box_prim_to_heap`。

/// Spec impl-ref-out-in-runtime: 描述 `Value::Ref` 指向的底层位置类型。
#[derive(Debug, Clone)]
pub enum RefKind {
    /// 指向 caller 调用栈第 `frame_idx` 层 frame 的 reg[`slot`]。
    /// `frame_idx` 是 `VmContext.frame_state_at` 列表索引。
    Stack { frame_idx: u32, slot: u32 },
    /// 指向 caller 数组对象的 `idx` 元素。GcRef 持有数组，让 GC 跟随。
    Array { gc_ref: GcRef<ArrayObj>, idx: usize },
    /// 指向 caller 对象的命名字段。
    Field { gc_ref: GcRef<ScriptObject>, field_name: String },
}

/// Origin of a [`Value::PinnedView`]. Recorded for diagnostics; both kinds
/// share the same wire form (raw bytes + length).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinSourceKind {
    Str,
    ArrayU8,
}

/// Payload of [`Value::PinnedView`] — boxed (review.md C1 step 1,
/// 2026-05-27) so the inline `Value` doesn't pay for the 24-byte raw
/// FFI view triple. `PinPtr` constructs one; `UnpinPtr` and any
/// `FieldGet` reading `.ptr` / `.len` borrow through the box.
#[derive(Debug, Clone)]
pub struct PinnedViewData {
    pub ptr:  u64,
    pub len:  u64,
    pub kind: PinSourceKind,
}

/// Payload of [`Value::StackClosure`] — boxed (review.md C1 chunk 3,
/// 2026-05-27) so the inline `Value` doesn't pay for the env-idx + fn
/// name pair. `MkClos` with stack-alloc=1 constructs one; `CallIndirect`
/// is the sole consumer.
#[derive(Debug, Clone)]
pub struct StackClosureData {
    pub env_idx: u32,
    pub fn_name: String,
}

/// Payload of [`Value::Closure`] — lives in the GC variable-length region
/// (`region_var`, `BlockType::Closure`). `MkClos` (heap-alloc path) constructs
/// one; `CallIndirect`, `__delegate_target`, `__delegate_fn_name`,
/// `__delegate_eq` and the GC scanner consume.
///
/// **unify-gc-heap PR-5**: `fn_name` migrated `String` → GC [`Str`] (8B handle),
/// so `ClosureData` now owns **no heap memory outside the GC** — both fields are
/// trivially-droppable (`GcRef`/`Str` have no-op/`Copy` drops). The block is a
/// POD leaf for finalization → the `BlockType::Closure` drop-glue arm is gone
/// (see `gc::arc_heap::var_drop_glue`). Both edges (`env` array, `fn_name`
/// string) are traced by [`Value::trace_children`].
#[derive(Debug, Clone)]
pub struct ClosureData {
    pub env: GcRef<ArrayObj>,
    pub fn_name: Str,
}

/// unify-gc-heap PR-2: read the [`ClosureData`] behind a closure's `VarGcRef` handle (the
/// payload of a `Value::Closure`). Mirrors [`Value::closure_data`] for call sites that have
/// already destructured `Value::Closure(vref)`.
///
/// Safe-signatured (like `Value::closure_data`) on the **liveness invariant**: `vref` must be a
/// live closure handle — true at every call site, which obtains it from a reachable
/// `Value::Closure`. The `ClosureData` is immutable after creation, so the shared borrow needs
/// no lock.
#[inline]
pub fn closure_data_of(vref: &VarGcRef) -> &ClosureData {
    // SAFETY: `vref` names an alive `Closure` block (caller invariant); the payload is exactly
    // one immutable `ClosureData`, valid for the returned borrow.
    unsafe { &*vref.payload_as_ptr::<ClosureData>() }
}
/// Execution mode for a module or function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExecMode {
    /// Tree-walking / bytecode interpreter — fast startup, no warmup cost.
    Interp,
    /// Just-in-time compilation — best steady-state throughput.
    Jit,
    /// Ahead-of-time compilation — best for predictable, startup-sensitive code.
    Aot,
}

impl Default for ExecMode {
    fn default() -> Self {
        ExecMode::Interp
    }
}

// ── Backward compatibility alias ─────────────────────────────────────────────

/// Deprecated alias kept so external code using `ObjectData` by name continues
/// to compile during the transition.  New code should use `ScriptObject`.
#[deprecated(note = "use ScriptObject instead")]
pub type ObjectData = ScriptObject;
