//! Struct boxing and snapshots after the caller resolves type and frame identities.
use crate::gc::Heap;
use super::{NativeData, StructArrayElem, StructTypeLayout, TypeDesc, Value};
use anyhow::Result;
use std::sync::Arc;

pub fn box_struct(heap: &Heap, td: Arc<TypeDesc>, bytes: &[u8], refs: &[Value]) -> Result<Value> {
    let obj_val = heap.alloc_object(td, Vec::new(), NativeData::None);
    match obj_val {
        Value::Object(gc) => {
            {
                let o = gc.borrow();
                let size = o.payload_len();
                // A fresh box (young, unreachable until returned): no barrier owed.
                let offs = o.type_desc.struct_layout().map(|sl| sl.ref_offsets.clone()).unwrap_or_default();
                o.write_blob(0, size, bytes, refs, &offs, &mut |_| {})?;
            }
            Ok(Value::BoxedStruct(gc))
        }
        // alloc_object returns Null under strict-OOM refusal.
        other => Ok(other),
    }
}

/// 装箱 struct 的整块快照（类型名、字节、引用叶子）——拆箱时拷进帧 arena。
pub fn snapshot_struct_box(gc: &crate::object::ObjRef) -> (Arc<str>, Vec<u8>, Vec<Value>) {
    let o = gc.borrow();
    let (bytes, refs) = o.boxed_blob();
    (Arc::from(&*o.type_desc.name), bytes, refs)
}

/// `struct[]` 元素的整块快照（字节、引用叶子、元素布局、元素类型名）——值语义拷出时用。
pub fn snapshot_struct_element(e: &StructArrayElem) -> Result<(Vec<u8>, Vec<Value>, Arc<StructTypeLayout>, Arc<str>)> {
    let arr = e.arr.borrow();
    let layout = arr.struct_layout()
        .ok_or_else(|| anyhow::anyhow!("as-cast on a non-value-struct array element"))?;
    let (bytes, refs) = arr.struct_elem(e.index as usize)
        .ok_or_else(|| anyhow::anyhow!("struct array element on a non-value-struct array"))?;
    Ok((bytes, refs, layout, arr.element_type().arc()))
}

