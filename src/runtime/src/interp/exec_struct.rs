//! add-struct-value-semantics Phase A: interp execution of blob value-type
//! instructions (StructAlloc / StructCopy / StructFieldGetPrim / StructFieldSetPrim).
//!
//! Operates on the per-context byte arena ([`super::struct_arena`]); registers
//! hold `Value::StructRef { idx, frame_id }` handles. The primitive byte<->Value
//! codec is `kind`-driven — `kind` is a `TypeTag` (`TAG_I32` / `TAG_F64` / …)
//! giving the leaf's byte width and how to decode/encode it.

use crate::metadata::types as ty;
use crate::metadata::types::Value;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};
use std::sync::Arc;

use crate::metadata::types::StructTypeLayout;
use super::Frame;

// unify-object-byte-layout (PR-2): the primitive byte<->Value codec moved to
// `metadata::types` (both object byte-storage and struct blobs consume it). Re-export
// so existing call sites here + in `corelib::reflection` keep the same path.
pub(crate) use crate::metadata::types::{decode_prim, encode_prim, prim_width, is_ref_tag};

/// `StructAlloc dst, type_name, size` — allocate a zero-initialized blob in the
/// per-context struct arena; `dst` = `Value::StructRef` handle. The blob's byte +
/// reference layout comes from the type's TYPE-section struct block (via
/// [`resolve_layout`]); pure-primitive types fall back to a `size`-only layout.
pub(super) fn struct_alloc(
    ctx: &VmContext, frame: &mut Frame, dst: u32, type_name: &str, size: u32,
) -> Result<()> {
    let v = struct_alloc_val(ctx, frame.frame_id, type_name, size);
    frame.set(dst, v);
    Ok(())
}

/// Frame-agnostic core of `StructAlloc` — allocate a zero-initialized blob in the
/// per-context struct arena stamped with `frame_id`; returns the `StructRef` handle.
/// Shared by interp ([`struct_alloc`]) and the JIT struct helpers
/// (`jit::helpers::struct_ops`) which read `frame_id` off `JitFrame`.
pub(crate) fn struct_alloc_val(
    ctx: &VmContext, frame_id: u32, type_name: &str, size: u32,
) -> Value {
    let layout = resolve_layout(ctx, type_name, size);
    let idx = ctx.struct_alloc(frame_id, Arc::from(type_name), layout);
    Value::StructRef { idx, frame_id }
}

/// Resolve a value-struct type's runtime layout (byte size + reference bitmap).
/// A-use delivers it via the loaded `TypeDesc`; a type without a delivered layout
/// (or before the TYPE-section block reaches the runtime) falls back to a
/// `size`-only pure-primitive layout — byte-for-byte the pre-A-use behavior.
///
/// ⚠️ **这条兜底在 symbolic-struct-field-access P2 之后承重方式变了。**
/// 烘焙偏移的年代，字段访问**不依赖**类型元数据；符号化之后 `resolve_field_path`
/// **必须**拿到布局才能算出偏移。所以「走了这条兜底的 blob」在 P2 之下
/// **无法被 `StructFieldGetPrim`/`SetPrim` 访问**。
///
/// 实测（2026-09-28，debug VM × 289 条 e2e 语料）：只有 **1** 条用例走到这里，
/// 类型是 `Std.GCHandle`，原因是 `why=TYPE-NOT-LOADED`。而 `GCHandle` 的
/// **每个成员都是 `[Native]`**（`Alloc`/`Target`/`IsAllocated`/`Kind`/`Free`），
/// 它的 `_slot` **没有任何 z42 代码碰** ⇒ 那个 blob 全程由 natives 按字节偏移操作
/// （Rust 侧的 `struct_field_{get,set}_val`，不经 IR 指令）⇒ 不受影响。
///
/// 若将来真有符号化访问落在这条兜底上：`resolve_field_path` 会给出
/// 「type `X` is not loaded」的**精确报错**，而不是一个错偏移 —— 响而不是静默。
pub(crate) fn resolve_layout(ctx: &VmContext, type_name: &str, size: u32) -> Arc<StructTypeLayout> {
    if let Some(td) = ctx.try_lookup_type(type_name) {
        if let Some(layout) = td.struct_layout() {
            return layout;
        }
    }
    Arc::new(StructTypeLayout {
        size: size as usize,
        ref_offsets: Box::new([]),
        ref_kinds: Box::new([]),
        // 兜底路径没有类型元数据 ⇒ 无字段表（symbolic-struct-field-access P0）。
        fields: Box::new([]),
    })
}

/// add-struct-object-boxing (PR2a): 拆箱——把堆 `BoxedStruct` 的 blob 拷回**当前帧** struct arena，
/// 返回值 struct `StructRef` 句柄（`(P)o` / `o as P` 用）。alloc 用类型布局（size 兜底自 `bytes.len()`），
/// 再 memcpy bytes + clone refs。拆出的 struct 是独立副本（改它不影响 boxed 或再次拆箱）。
pub(crate) fn unbox_struct(
    ctx: &VmContext, frame_id: u32, gc: &crate::gc::GcRef<ty::ScriptObject>,
) -> Result<Value> {
    // add-boxed-struct-identity (P4b, 路 B2): the box is a shared struct-typed
    // `ScriptObject` — snapshot its blob (`struct_bytes`/`struct_refs`) into a fresh
    // current-frame arena `StructRef` (value-semantics unbox: the arena copy is
    // independent of the box).
    let (type_name, bytes, refs): (Arc<str>, Vec<u8>, Vec<Value>) = {
        let o = gc.borrow();
        (Arc::from(&*o.type_desc.name), o.bytes().to_vec(), o.refs().to_vec())
    };
    let layout = resolve_layout(ctx, &type_name, bytes.len() as u32);
    let idx = ctx.struct_alloc(frame_id, type_name, layout);
    ctx.struct_arena.lock().with_mut(idx, frame_id, |s| {
        let n = bytes.len().min(s.bytes.len());
        s.bytes[..n].copy_from_slice(&bytes[..n]);
        let rn = refs.len().min(s.refs.len());
        s.refs[..rn].clone_from_slice(&refs[..rn]);
    })?;
    Ok(Value::StructRef { idx, frame_id })
}

/// add-struct-foreach (P3b follow-up): copy a `StructBytes`-array element out to a fresh
/// **current-frame** arena `StructRef` (a value-semantics snapshot). Used by `as_cast` when
/// a `foreach (P p in arr)` loop var (or any value-context read) receives a `StructRefHeap`
/// element handle — the loop var must be an independent copy, not an alias into the array.
/// Mirrors [`unbox_struct`] but the source is a byte-backed array element, not a boxed blob.
pub(crate) fn copy_array_elem_out(ctx: &VmContext, frame_id: u32, e: &ty::StructArrayElem) -> Result<Value> {
    let i = e.index as usize;
    let (src_bytes, src_refs, layout, tname): (Vec<u8>, Vec<Value>, Arc<StructTypeLayout>, Arc<str>) = {
        let arr = e.arr.borrow();
        // unify-gc-heap PR-3: struct[] element bytes + refs live in GC blocks — read via accessors.
        let layout = arr.struct_layout().ok_or_else(|| anyhow::anyhow!("as-cast on a non-value-struct array element"))?;
        let elem_size = layout.size;
        let rc = layout.ref_count();
        let bstart = i * elem_size;
        let bytes = arr.struct_bytes().expect("StructBytes backing");
        let refs = arr.gc_refs();
        (bytes[bstart..bstart + elem_size].to_vec(),
         refs[i * rc..i * rc + rc].to_vec(),
         layout,
         arr.element_type.clone())
    };
    let idx = ctx.struct_alloc(frame_id, tname, layout);
    ctx.struct_arena.lock().with_mut(idx, frame_id, |s| {
        let n = src_bytes.len().min(s.bytes.len());
        s.bytes[..n].copy_from_slice(&src_bytes[..n]);
        let rn = src_refs.len().min(s.refs.len());
        s.refs[..rn].clone_from_slice(&src_refs[..rn]);
    })?;
    Ok(Value::StructRef { idx, frame_id })
}

/// `StructCopy dst, src, size` — copy the `src` blob into the `dst` blob (both
/// already allocated). This is the value-semantics copy point (assign/param/return).
pub(super) fn struct_copy(
    ctx: &VmContext, frame: &mut Frame, dst: u32, src: u32, size: u32,
) -> Result<()> {
    let dst_val = frame.get(dst)?.clone();
    let src_val = frame.get(src)?.clone();
    struct_copy_val(ctx, &dst_val, &src_val, size)
}

/// Frame-agnostic core of `StructCopy` — copy the `src` blob into the `dst` blob
/// (both already arena-allocated). Shared by interp and the JIT struct helpers.
pub(crate) fn struct_copy_val(
    ctx: &VmContext, dst_val: &Value, src_val: &Value, size: u32,
) -> Result<()> {
    let (d_idx, d_fid) = as_struct_ref(dst_val, "StructCopy dst")?;
    let (s_idx, s_fid) = as_struct_ref(src_val, "StructCopy src")?;
    ctx.struct_arena.lock().copy_into(d_idx, d_fid, s_idx, s_fid, size as usize)
}

/// symbolic-struct-field-access P2 (方案 A)：把 `(root_type, 字段序号路径)` 解析成
/// 这条指令要访问的**字节偏移**。
///
/// ## 为什么 `root_type` 是这里最重要的操作数
///
/// 偏移活在**两个互不相容的编号空间**里：
///
/// | 空间 | 基准 | 运行期 base kind |
/// |---|---|---|
/// | struct 布局相对 | blob 起始 | `StructRef` · `BoxedStruct` · `StructRefHeap` |
/// | composed 对象布局相对 | 对象起始 | `Object` · `StackObject` |
///
/// zbc 1.46 之前指令携带一个**烘焙好的和**，而**里面一个字都没记是哪个空间** ——
/// 正确性靠编译器（`_isInlineStructFieldRoot`）与运行时（按 `Value` 变体分派）
/// 各自独立地同意。那是审计 R2「判据复制」的实例。
///
/// ⭐ **`root_type` 本身就是判别器**：名字解析出来是 class ⇒ 第一级走对象合成布局；
/// 是 struct ⇒ 走 blob 布局。不需要额外的标志位。
///
/// ## 第 2 级起恒走 struct
///
/// 编译器在**非内联**的链节处会断链（`fix-generic-struct-chain-access`：泛型 struct 的
/// 型参字段擦除成引用叶子，存储不在容器里），所以一条路径内部的每一节都**真内联**。
/// 把 `TypeDesc.fields[i].type_tag`（**声明拼写**）解析成注册表键（**FQ**）。
///
/// 🔴 两者不是一回事，而这正是 P2 第一版栽的地方：字段表里 `Demo.Loc<P2,int>` 的第 0 个字段
/// 其 `type_tag` 是 **`P2`**，而注册表键是 **`Demo.P2`** ⇒ 路径第 2 跳报「type `P2` is not loaded」。
/// （同一族教训：**不能走类型拼写** —— 那串同时是声明记法与查找键，两边规则不同。）
///
/// 规则镜像编译器的 `ExprEmitter._qualifyInstName` / `ClassDescBuilder` 那条
/// **「只限定基名、实参原样」**：`Pair<int,long>` 在 `Demo.*` 下 ⇒ `Demo.Pair<int,long>`，
/// 泛型实参**不**跟着限定（编译器写描述符名时就是这么定的，两侧必须同一约定）。
fn qualify_field_type(ctx: &VmContext, tag: &str, owner_fq: &str) -> Option<std::sync::Arc<str>> {
    // 已经能直接查到（本来就是 FQ、或是基元）⇒ 原样。
    if ctx.try_lookup_type(tag).is_some() {
        return Some(tag.into());
    }
    // ⚠️ 声明拼写里的**空白**：`Loc<P2, long>`（逗号后有空格）对应的注册表键是
    // `Demo.Loc<P2,long>`（无空格）。类型名里不存在有意义的空格，所以去掉全部空格是安全的
    // ——这是第三处「同一个类型、两种拼写」，前两处是「短名 vs FQ」与「实参限定与否」。
    let tag: std::borrow::Cow<str> = if tag.contains(' ') {
        std::borrow::Cow::Owned(tag.replace(' ', ""))
    } else {
        std::borrow::Cow::Borrowed(tag)
    };
    let tag: &str = &tag;
    if ctx.try_lookup_type(tag).is_some() {
        return Some(tag.into());
    }
    // owner 的命名空间 = 其**基名**（去掉泛型实参）里最后一个 `.` 之前的部分。
    let base_end = owner_fq.find('<').unwrap_or(owner_fq.len());
    let ns = owner_fq[..base_end].rfind('.')?;
    let ns = &owner_fq[..ns];
    // 只限定基名：`Pair<int,long>` → `Demo.Pair<int,long>`。
    let tag_base_end = tag.find('<').unwrap_or(tag.len());
    let candidate = format!("{ns}.{}{}", &tag[..tag_base_end], &tag[tag_base_end..]);
    ctx.try_lookup_type(&candidate).map(|_| candidate.as_str().into())
}

pub(crate) fn resolve_field_path(ctx: &VmContext, root_type: &str, path: &[u16]) -> Result<u32> {
    debug_assert!(!path.is_empty(), "decoder rejects depth 0");
    let mut off: u32 = 0;
    let mut cur: std::sync::Arc<str> = root_type.into();
    for (level, &idx) in path.iter().enumerate() {
        let td = ctx.try_lookup_type(&cur).ok_or_else(|| {
            anyhow::anyhow!(
                "struct field path: type `{cur}` (level {level} of `{root_type}`) is not loaded"
            )
        })?;
        let i = idx as usize;
        // 第 1 级且 root 是 class ⇒ 对象合成布局；其余一律 blob 布局。
        let step = if level == 0 && !td.is_struct() {
            let col = td.composed_object_layout().ok_or_else(|| {
                anyhow::anyhow!("struct field path: class `{cur}` has no composed object layout")
            })?;
            col.field_offsets.get(i).copied().ok_or_else(|| {
                anyhow::anyhow!(
                    "struct field path: field index {i} out of range for class `{cur}` \
                     ({} field(s))",
                    col.field_offsets.len()
                )
            })?
        } else {
            let sl = td.struct_layout().ok_or_else(|| {
                anyhow::anyhow!("struct field path: type `{cur}` has no struct layout")
            })?;
            sl.field_offset(i).ok_or_else(|| {
                anyhow::anyhow!(
                    "struct field path: field index {i} out of range for struct `{cur}` \
                     ({} field(s))",
                    sl.field_count()
                )
            })?
        };
        off += step;
        // 下一跳的类型名走 `TypeDesc.fields[i].type_tag` —— 它与上面那张偏移表
        // **同序平行**（zbc 1.45 的约定，载入期有 `debug_assert` 守着，见 T1）。
        if level + 1 < path.len() {
            let next = td.fields.get(i).ok_or_else(|| {
                anyhow::anyhow!(
                    "struct field path: `{cur}` has no field #{i} to continue the path through \
                     — the offset table and `fields` are out of step"
                )
            })?;
            let tag = next.type_tag.clone();
            cur = qualify_field_type(ctx, &tag, &cur).ok_or_else(|| {
                anyhow::anyhow!(
                    "struct field path: field #{i} of `{cur}` is declared `{tag}`, which resolves \
                     to no loaded type (tried it as-is and qualified with `{cur}`'s namespace)"
                )
            })?;
        }
    }
    Ok(off)
}

/// A 白送的那条对账：`root_type` 声明的编号空间必须与运行期 `base` 的实际形态一致。
///
/// 这在 zbc 1.46 之前**无从做起** —— 指令里没有类型名，运行时只能按 `Value` 变体
/// 自己猜该用哪套布局。错配的后果不是崩，是**按错的基准算偏移**（静默错值）。
pub(crate) fn check_base_space(ctx: &VmContext, root_type: &str, base_val: &Value, who: &str) -> Result<()> {
    let root_is_struct = match ctx.try_lookup_type(root_type) {
        Some(td) => td.is_struct(),
        // 类型还没加载：路径解析那一步会给出更准确的诊断，这里不抢着报。
        None => return Ok(()),
    };
    let base_is_struct_space = match base_val {
        Value::StructRef { .. } | Value::StructRefHeap { .. } => Some(true),
        Value::Object(_) | Value::StackObject { .. } => Some(false),
        // `BoxedStruct` 走的是 struct 空间，但它由 `Value::Object` 承载（装箱的 struct
        // 是个 struct 类型的 `ScriptObject`）——由 root_type 说了算，不在这里判。
        _ => None,
    };
    if let Some(base_is_struct) = base_is_struct_space {
        // 装箱 struct：base 是 Object 但 root 是 struct —— 合法，跳过。
        let boxed_struct = root_is_struct && !base_is_struct && matches!(base_val, Value::Object(_));
        if !boxed_struct && root_is_struct != base_is_struct {
            anyhow::bail!(
                "{who}: the instruction says the path is rooted at `{root_type}` (a {}), but the \
                 base register holds {base_val:?} — the two disagree about which offset \
                 numbering space applies (blob-relative vs composed-object-relative), so the \
                 resolved offset would be measured from the wrong origin.",
                if root_is_struct { "value struct" } else { "class" },
            );
        }
    }
    Ok(())
}

/// 对账 + 解析的**唯一**入口，interp 与 JIT helper 共用。
///
/// 顺序刻意是「先对账、后解析」：错的编号空间下算出来的偏移是个**看起来合法的数**，
/// 先解析再对账等于把最有信息量的那个诊断让给一个更晚、更远的失败。
#[inline]
pub(crate) fn resolve_for_access(
    ctx: &VmContext, root_type: &str, path: &[u16], base_val: &Value, who: &str,
) -> Result<u32> {
    check_base_space(ctx, root_type, base_val, who)?;
    resolve_field_path(ctx, root_type, path)
}

/// `StructFieldGetPrim dst, base, (root_type, path), kind` — read the named leaf of
/// `base` into `dst`. A primitive `kind` decodes bytes; a reference `kind`
/// (`string`/object/array) reads the `Value` from the reference side-slice.
///
/// `base` may be a frame-scoped **arena** `StructRef` (local/param/temp struct), a
/// heap `Value::Object` whose inline struct field lives in
/// `ScriptObject::struct_bytes`/`struct_refs` (add-struct-heap-inline P3b, route α),
/// a `StackObject`, a `BoxedStruct`, or a `StructRefHeap` array element.
pub(super) fn struct_field_get_prim(
    ctx: &VmContext, frame: &mut Frame,
    insn: &crate::metadata::bytecode::StructFieldGetInsn,
) -> Result<()> {
    let base_val = frame.get(insn.base)?.clone();
    let byte_off = resolve_for_access(ctx, &insn.root_type, &insn.path, &base_val, "StructFieldGetPrim")?;
    let val = struct_field_get_val(ctx, &base_val, byte_off, insn.kind)?;
    frame.set(insn.dst, val);
    Ok(())
}

/// Frame-agnostic core of `StructFieldGetPrim` — read the leaf at `byte_off` of the
/// `base_val` struct (base = arena `StructRef` / heap `Object` inline field /
/// `StructRefHeap` array element). Shared by interp and the JIT struct helpers.
pub(crate) fn struct_field_get_val(
    ctx: &VmContext, base_val: &Value, byte_off: u32, kind: u8,
) -> Result<Value> {
    let val = match base_val {
        // unify-object-byte-layout (PR-2): inline struct leaf of a heap object — read
        // from the object's `bytes`/`refs` via the composed object layout. `byte_off`
        // is the compiler-baked **composed** object-relative offset (task 2.6).
        Value::Object(gc) => {
            let obj = gc.borrow();
            if is_ref_tag(kind) {
                let col = obj.type_desc.composed_object_layout().ok_or_else(|| {
                    anyhow::anyhow!("StructFieldGetPrim: object `{}` has no object layout", obj.type_desc.name)
                })?;
                let ri = col.ref_index(byte_off).ok_or_else(|| {
                    anyhow::anyhow!("inline struct ref leaf at byte offset {byte_off} not in object layout")
                })?;
                obj.refs()[ri].clone()
            } else {
                let off = byte_off as usize;
                let w = prim_width(kind)?;
                decode_prim(&obj.bytes(), off, w, kind)?
            }
        }
        // fix-stackobj-inline-struct-leaf: same as `Value::Object` above, but the object
        // lives in the **stack arena** (escape analysis stack-allocated it). Identical
        // layout and `byte_off` semantics — a stack object is the same type descriptor,
        // just a different home. Without this arm the read fell through to
        // `as_struct_ref` and bailed `expected a struct value (StructRef), got StackObject`.
        Value::StackObject { idx, frame_id } => {
            let (idx, frame_id) = (*idx, *frame_id);
            ctx.stack_arena.lock().with_obj(idx, frame_id, |obj| {
                if is_ref_tag(kind) {
                    let col = obj.type_desc.composed_object_layout().ok_or_else(|| {
                        anyhow::anyhow!("StructFieldGetPrim: stack object `{}` has no object layout", obj.type_desc.name)
                    })?;
                    let ri = col.ref_index(byte_off).ok_or_else(|| {
                        anyhow::anyhow!("inline struct ref leaf at byte offset {byte_off} not in object layout")
                    })?;
                    Ok(obj.refs()[ri].clone())
                } else {
                    let w = prim_width(kind)?;
                    decode_prim(&obj.bytes(), byte_off as usize, w, kind)
                }
            })??
        }
        // add-static-struct-bytecization (PR-2 S): leaf of a **boxed** value struct (a
        // static struct field is stored as a `BoxedStruct` for process-lifetime +
        // reference identity — `Holder.P.X` mutates it in place). The box's `bytes`/`refs`
        // ARE the struct blob (**struct** layout, not composed object layout); `byte_off`
        // is the struct-relative leaf offset (`FieldByteOffset`).
        Value::BoxedStruct(gc) => {
            let obj = gc.borrow();
            if is_ref_tag(kind) {
                let sl = obj.type_desc.struct_layout().ok_or_else(|| {
                    anyhow::anyhow!("StructFieldGetPrim: boxed struct `{}` has no struct layout", obj.type_desc.name)
                })?;
                let ri = sl.ref_index(byte_off).ok_or_else(|| {
                    anyhow::anyhow!("boxed struct ref leaf at byte offset {byte_off} not in struct layout")
                })?;
                obj.refs()[ri].clone()
            } else {
                let off = byte_off as usize;
                let w = prim_width(kind)?;
                decode_prim(&obj.bytes(), off, w, kind)?
            }
        }
        // add-struct-heap-inline (P3b, D1-a): leaf of a struct[] element `arr[index]`.
        // make-value-copy: resolve the StructRefHeap handle → StructArrayElem via the arena.
        Value::StructRefHeap { idx, frame_id } => {
            let e = ctx.transient_arena.lock().struct_elem(*idx, *frame_id)?;
            let arr = e.arr.borrow();
            // unify-gc-heap PR-3: struct[] element bytes/refs live in GC blocks — read via accessors.
            let layout = arr.struct_layout()
                .ok_or_else(|| anyhow::anyhow!("StructFieldGetPrim: StructRefHeap base is not a value-struct array"))?;
            let i = e.index as usize;
            if is_ref_tag(kind) {
                let rc = layout.ref_count();
                let ri = layout.ref_index(byte_off).ok_or_else(|| {
                    anyhow::anyhow!("struct[] ref leaf at byte offset {byte_off} not in element layout")
                })?;
                arr.gc_refs()[i * rc + ri].clone()
            } else {
                let off = i * layout.size + byte_off as usize;
                let w = prim_width(kind)?;
                decode_prim(arr.struct_bytes().expect("StructBytes backing"), off, w, kind)?
            }
        }
        _ => {
            let (idx, fid) = as_struct_ref(base_val, "StructFieldGetPrim base")?;
            if is_ref_tag(kind) {
                ctx.struct_arena.lock().get_ref(idx, fid, byte_off)?
            } else {
                let off = byte_off as usize;
                let w = prim_width(kind)?;
                ctx.struct_arena.lock().with(idx, fid, |s| decode_prim(&s.bytes, off, w, kind))??
            }
        }
    };
    Ok(val)
}

/// `StructFieldSetPrim base, byte_off, kind, val` — write `val` into the `base`
/// struct at `byte_off` (in place; the value-struct lvalue write). A primitive
/// `kind` encodes bytes; a reference `kind` stores the `Value` into the reference
/// side-slice.
///
/// Arena base: no write barrier (the arena is a GC root, re-scanned every cycle).
/// Heap-object base (P3b): a reference-leaf write into `struct_refs` **does** need a
/// write barrier — the heap object is not re-scanned as a root, so a concurrent /
/// generational collector must observe the store (routed through `write_barrier_field`).
pub(super) fn struct_field_set_prim(
    ctx: &VmContext, frame: &mut Frame,
    insn: &crate::metadata::bytecode::StructFieldSetInsn,
) -> Result<()> {
    let base_val = frame.get(insn.base)?.clone();
    let byte_off = resolve_for_access(ctx, &insn.root_type, &insn.path, &base_val, "StructFieldSetPrim")?;
    let v = frame.get(insn.val)?.clone();
    struct_field_set_val(ctx, &base_val, byte_off, insn.kind, &v)
}

/// Frame-agnostic core of `StructFieldSetPrim` — write `v` into the `base_val`
/// struct's leaf at `byte_off` in place (base = arena `StructRef` / heap `Object`
/// inline field / `StructRefHeap` array element). Heap bases route reference-leaf
/// writes through a write barrier. Shared by interp and the JIT struct helpers.
pub(crate) fn struct_field_set_val(
    ctx: &VmContext, base_val: &Value, byte_off: u32, kind: u8, v: &Value,
) -> Result<()> {
    match base_val {
        // unify-object-byte-layout (PR-2): inline struct leaf of a heap object — write
        // into the object's `bytes`/`refs` via the composed object layout. `byte_off`
        // is the compiler-baked composed object-relative offset (task 2.6).
        Value::Object(gc) => {
            if is_ref_tag(kind) {
                let ri = {
                    let mut obj = gc.borrow_mut();
                    let col = obj.type_desc.composed_object_layout().ok_or_else(|| {
                        anyhow::anyhow!("StructFieldSetPrim: object `{}` has no object layout", obj.type_desc.name)
                    })?;
                    let ri = col.ref_index(byte_off).ok_or_else(|| {
                        anyhow::anyhow!("inline struct ref leaf at byte offset {byte_off} not in object layout")
                    })?;
                    obj.set_ref_slot(ri, v);
                    ri
                };
                // Write barrier: reference stored into a heap object. The `slot`
                // argument is informational (card/diagnostics); the ref index is a
                // stable per-object identifier. STW mode = no-op.
                if v.is_heap_ref() {
                    ctx.heap().write_barrier_field(base_val, ri, v);
                }
                Ok(())
            } else {
                let off = byte_off as usize;
                let w = prim_width(kind)?;
                let mut obj = gc.borrow_mut();
                encode_prim(&mut obj.bytes_mut(), off, w, kind, v)
            }
        }
        // fix-stackobj-inline-struct-leaf: same as `Value::Object` above, but the object
        // lives in the **stack arena** (escape analysis stack-allocated it).
        //
        // 🔴 **No write barrier**, unlike the heap-object arm — and that asymmetry is
        // deliberate, mirroring `exec_object.rs::field_set`: a stack object is not a heap
        // slot, and its heap-ref fields are kept live by **root-scanning the arena** every
        // cycle. Adding a barrier here would be harmless-but-wrong (it would card-mark a
        // non-heap address).
        Value::StackObject { idx, frame_id } => {
            let (idx, frame_id) = (*idx, *frame_id);
            ctx.stack_arena.lock().with_obj_mut(idx, frame_id, |obj| {
                if is_ref_tag(kind) {
                    let ri = {
                        let col = obj.type_desc.composed_object_layout().ok_or_else(|| {
                            anyhow::anyhow!("StructFieldSetPrim: stack object `{}` has no object layout", obj.type_desc.name)
                        })?;
                        col.ref_index(byte_off).ok_or_else(|| {
                            anyhow::anyhow!("inline struct ref leaf at byte offset {byte_off} not in object layout")
                        })?
                    };
                    obj.set_ref_slot(ri, v);
                    Ok(())
                } else {
                    let w = prim_width(kind)?;
                    encode_prim(&mut obj.bytes_mut(), byte_off as usize, w, kind, v)
                }
            })?
        }
        // add-static-struct-bytecization (PR-2 S): leaf write into a **boxed** value
        // struct in place (static struct field `Holder.P.X = 5`; the box has reference
        // identity so the mutation persists). Struct layout + struct-relative `byte_off`.
        Value::BoxedStruct(gc) => {
            if is_ref_tag(kind) {
                let ri = {
                    let mut obj = gc.borrow_mut();
                    let sl = obj.type_desc.struct_layout().ok_or_else(|| {
                        anyhow::anyhow!("StructFieldSetPrim: boxed struct `{}` has no struct layout", obj.type_desc.name)
                    })?;
                    let ri = sl.ref_index(byte_off).ok_or_else(|| {
                        anyhow::anyhow!("boxed struct ref leaf at byte offset {byte_off} not in struct layout")
                    })?;
                    obj.set_ref_slot(ri, v);
                    ri
                };
                if v.is_heap_ref() {
                    ctx.heap().write_barrier_field(base_val, ri, v);
                }
                Ok(())
            } else {
                let off = byte_off as usize;
                let w = prim_width(kind)?;
                let mut obj = gc.borrow_mut();
                encode_prim(&mut obj.bytes_mut(), off, w, kind, v)
            }
        }
        // add-struct-heap-inline (P3b, D1-a): leaf write into a struct[] element.
        // make-value-copy: resolve the StructRefHeap handle → StructArrayElem via the arena.
        Value::StructRefHeap { idx, frame_id } => {
            let e = ctx.transient_arena.lock().struct_elem(*idx, *frame_id)?;
            if is_ref_tag(kind) {
                {
                    // unify-gc-heap PR-3: write the ref leaf into the struct[] refs block.
                    let mut arr = e.arr.borrow_mut();
                    let layout = arr.struct_layout()
                        .ok_or_else(|| anyhow::anyhow!("StructFieldSetPrim: StructRefHeap base is not a value-struct array"))?;
                    let rc = layout.ref_count();
                    let ri = layout.ref_index(byte_off).ok_or_else(|| {
                        anyhow::anyhow!("struct[] ref leaf at byte offset {byte_off} not in element layout")
                    })?;
                    if !arr.set_struct_ref(e.index as usize * rc + ri, v) {
                        anyhow::bail!("StructFieldSetPrim: struct[] ref leaf {ri} of element {} out of range", e.index);
                    }
                }
                // Write barrier: reference stored into a heap array element (P3b).
                if v.is_heap_ref() {
                    let owner = Value::Array(e.arr.clone());
                    ctx.heap().write_barrier_array_elem(&owner, e.index as usize, v);
                }
                Ok(())
            } else {
                // unify-gc-heap PR-3: encode the prim leaf into the struct[] bytes block.
                let mut arr = e.arr.borrow_mut();
                let layout = arr.struct_layout()
                    .ok_or_else(|| anyhow::anyhow!("StructFieldSetPrim: StructRefHeap base is not a value-struct array"))?;
                let off = e.index as usize * layout.size + byte_off as usize;
                let w = prim_width(kind)?;
                encode_prim(arr.struct_bytes_mut().expect("StructBytes backing"), off, w, kind, v)
            }
        }
        _ => {
            let (idx, fid) = as_struct_ref(base_val, "StructFieldSetPrim base")?;
            if is_ref_tag(kind) {
                return ctx.struct_arena.lock().set_ref(idx, fid, byte_off, v.clone());
            }
            let off = byte_off as usize;
            let w = prim_width(kind)?;
            ctx.struct_arena.lock().with_mut(idx, fid, |s| encode_prim(&mut s.bytes, off, w, kind, v))?
        }
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn as_struct_ref(v: &Value, what: &str) -> Result<(u32, u32)> {
    match v {
        Value::StructRef { idx, frame_id } => Ok((*idx, *frame_id)),
        other => bail!("{what}: expected a struct value (StructRef), got {other:?}"),
    }
}

#[cfg(test)]
#[path = "exec_struct_tests.rs"]
mod exec_struct_tests;
