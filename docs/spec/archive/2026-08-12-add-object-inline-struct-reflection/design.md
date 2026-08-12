# Design: 对象内联 struct 字段反射（P4b-B）

## D1：为什么 class 内联布局要单独复刻（≠ struct `_compute`）

P4b 的 `struct_reflect::compute` 复刻的是 **struct** 的 `_compute`：struct 的**每个**字段都进字节 blob
（基元裸打包 / 引用叶子进侧表 / 嵌套 struct 递归）。

class 的内联布局不同（`StructLayout._computeInlineLayout`）：class 是**引用类型**，只有它的**值类型 struct 字段**
被内联进对象 `struct_bytes`；基元 / 引用 / 其它 class 字段仍在 `slots`。所以：

- 迭代顺序按 `TypeDesc.fields`（声明序），但**跳过非 struct 字段**（它们不占字节区）。
- 每个 struct 字段按其 struct 的 align 对齐、放在运行 offset、offset 前进 struct.size；引用叶子按
  `对象相对 offset = 字段对象相对 offset + struct 内叶子 offset` 展平进对象引用位图。
- 区 size = `align_up(offset, maxAlign)`。

`compute_class_inline` 逐行镜像该算法，产出与 struct 版同构的 `ComputedLayout`（leaves 全为 `is_struct`）。

## D2：校验——对象相对布局对交付 `inline_layout` 三层核对

P3b 已把 class 的合成内联布局 wire 进 zbc/zpkg（`CLASS_FLAG_HAS_INLINE_STRUCT`，reader 组装
`TypeDescCold.inline_layout: StructTypeLayout`）。这份**交付布局是权威**（编译器写的、codegen 烘焙 offset 的
同一来源）。`compute_class_inline` 的产出用 `ComputedLayout::validate_against(inline_layout)` 核对：
① 区 size 相等；② 展平引用叶子 offset 集相等；③ 逐叶子 ref/prim 分类一致（class 内联叶子全 struct →
只走 ①②，对齐/大小任一漂移都会让 offset 集或 size 不符而 `bail!`）。复刻漂移 → 可 catch 的错误，绝不静默误读。

## D3：读写路径（复用 P4b，最小新增）

反射的 `Value::Object` 臂先问 `struct_field_fq(class, name)`：

- **`Some(fq)`（内联 struct 字段）**：`compute_class_inline` + 校验 + `comp.field(name)` 取 leaf。
  - **GetValue** → `snapshot_struct_leaf(ctx, &obj, &comp, leaf)`：切 `struct_bytes[off..off+size]`、按嵌套
    struct 的引用叶子 offset 经 `comp.ref_index(off + nested_ref_off)` 从对象 `struct_refs` 取引用叶子，
    `box_struct_blob` → boxed 快照（值语义，改快照不动对象）。
  - **SetValue** → `write_struct_leaf(ctx, target, obj_gc, &comp, leaf, src_box)`：把 src box 的字节写进对象
    `struct_bytes[off..]`、引用叶子写进对象 `struct_refs`（对象相对 index）+ 每个堆引用叶子 `write_barrier_field`。
    对象是共享 `GcRef<ScriptObject>` → 写穿对调用方可见（引用身份）。
- **`None`（基元/引用字段）**：回落原 `field_index → slots` 路径，行为不变。

`snapshot_struct_leaf` / `write_struct_leaf` 从 P4b 的 `boxed_struct_field_get`（嵌套分支）/
`boxed_struct_field_set`（嵌套分支）抽出——装箱路径与对象路径**同一份**代码，因为 P4b 后 `BoxedStruct` 也是
`GcRef<ScriptObject>`，两者的字节区/侧表读写完全同构。

## D4：值语义 vs 引用身份（与 C# 对齐）

- **GetValue 返 boxed 快照**：内联 struct 字段是值类型，反射读出的是一份**独立副本**（`box_struct_blob` 新分配
  盒）——改这个盒不回写对象，匹配 C# `FieldInfo.GetValue` 对值类型字段返回装箱副本的语义。
- **SetValue 写穿对象**：对象本身是引用类型（`GcRef` 共享），SetValue 就地改对象字节区 → 该对象的所有别名都
  见新值。这是「对象字段」的引用身份，非「盒」的——与 C# 一致。

golden 用例覆盖：GetValue 快照独立（改 `wo2` 不动 `w.origin`）+ SetValue 写穿（`w.origin` 直读见新值）。

## D5：风险与兜底

- **复刻漂移**（对齐/大小规则与编译器不一致）：D2 三层校验兜底，不符即 `bail!`（可 catch），不会静默误读字节。
- **短名解析**（字段声明 `Point pt` 的 type_tag 常是短名 `Point`）：`struct_field_fq` / `compute_class_inline`
  经 `resolve_named`（P4b 已有）按声明类 namespace 解析到 FQ（`Demo.Point`），与 P4b 嵌套字段同款处理。
- **无内联字段的类**：`struct_field_fq` 返 `None` → 走 slot 路径，不触发 `compute_class_inline`（也不需要
  `inline_layout`，对普通类为 `None`）。

## 测试

- **单元**（`struct_reflect_tests.rs`）：`class_inline_layout_packs_only_struct_fields`（单 struct 字段 + 前后
  夹基元/string slot 字段）、`..._two_struct_fields_pack_contiguously`（两 struct 字段连续打包、中间夹基元）、
  `class_with_no_struct_fields_has_empty_inline_layout`。均校验 offset / 引用位图 / `validate_against` /
  `struct_field_fq` 分类。
- **端到端 golden**（`reflection/struct_field`）：普通 slot 字段仍走 slot（`id`/`label`）+ 内联 struct 字段
  GetValue 快照（`origin.x`/`.tag`）+ SetValue 写穿（对象直读 `w.origin.x`=555）+ 快照值语义独立 + 嵌套内联
  `Frame{Line edge}`（Line 含两 Point，引用叶子展平）。interp + jit 双模式匹配 expected。
