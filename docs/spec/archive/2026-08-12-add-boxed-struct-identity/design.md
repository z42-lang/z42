# Design: 装箱 struct 引用身份 + struct 字段反射（P4b）

两部分：**(I) 装箱引用身份**（路 B2，地基）+ **(II) struct 字段反射**（建其上）。

---

## Part I — 装箱引用身份（路 B2：装箱进 ScriptObject）

### 决策 D0：为什么必须走 GC 集成 + 复用 region_object

装箱盒里的 `refs`（引用叶子）含 `GcRef`（指向 string/object/array）。盒若共享，GC 必须能追踪盒内引用，
否则盒活着、盒指的对象被回收 → use-after-free。故不能只用非 GC 的 `Arc`——**共享盒必须进 GC 管理**。

GC（`ArcMagrGC`）现只管两种堆类型：`region_object`（ScriptObject）+ `region_array`（ArrayObj），
`arc_heap.rs` 遍布硬编「both regions」。**新增第三个 Region（路 A）= 深度 GC 改动**（每处 mark/sweep/
young-list/dirty-card 都要 +1）。**路 B = 装箱进 ScriptObject，复用 region_object，零 GC 核心改动**——
User 已裁路 B。

### 决策 D0-impl：B2 保留 `Value::BoxedStruct` 变体，payload 改 `GcRef<ScriptObject>`

```rust
// 前：值语义（Box 独占 → clone 深拷贝）
BoxedStruct(Box<BoxedStructData>) = 17,
// 后：共享堆句柄（GcRef → clone 共享同一 ScriptObject）
BoxedStruct(GcRef<ScriptObject>) = 17,
```

- **装箱后的对象** = 一个 `ScriptObject`，`type_desc` = struct 类型（`is_struct()` 真），struct blob 存进
  对象已有的 `struct_bytes`（基元叶子紧凑）/ `struct_refs`（引用叶子侧表），`slots` 空。
- **为何 B2 而非 B1（删变体、全用 `Value::Object`）**：B2 让 is/as/GetType/vcall/Equals/PartialEq/反射的
  **现有 BoxedStruct 特判臂结构不变**（仍按变体分派 boxed 值类型语义），只改「读盒」的方式（从内联
  `BoxedStructData` 字段 → 经对象 `type_desc`/`struct_bytes`/`struct_refs`）。B1 需把每个 BoxedStruct 臂
  折进 Object 臂 + `is_struct()` 判别，且要保证不误伤普通对象——**churn 更大、风险更高**。B2 的两个变体
  都持 `GcRef<ScriptObject>` 略冗余，但换来最小语义改动 + 复用 region_object 的 GC。

### 决策 D0-size：`inline_region_sizes()` 对 `is_struct()` 读 `struct_layout`

`alloc_object` 经 `type_desc.inline_regions()`（读 `inline_layout`）定 `struct_bytes`/`struct_refs` 尺寸。
struct 类型 `inline_layout=None`（那是**非 struct class 的内联 struct 字段**复合布局）。改：

```rust
pub fn inline_region_sizes(&self) -> (usize, usize) {
    if self.is_struct() {
        // 装箱进 ScriptObject：整个对象就是这个 struct，用它自己的 blob 布局。
        if let Some(sl) = self.struct_layout() { return (sl.size, sl.ref_count()); }
    }
    match self.cold...inline_layout { Some(il) => (il.size, il.ref_count()), None => (0,0) }
}
```

只影响 struct 类型的 ScriptObject（普通值 struct 走 arena、从不 alloc_object；唯一 struct-typed
ScriptObject = 装箱盒）→ 安全。

### 装箱/拆箱数据流

- **`__box_struct`（convert.rs）**：`StructRef{idx,frame_id}` → 读 arena slot 的 (type_name, bytes, refs)
  → `heap.alloc_object(struct_type_desc, slots=空, native)` 得 struct-typed ScriptObject → 把 bytes/refs
  拷进对象的 `struct_bytes`/`struct_refs` → `Value::BoxedStruct(gc)`。已是 BoxedStruct → 幂等返回（**同一
  盒，引用身份保持**）。需按 type_name 查 `struct_type_desc`（`ctx.try_lookup_type`）。
- **`unbox_struct`（exec_struct.rs）** `(P)o` / `o as P`：`BoxedStruct(gc)` → 读 `gc.borrow().struct_bytes`/
  `struct_refs` → memcpy 进当前帧 arena StructRef（独立副本，值语义拆出）。
- **`__struct_hash_code`**：读 `gc.struct_bytes`/`struct_refs`（逻辑不变，仅取值路径）。

### GC（arc_heap.rs + types.rs）

`BoxedStruct(gc)` 在 GC 里**与 `Object(gc)` 同路**：mark 循环 / mark_if_unmarked / trace_children /
scan_object_refs → 标记并追踪该 `GcRef<ScriptObject>`（对象自身的 `struct_refs` 由现有 ScriptObject
追踪路径遍历）。`is_heap_ref`=true（不变）。`object_size_bytes` → 对象尺寸。`Value::visit`（types.rs:1172）
BoxedStruct 臂 → `visit` 对象。**不再内联扫 `b.refs`**（盒已是独立堆对象）。

### PartialEq / == 语义（保持现有值相等，读法改）

`Value::BoxedStruct` 的 `PartialEq`（types.rs:1233）：保持 PR2a 的**值相等**（比 struct_bytes + refs），
只改成经对象读。引用身份下 `object b = a` → 同对象 → 既引用等也值等，现有 golden 行为不变。
（C# `==` on object 本是引用相等，但 z42 PR2a 选值相等 provisional；本 change 不改这个既定语义。）

---

## Part II — struct 字段反射（建在引用身份之上）

### 决策 D1：Rust 复刻 `_compute`（方案 B，User 已裁）

runtime 对值 struct 只交付 `StructTypeLayout{size, ref_offsets, ref_kinds}`（`types.rs:167`）——有 size +
无名引用位图，但**无字段名→offset**。反射按字段名读值，故须恢复 `field_name → (byte_off, tag, is_struct,
type_name)`。输入现成：`TypeDesc.fields: Vec<FieldSlot{name, type_tag}>`（声明序，`type_tag` = codegen
喂给 `Tag.FromName` 的同一串）。**在 Rust 复刻 `StructLayout._compute`**（`StructLayout.z42:279`）。

复刻算法（逐字节镜像 `_compute` + `_kindOf`/`_sizeOf`/`_alignOf`/`Canon`/`Tag.FromName`）：

```
compute(type_name):
  offset=0; max_align=1; leaves=[]
  for (fname, ftype) in TypeDesc(type_name).fields:      // 声明序
    if is_struct_type(ftype):                            // 递归嵌套
      n = compute(ftype); align = max(n.align, 1)
      offset = align_up(offset, align)
      leaves.push{fname, offset, size:n.size, is_struct:true, type_name:ftype}; size = n.size
    else:
      canon = Canon(ftype)                               // byte→u8 int→i32 long→i64 ...
      kind  = leaf_kind(canon)                           // Prim / ArcString / GcRef
      size  = size_of(canon, kind)                       // 1/2/4/8; ref=16
      align = align_of(kind, size)                       // prim=size; ref=8
      offset= align_up(offset, align)
      tag   = tag_from_name(ftype)                       // 镜像 Tag.FromName（同 codegen，不额外 Canon）
      leaves.push{fname, offset, size, is_struct:false, tag}
    offset += size; max_align = max(max_align, align)
  return {leaves, size: align_up(offset, max_align), align: max_align}
```

| 镜像函数 | 编译器出处 |
|------|-----------|
| `canon` | `Z42Type.z42:15`（剥 `?` + byte/sbyte/short/ushort/int/uint/long/ulong/float/double 别名） |
| `size_of`/`align_of`/`leaf_kind` | `StructLayout.z42:332/343/322` |
| `tag_from_name` | `ZbcFormat.z42:75`（bool/i8../i32=int/i64=long/f32=float/f64=double/char/str/else=Object） |

> ⚠️ `canon`（用于尺寸）认全别名；`tag_from_name`（用于 decode signedness）只认 `Tag.FromName` 的覆盖。
> 二者刻意分工：**decode tag 必须忠实 `Tag.FromName`（不额外 Canon）**——才读出 codegen encode 时的同一
> signedness/宽度。引用叶子（string/object/array）无论 tag 是 Str 还是 Object 都走 `is_ref_tag` 侧表 →
> 其 tag 精确值与正确性无关。

### 决策 D1-校验：三层校验（对记忆原方案的强化）

复刻后用交付的权威 `struct_layout` 三层校验，任一不符 `bail!`（可 catch，不静默错读）：

1. `computed.size == struct_layout.size`；
2. computed 全引用叶子 offset（含嵌套展平）排序后**逐一等** `struct_layout.ref_offsets`；
3. 逐叶子 ref/prim 分类交叉核对：判为引用的 offset 必在 `ref_offsets`；判为基元的必不在。

1+2 抓 offset/size/对齐漂移（`byte`/`short` 尺寸算错→连锁改后续 offset+总大小→被抓）。3 抓误分类。
残留未抓 = 同宽 signed↔unsigned decode（`i32` vs `u32`）→ 由 `tag_from_name` 忠实镜像 `Tag.FromName` +
全基元单元测试护栏保证。

**为何不选方案 C（格式 bump 偏移表）**：更 robust 但要两阶段 nightly + CI 两代自举 + fixture 重生 +
macOS 环境墙。P4b 本可纯 runtime warm 本地全验，方案 C 拖进重量级发布周期不值。复刻漂移由三层校验压制。

### 决策 D2：target 多态

| target | GetValue | SetValue |
|--------|----------|----------|
| `BoxedStruct(gc)`（共享对象，主路径 A） | `compute(type)`+validate → 找 `fi.Name` 叶子：基元 `decode_prim(struct_bytes)` / 引用 `struct_refs[ref_index]` / 嵌套 → boxed 副本(D4) | **就地写穿** `gc.borrow_mut().struct_bytes/refs`（对象共享→调用方可见，C# 语义达成）；引用叶子写屏障 |
| `Object(gc)` 且 `fi.Name` 是内联 struct 字段（B） | 从对象 `struct_bytes`/`struct_refs` + 对象级复合布局物化 boxed 副本 | 传入 boxed 值 bytes/refs 拷进对象内联区（复合 offset）+ 写屏障 |
| `Object(gc)` 普通字段 | 不变（`slots[field_index]`） | 不变 |

> **判别 (B)**：target 是 `Object` 且 `fi.Name` 字段声明类型 `is_struct_type`。此时不读 dead slot，改从对象
> 级内联布局（`td.inline_layout()` 交付；offset=字段在对象内联区起始）定位。B 的对象级复合布局同样跑
> `compute`（root=class 字段序）。若 B 复刻过复杂→拆紧随小 follow-up，本 change 保底交付 A + SetValue 写穿。

**顺序铁律**：GetValue/SetValue 必须**先判 boxed struct / is_struct 字段**再触 `slots`——struct-typed
对象 `slots` 空，`field_index` 命中但 `slots[i]` 越界。

### 决策 D3：SetValue 语义 = 就地写穿（引用身份达成，与记忆原方案对齐）

装箱引用身份落地后，boxed struct 是共享对象 → `fi.SetValue(box, v)` 改 `gc.borrow_mut().struct_bytes/refs`
**被所有持该盒的人看见**（C# 语义）。这正是缺口 1 修复的直接收益。基元 `encode_prim`；引用叶子
`struct_refs[ri]=v` + `write_barrier_field`；嵌套 struct → 传入 boxed 值的 bytes/refs 拷进盒该字段区间。

### 决策 D4：嵌套 struct 字段 GetValue → boxed 副本

字段类型本身是 struct → 返回该字段的 **boxed 快照**（值语义）：新装一个盒，`struct_bytes` =
父 `struct_bytes[off..off+size]` 拷贝、`struct_refs` = 映射的引用叶子拷贝（嵌套类型 `ref_offsets` 逐一经
`父.ref_index(off + nested_ref_off)` 映射）。改返回盒不影响父。

### 测试策略

- **Rust 单元**：`struct_reflect.rs`（compute 对纯基元/含 string/嵌套/混合对齐正确；validate 通过 + 篡改
  bail；`tag_from_name` 全基元断言）；引用身份（装箱后 clone 共享同对象、改一处另一处见）。
- **Golden e2e**：`struct_boxing_identity.z42`（`object b=a`/传参改盒可见）；`reflection/struct_field/`
  （GetValue 读基元/string/嵌套；SetValue 写穿 + 再读断言；对象内联 struct 字段若 B 纳入）。
- **GREEN**：`cargo test --lib` + `xtask test`（**不传 Z42_HOME**）+ self-host 5/5（codegen 未动逐字节不变）。

### Deferred

- 反射 invoke boxed struct 合成方法；static struct 字段反射；方案 C（未来反射面扩张一次性收敛）。
