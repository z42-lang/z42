//! Shared reference reads and writes for backend frames and builtin ref arguments.
use anyhow::Result;
use crate::{object::Value, vm_context::VmContext};

/// Spec impl-ref-out-in-runtime: dereference a `Value::Ref { kind }` into
/// the underlying value. Stack kind looks up `ctx.frame_state_at(frame_idx)`
/// (raw pointer, safe under design Decision 9: refs don't escape call
/// stack). Array/Field kinds borrow the held ArrRef and read.
pub(crate) fn deref_ref(
    kind: &crate::object::RefKind, ctx: &VmContext,
) -> Result<Value> {
    use crate::object::RefKind;
    match kind {
        RefKind::Stack { frame_idx, slot } => {
            let regs_ptr = ctx.frame_state_at(*frame_idx as usize)
                .ok_or_else(|| anyhow::anyhow!(
                    "ref points to popped frame (frame_idx={frame_idx})"))?;
            // SAFETY: spec Decision 9 — refs never escape the call stack;
            // when this deref runs, the target frame is still active.
            let regs = unsafe { &*regs_ptr };
            let v = regs.get(*slot as usize)
                .ok_or_else(|| anyhow::anyhow!(
                    "ref target slot %{slot} out of frame range"))?;
            // Sanity guard: ref-to-ref nesting not supported (codegen never
            // produces it; defend in case of malformed bytecode).
            if let Value::Ref { .. } = v {
                anyhow::bail!("ref-to-ref nesting not supported");
            }
            Ok(v.clone())
        }
        // 元素 / 字段的读取走 objops（与 ArrayGet / FieldGet 同一份存储访问）。
        RefKind::Array { gc_ref, idx } =>
            crate::objops::array::elem_load(gc_ref, *idx).map_err(crate::objops::OpError::into_anyhow),
        RefKind::Field { gc_ref, field_name } =>
            crate::objops::field::load_named(gc_ref, field_name).map_err(crate::objops::OpError::into_anyhow),
    }
}

/// Spec impl-ref-out-in-runtime: store a value through a `Value::Ref` to
/// the underlying location. Mirror of `deref_ref` for the write path.
pub(crate) fn store_thru_ref(
    kind: &crate::object::RefKind, val: Value, ctx: &VmContext,
) -> Result<()> {
    use crate::object::RefKind;
    match kind {
        RefKind::Stack { frame_idx, slot } => {
            let regs_ptr = ctx.frame_state_at(*frame_idx as usize)
                .ok_or_else(|| anyhow::anyhow!(
                    "ref points to popped frame (frame_idx={frame_idx})"))?;
            // SAFETY: same as deref_ref; the frame is still active.
            // We need *mut here — cast from *const Vec<Value>. The frame's
            // regs Vec is borrowed from `Frame` which is `&mut` during exec
            // of its instructions, so the Vec is uniquely owned by that
            // frame. Cross-frame mutation requires us to coerce to mut.
            let regs = unsafe { &mut *(regs_ptr as *mut Vec<Value>) };
            let slot_idx = *slot as usize;
            if slot_idx >= regs.len() {
                regs.resize(slot_idx + 1, Value::Null);
            }
            regs[slot_idx] = val;
            Ok(())
        }
        // 经 ref 写元素 / 字段与 ArraySet / FieldSet 是同一种写入：同一份 objops 原语，
        // 写屏障与基元槽类型检查都在里面（fix-missing-array-write-barriers / fix-ref-field-write-barrier）。
        RefKind::Array { gc_ref, idx } =>
            crate::objops::array::elem_store(ctx, gc_ref, *idx, &val).map_err(crate::objops::OpError::into_anyhow),
        RefKind::Field { gc_ref, field_name } =>
            crate::objops::field::store_named(ctx, gc_ref, field_name, &val).map_err(crate::objops::OpError::into_anyhow),
    }
}

