//! Payloads for references, pinned native views and closures.

use crate::object::{ArrRef, header::VarGcRef};

/// Identity of a value-struct array element (`arr[index]`). The reachable
/// [`crate::object::Value::StructRefHeap`] payload traces the array and its leaves.
#[derive(Debug, Clone)]
pub struct StructArrayElem {
    pub arr: ArrRef,
    pub index: u32,
}

/// The underlying location named by a [`crate::object::Value::Ref`].
#[derive(Debug, Clone)]
pub enum RefKind {
    /// 指向 caller 调用栈第 `frame_idx` 层 frame 的 reg[`slot`]。
    /// `frame_idx` 是 `VmContext.frame_state_at` 列表索引。
    Stack { frame_idx: u32, slot: u32 },
    /// 指向 caller 数组对象的 `idx` 元素。ArrRef 持有数组，让 GC 跟随。
    Array { gc_ref: ArrRef, idx: usize },
    /// 指向 caller 对象的命名字段。
    Field { gc_ref: crate::object::ObjRef, field_name: String },
}

/// Origin of a [`crate::object::Value::PinnedView`]. Recorded for diagnostics; both kinds
/// share the same wire form (raw bytes + length).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinSourceKind {
    Str,
    ArrayU8,
}

/// Out-of-line native view payload: raw pointer, byte length and source kind.
/// `PinPtr` creates it; consumers borrow it through the pinned view value.
#[derive(Debug, Clone)]
pub struct PinnedViewData {
    pub ptr:  u64,
    pub len:  u64,
    pub kind: PinSourceKind,
}

/// Closure payload in the GC variable-length region. It owns no external heap
/// memory and needs no drop glue; [`crate::object::Value::trace_children`] traces
/// its environment array. The target uses the same bound function ID space as
/// [`crate::object::Value::FuncRef`]; reflection resolves the name from that ID.
#[derive(Debug, Clone)]
pub struct ClosureData {
    pub env: ArrRef,
    pub fn_id: crate::metadata::ids::tokens::FnId,
}

/// Read immutable closure data from a live closure handle. The caller must obtain
/// `vref` from a reachable [`crate::object::Value::Closure`], so the shared borrow
/// remains valid without a lock.
#[inline]
pub fn closure_data_of(vref: &VarGcRef) -> &ClosureData {
    // SAFETY: `vref` names an alive `Closure` block (caller invariant); the payload is exactly
    // one immutable `ClosureData`, valid for the returned borrow.
    unsafe { &*vref.payload_as_ptr::<ClosureData>() }
}
