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
use anyhow::Result;
use std::sync::Arc;

use crate::metadata::types::StructTypeLayout;
use super::Frame;

// unify-object-byte-layout (PR-2): the primitive byte<->Value codec moved to
// `metadata::types` (both object byte-storage and struct blobs consume it). Re-export
// so existing call sites here + in `corelib::reflection` keep the same path.
pub(crate) use crate::metadata::types::{decode_prim, encode_prim, prim_width};
// 叶子读写的引擎无关核心在 objops（interp 与 JIT 共用）。
use crate::objops::struct_leaf::{as_struct_ref, struct_field_get_val, struct_field_set_val};

/// `StructAlloc dst, type_name, size` — allocate a zero-initialized blob in the
/// per-context struct arena; `dst` = `Value::StructRef` handle. The blob's byte +
/// reference layout comes from the type's TYPE-section struct block (via
/// [`resolve_layout`]); pure-primitive types fall back to a `size`-only layout.
pub(super) fn struct_alloc(
    ctx: &VmContext, frame: &mut Frame, dst: u32, type_name: &str, size: u32,
) -> Result<()> {
    let v = struct_alloc_val(ctx, frame.frame_id(ctx), type_name, size);
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
    let (type_name, bytes, refs) = crate::objops::struct_leaf::snapshot_box(gc);
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
    let (src_bytes, src_refs, layout, tname) = crate::objops::struct_leaf::snapshot_elem(e)?;
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
///
/// ## 🔴 别再给这条路做提速了 —— 动态权重已实测（2026-09-29）
///
/// 此前只有**静态发射**占比（编译期数指令条数，深度 1 = 85.1%），而那**推不出**
/// 「85.1% 的开销是深度 1」：一个深度 3 的访问写在热循环里能执行几百万次。
/// 挂 debug 计数器跑**全 e2e 语料**（361 个程序，360 个跑起来）实测：
///
/// | 深度 | 次数 | 占比 |
/// |---|---|---|
/// | 1 | 1857 | 80.6% |
/// | 2 | 400 | 17.4% |
/// | 3 | 42 | 1.8% |
/// | 4 | 4 | 0.2% |
///
/// 方向与静态占比一致，但**决定性的是总量：全语料一共只有 2303 次**。
/// 按 T12 的逐深度实测（深度 1 = 37.3ns / 深度 2 = 144ns），361 个程序的符号化
/// 成本合计 ≈ **85 微秒**。
///
/// ⭐⭐⭐ 而**真实编译负载是 0 次** —— debug VM 跑 z42c 编一个文件（12 秒），
/// 计数器一次都没动。这与「stdlib 一条 `StructFieldGetPrim` 都不发」是同一件事的
/// 延伸：**z42c 自己也一条都不执行**。⇒ 提案里「真实编译负载 <2%」那条门槛在 z42c
/// 上是 0/0，**根本无法对照**（⚠️ 那条线本来就只是**我提的建议值、User 从未确认**，
/// 别把它当既定门槛引用）。
///
/// 据此撤掉了两项已动工/在排队的优化：**深度 1 快路**（用 `Value::StructRef` 的 arena
/// 槽自带 `layout` 省掉 `try_lookup_type`）与**把快路扩到深度 ≥2**。前者还额外要命：
/// 它在 release 下绕开 `check_base_space`，而那条对账正是 P2（#938）的**全部**价值主张
/// —— 性能本来就是中性的。**用唯一的收益去换一个测不出来的加速，方向是反的。**
///
/// ⚠️ 限定：仓里**没有 struct 密集的基准**（stdlib 的 bench 全不走这条路）⇒ 上面的
/// 绝对量只对「树内跑得到的负载」成立。真实 struct 密集用户代码（矩阵/向量/游戏循环）
/// 会完全不同，但我们手上没有那样的负载。要重做这个测量：在本函数入口挂一个按
/// `path.len()` 分桶的 debug 计数器，用 `libc::atexit` 把计数**追加**进一个文件
/// （e2e 语料是几百个独立进程，`Drop` 不跑 —— `main()` 里有多处 `std::process::exit`），
/// 然后逐条跑 `artifacts/build/tests/**/*.zbc` 求和。
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

/// `StructFieldSetPrim base, byte_off, kind, val` — write `val` into the `base`
/// struct at `byte_off` (in place; the value-struct lvalue write). A primitive
/// `kind` encodes bytes; a reference `kind` stores the `Value` into the reference
/// side-slice.
///
/// Arena base: no write barrier (the arena is a GC root, re-scanned every cycle).
/// Heap-object base (P3b): a reference-leaf write into `struct_refs` **does** need a
/// write barrier — the heap object is not re-scanned as a root, so a
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

#[cfg(test)]
#[path = "exec_struct_tests.rs"]
mod exec_struct_tests;
