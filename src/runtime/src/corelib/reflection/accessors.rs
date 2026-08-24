use super::*;

/// `__property_get_value(prop: PropertyInfo, target: object) -> object`.
/// Reflectively reads a property by invoking its `get_<Name>` accessor (whose
/// qualified name the VM stamped onto `__getterQualified` in
/// `builtin_type_properties`). `target` is the receiver (reg 0). A read-only
/// property (no getter) raises a catchable `Std.Exception`.
pub fn builtin_property_get_value(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let pi = args.first().cloned().unwrap_or(Value::Null);
    let target = args.get(1).cloned().unwrap_or(Value::Null);
    let getter = match read_obj_slot(&pi, "__getterQualified") {
        Value::Str(s) => s.to_string(),
        _ => bail!("PropertyInfo.GetValue: property has no getter (write-only)"),
    };
    invoke_qualified(ctx, &getter, &[target], &[])
}

/// `__property_set_value(prop: PropertyInfo, target: object, value: object)`.
/// Reflectively writes a property by invoking its `set_<Name>` accessor (whose
/// qualified name the VM stamped onto `__setterQualified`). `target` is the
/// receiver (reg 0), `value` the assigned value. A read-only property (no
/// setter) raises a catchable `Std.Exception`.
pub fn builtin_property_set_value(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let pi = args.first().cloned().unwrap_or(Value::Null);
    let target = args.get(1).cloned().unwrap_or(Value::Null);
    let value = args.get(2).cloned().unwrap_or(Value::Null);
    let setter = match read_obj_slot(&pi, "__setterQualified") {
        Value::Str(s) => s.to_string(),
        _ => bail!("PropertyInfo.SetValue: property has no setter (read-only)"),
    };
    invoke_qualified(ctx, &setter, &[target, value], &[])
}

/// `__field_get_value(field: FieldInfo, target: object) -> object` — read an
/// instance field's value straight off the target object's slot (by the field's
/// `Name` → the object's own `field_index`). Unlike `PropertyInfo.GetValue`
/// there is no accessor: a field IS a slot. Powers reflective (de)serialization.
pub fn builtin_field_get_value(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let fi = args.first().cloned().unwrap_or(Value::Null);
    let target = args.get(1).cloned().unwrap_or(Value::Null);
    let name = match read_obj_slot(&fi, "Name") {
        Value::Str(s) => s.to_string(),
        _ => bail!("FieldInfo.GetValue: receiver is not a FieldInfo"),
    };
    match &target {
        // add-boxed-struct-identity (P4b): reflect a value struct's own fields off its
        // shared box object — the field data lives byte-packed in `struct_bytes` +
        // reference-leaf side-table `struct_refs`, not in `slots`.
        Value::BoxedStruct(gc) => boxed_struct_field_get(ctx, gc, &name),
        Value::Object(rc) => {
            // add-object-inline-struct-reflection (P4b-B): an inline value-struct field
            // (`class C { Point pt; }`) has a dead placeholder slot — its bytes live in the
            // object's `struct_bytes`/`struct_refs`. Read those before the plain-slot path.
            if let Some(v) = object_inline_struct_field_get(ctx, rc, &name)? {
                return Ok(v);
            }
            match rc.type_desc().field_index.get(&name).copied() {
                Some(i) => Ok(rc.borrow().field_value(i)),
                None => bail!("FieldInfo.GetValue: field `{name}` not present on target instance"),
            }
        }
        _ => bail!("FieldInfo.GetValue: target is not an object instance"),
    }
}

/// add-object-inline-struct-reflection (P4b-B): read an inline value-struct field off a heap
/// object (`class C { Point pt; }`). The field's slot is a dead `Null` placeholder (P3b); the
/// real value is byte-packed in the object's `struct_bytes` + `struct_refs` at an
/// object-relative offset recovered by replicating the **class** inline layout
/// (`struct_reflect::compute_class_inline`, validated against the delivered `inline_layout`).
/// Returns `Ok(None)` when `name` is not an inline struct field → caller reads the plain slot.
pub(super) fn object_inline_struct_field_get(
    ctx: &VmContext, rc: &crate::gc::GcRef<crate::metadata::types::ScriptObject>, name: &str,
) -> Result<Option<Value>> {
    let class_name = rc.type_desc().name.to_string();
    let resolve = |n: &str| ctx.try_lookup_type(n);
    // `struct_field_fq` returns the field type's **fully-qualified** struct name (or
    // `None` for a primitive / reference field → ordinary slot path).
    let struct_type = match crate::corelib::struct_reflect::struct_field_fq(&resolve, &class_name, name) {
        Some(fq) => fq,
        None => return Ok(None),
    };
    // unify-object-byte-layout (PR-2): the struct field's blob lives in the object's
    // `bytes` at its composed offset (`field_access[slot]`); interior reference leaves
    // resolve through the composed object ref bitmap. Snapshot into a fresh box (value
    // semantics — mutating the returned box doesn't touch the parent).
    let slot = rc.type_desc().field_index.get(name).copied().ok_or_else(|| {
        anyhow::anyhow!("FieldInfo.GetValue: inline struct field `{name}` not on `{class_name}`")
    })?;
    let col = rc.type_desc().composed_object_layout().ok_or_else(|| {
        anyhow::anyhow!("FieldInfo.GetValue: class `{class_name}` has an inline struct field `{name}` but no object layout")
    })?;
    let fa = *col.field_access.get(slot).ok_or_else(|| {
        anyhow::anyhow!("FieldInfo.GetValue: field `{name}` slot {slot} not in object layout")
    })?;
    let nested = crate::corelib::struct_reflect::compute(&resolve, &struct_type)?;
    let composed_base = fa.offset as usize;
    let o = rc.borrow();
    let bytes = o.bytes[composed_base..composed_base + fa.width as usize].to_vec();
    let mut refs = Vec::with_capacity(nested.ref_offsets.len());
    for &nro in &nested.ref_offsets {
        let ri = col.ref_index(fa.offset + nro).ok_or_else(|| {
            anyhow::anyhow!("object inline struct ref leaf offset not in composed bitmap")
        })?;
        refs.push(o.refs[ri].clone());
    }
    Ok(Some(crate::corelib::convert::box_struct_blob(ctx, &struct_type, &bytes, &refs)?))
}

/// Snapshot a nested/inline value-struct `leaf` out of a heap object's byte region into a
/// fresh boxed struct (value semantics — mutating the returned box does not touch the parent).
/// Shared by the boxed-struct nested-field path (P4b) and the object-inline struct-field path
/// (P4b-B): both read from a `ScriptObject`'s `struct_bytes`/`struct_refs` given the *parent*
/// computed layout + the struct leaf (`parent.ref_index` maps a nested ref leaf's
/// object-relative offset to its `struct_refs` slot).
pub(super) fn snapshot_struct_leaf(
    ctx: &VmContext,
    o: &crate::metadata::types::ScriptObject,
    parent: &crate::corelib::struct_reflect::ComputedLayout,
    leaf: &crate::corelib::struct_reflect::FieldLeaf,
) -> Result<Value> {
    let resolve = |n: &str| ctx.try_lookup_type(n);
    let start = leaf.byte_off as usize;
    let size = leaf.size as usize;
    let nested = crate::corelib::struct_reflect::compute(&resolve, &leaf.type_name)?;
    let bytes = o.bytes[start..start + size].to_vec();
    let mut refs = Vec::with_capacity(nested.ref_offsets.len());
    for &nro in &nested.ref_offsets {
        let ri = parent.ref_index(leaf.byte_off + nro).ok_or_else(|| {
            anyhow::anyhow!("struct field reflection: nested ref leaf offset not in parent bitmap")
        })?;
        refs.push(o.refs[ri].clone());
    }
    crate::corelib::convert::box_struct_blob(ctx, &leaf.type_name, &bytes, &refs)
}

/// add-boxed-struct-identity (P4b): read field `name` out of a boxed value struct. Replicates
/// the struct's byte layout (`struct_reflect::compute`, validated against the delivered
/// `struct_layout`), then decodes the leaf: primitive → `decode_prim` off `struct_bytes`;
/// reference → the `struct_refs` side-table; nested struct → a fresh boxed snapshot (value
/// semantics — mutating the returned box does not touch the parent).
pub(super) fn boxed_struct_field_get(
    ctx: &VmContext, gc: &crate::gc::GcRef<crate::metadata::types::ScriptObject>, name: &str,
) -> Result<Value> {
    use crate::interp::exec_struct::{decode_prim, prim_width};
    let type_name = gc.type_desc().name.to_string();
    let resolve = |n: &str| ctx.try_lookup_type(n);
    let comp = crate::corelib::struct_reflect::compute(&resolve, &type_name)?;
    if let Some(sl) = gc.type_desc().struct_layout() {
        comp.validate_against(&sl, &type_name)?;
    } else {
        bail!("FieldInfo.GetValue: type `{type_name}` has no delivered struct layout");
    }
    let leaf = comp.field(name).ok_or_else(|| {
        anyhow::anyhow!("FieldInfo.GetValue: field `{name}` not present on `{type_name}`")
    })?;
    if leaf.is_struct {
        // Nested value-struct field → boxed snapshot (shared with the object-inline path).
        let o = gc.borrow();
        snapshot_struct_leaf(ctx, &o, &comp, leaf)
    } else if leaf.is_ref {
        let ri = comp.ref_index(leaf.byte_off).ok_or_else(|| {
            anyhow::anyhow!("FieldInfo.GetValue: reference leaf offset not in bitmap")
        })?;
        Ok(gc.borrow().refs.get(ri).cloned().unwrap_or(Value::Null))
    } else {
        let w = prim_width(leaf.tag)?;
        decode_prim(&gc.borrow().bytes, leaf.byte_off as usize, w, leaf.tag)
    }
}

/// `__field_set_value(field: FieldInfo, target: object, value: object)` — write
/// an instance field's slot directly (by `Name` → `field_index`). Powers
/// reflective deserialization (binding JSON members onto plain public fields).
pub fn builtin_field_set_value(ctx: &VmContext, args: &[Value]) -> Result<Value> {
    let fi = args.first().cloned().unwrap_or(Value::Null);
    let target = args.get(1).cloned().unwrap_or(Value::Null);
    let value = args.get(2).cloned().unwrap_or(Value::Null);
    let name = match read_obj_slot(&fi, "Name") {
        Value::Str(s) => s.to_string(),
        _ => bail!("FieldInfo.SetValue: receiver is not a FieldInfo"),
    };
    match &target {
        // add-boxed-struct-identity (P4b): write through to the shared box object. Because
        // a boxed struct now has reference identity (a shared `ScriptObject`), the mutation
        // is visible to every holder of the box — matching C# `FieldInfo.SetValue(box, v)`.
        Value::BoxedStruct(gc) => boxed_struct_field_set(ctx, &target, gc, &name, &value),
        Value::Object(rc) => {
            // add-object-inline-struct-reflection (P4b-B): an inline value-struct field
            // writes through to the object's shared `struct_bytes`/`struct_refs`, not a slot.
            if object_inline_struct_field_set(ctx, &target, rc, &name, &value)?.is_some() {
                return Ok(Value::Null);
            }
            match rc.type_desc().field_index.get(&name).copied() {
                Some(i) => {
                    rc.borrow_mut().set_field_value(i, &value);
                    Ok(Value::Null)
                }
                None => bail!("FieldInfo.SetValue: field `{name}` not present on target instance"),
            }
        }
        _ => bail!("FieldInfo.SetValue: target is not an object instance"),
    }
}

/// add-object-inline-struct-reflection (P4b-B): write a boxed value struct into an inline
/// struct field of a heap object, in place on the object's shared byte region (visible to
/// every holder — C# reference identity). Returns `Ok(None)` when `name` is not an inline
/// struct field → caller writes the plain slot.
pub(super) fn object_inline_struct_field_set(
    ctx: &VmContext, target: &Value,
    rc: &crate::gc::GcRef<crate::metadata::types::ScriptObject>, name: &str, value: &Value,
) -> Result<Option<()>> {
    let class_name = rc.type_desc().name.to_string();
    let resolve = |n: &str| ctx.try_lookup_type(n);
    // `struct_field_fq` returns the field type's fully-qualified struct name.
    let struct_type = match crate::corelib::struct_reflect::struct_field_fq(&resolve, &class_name, name) {
        Some(fq) => fq,
        None => return Ok(None), // primitive / reference field → ordinary slot
    };
    let src = match value {
        Value::BoxedStruct(s) => s,
        other => bail!(
            "FieldInfo.SetValue: inline field `{name}` is a value struct; expected a boxed struct, got {other:?}"
        ),
    };
    // unify-object-byte-layout (PR-2): copy the boxed struct into the object's `bytes`
    // at the field's composed offset, in place (visible to every holder — C# reference
    // identity); interior reference leaves write into `refs` via the composed bitmap,
    // each with a write barrier.
    let slot = rc.type_desc().field_index.get(name).copied().ok_or_else(|| {
        anyhow::anyhow!("FieldInfo.SetValue: inline struct field `{name}` not on `{class_name}`")
    })?;
    let col = rc.type_desc().composed_object_layout().ok_or_else(|| {
        anyhow::anyhow!("FieldInfo.SetValue: class `{class_name}` has an inline struct field `{name}` but no object layout")
    })?;
    let fa = *col.field_access.get(slot).ok_or_else(|| {
        anyhow::anyhow!("FieldInfo.SetValue: field `{name}` slot {slot} not in object layout")
    })?;
    let nested = crate::corelib::struct_reflect::compute(&resolve, &struct_type)?;
    let composed_base = fa.offset as usize;
    let size = fa.width as usize;
    let (src_bytes, src_refs): (Vec<u8>, Vec<Value>) = {
        let so = src.borrow();
        (so.bytes[..size.min(so.bytes.len())].to_vec(), so.refs.to_vec())
    };
    let mut ref_writes: Vec<(usize, Value)> = Vec::new();
    for (k, &nro) in nested.ref_offsets.iter().enumerate() {
        if let Some(ri) = col.ref_index(fa.offset + nro) {
            if let Some(v) = src_refs.get(k) { ref_writes.push((ri, v.clone())); }
        }
    }
    {
        let mut o = rc.borrow_mut();
        let n = size.min(src_bytes.len());
        o.bytes[composed_base..composed_base + n].copy_from_slice(&src_bytes[..n]);
        for (ri, v) in &ref_writes { o.refs[*ri] = v.clone(); }
    }
    for (ri, v) in &ref_writes {
        if v.is_heap_ref() {
            ctx.heap().write_barrier_field(target, *ri, v);
        }
    }
    Ok(Some(()))
}

/// Copy a boxed value-struct `src` into a heap object's nested/inline struct field region
/// (`parent` layout, struct `leaf`). Primitive bytes copy verbatim; each reference leaf is
/// written into the object's `struct_refs` side-table **with a write barrier** (a reference
/// stored into a heap node must be tracked for cross-gen / concurrent GC). Shared by the
/// boxed-struct nested-field write (P4b) and the object-inline struct-field write (P4b-B).
pub(super) fn write_struct_leaf(
    ctx: &VmContext, base_val: &Value,
    gc: &crate::gc::GcRef<crate::metadata::types::ScriptObject>,
    parent: &crate::corelib::struct_reflect::ComputedLayout,
    leaf: &crate::corelib::struct_reflect::FieldLeaf,
    src: &crate::gc::GcRef<crate::metadata::types::ScriptObject>,
) -> Result<()> {
    let resolve = |n: &str| ctx.try_lookup_type(n);
    let nested = crate::corelib::struct_reflect::compute(&resolve, &leaf.type_name)?;
    let start = leaf.byte_off as usize;
    let size = leaf.size as usize;
    // Snapshot the source blob first (avoid holding two borrows).
    let (src_bytes, src_refs): (Vec<u8>, Vec<Value>) = {
        let so = src.borrow();
        (so.bytes[..size.min(so.bytes.len())].to_vec(), so.refs.to_vec())
    };
    // Map nested ref leaves → the object's `struct_refs` indices (object-relative offset).
    let mut ref_writes: Vec<(usize, Value)> = Vec::new();
    for (k, &nro) in nested.ref_offsets.iter().enumerate() {
        if let Some(ri) = parent.ref_index(leaf.byte_off + nro) {
            if let Some(v) = src_refs.get(k) { ref_writes.push((ri, v.clone())); }
        }
    }
    {
        let mut o = gc.borrow_mut();
        let n = size.min(src_bytes.len());
        o.bytes[start..start + n].copy_from_slice(&src_bytes[..n]);
        for (ri, v) in &ref_writes {
            o.refs[*ri] = v.clone();
        }
    }
    // Write barriers after releasing the mutable borrow.
    for (ri, v) in &ref_writes {
        if v.is_heap_ref() {
            ctx.heap().write_barrier_field(base_val, *ri, v);
        }
    }
    Ok(())
}

/// add-boxed-struct-identity (P4b): write `value` into field `name` of a boxed value struct,
/// in place on the shared box object. Primitive → `encode_prim` into `struct_bytes`;
/// reference → the `struct_refs` side-table (+ write barrier); nested struct → copy the
/// supplied box's bytes/refs into the parent's field region.
pub(super) fn boxed_struct_field_set(
    ctx: &VmContext, base_val: &Value,
    gc: &crate::gc::GcRef<crate::metadata::types::ScriptObject>, name: &str, value: &Value,
) -> Result<Value> {
    use crate::interp::exec_struct::{encode_prim, prim_width};
    let type_name = gc.type_desc().name.to_string();
    let resolve = |n: &str| ctx.try_lookup_type(n);
    let comp = crate::corelib::struct_reflect::compute(&resolve, &type_name)?;
    if let Some(sl) = gc.type_desc().struct_layout() {
        comp.validate_against(&sl, &type_name)?;
    } else {
        bail!("FieldInfo.SetValue: type `{type_name}` has no delivered struct layout");
    }
    let leaf = comp.field(name).ok_or_else(|| {
        anyhow::anyhow!("FieldInfo.SetValue: field `{name}` not present on `{type_name}`")
    })?.clone();
    if leaf.is_struct {
        // Nested value-struct field ← the supplied box's blob (shared with the object path).
        let src = match value {
            Value::BoxedStruct(s) => s,
            other => bail!("FieldInfo.SetValue: field `{name}` is a value struct; expected a boxed struct, got {other:?}"),
        };
        write_struct_leaf(ctx, base_val, gc, &comp, &leaf, src)?;
        Ok(Value::Null)
    } else if leaf.is_ref {
        let ri = comp.ref_index(leaf.byte_off).ok_or_else(|| {
            anyhow::anyhow!("FieldInfo.SetValue: reference leaf offset not in bitmap")
        })?;
        gc.borrow_mut().refs[ri] = value.clone();
        // Write barrier: a reference stored into a heap object (the shared box).
        if value.is_heap_ref() {
            ctx.heap().write_barrier_field(base_val, ri, value);
        }
        Ok(Value::Null)
    } else {
        // The `object value` arg arrives boxed for value-type primitives (int → Std.Int32
        // box); `encode_prim` needs the raw primitive. Transparently unbox.
        // unify Phase 2 R3: 基元盒现是 `BoxedStruct`（整数标量存 struct_bytes）→ `boxed_prim_i64`
        // 拆回裸标量；非整数盒（None）保持原值（不该到此——标量 leaf 收基元）。
        let unboxed;
        let raw: &Value = match value {
            Value::BoxedStruct(s) => match s.borrow().boxed_prim_i64() {
                Some(n) => { unboxed = Value::I64(n); &unboxed }
                None => value,
            },
            other => other,
        };
        let w = prim_width(leaf.tag)?;
        let mut o = gc.borrow_mut();
        encode_prim(&mut o.bytes, leaf.byte_off as usize, w, leaf.tag, raw)?;
        Ok(Value::Null)
    }
}
