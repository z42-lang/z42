# Proposal: 对象内联 struct 字段反射（P4b-B）

## Why

P4b（add-boxed-struct-identity）交付了**装箱 struct** 的字段反射 + 引用身份，但明确把
**对象内联 struct 字段反射** 留作 follow-up：

> `class C { Point pt; }` 的 `FieldInfo.GetValue(fi_pt, c)` 现读 P3b 的 **dead slot → `Null`**。

P3b 把 class 的 struct 字段**内联**进对象的 `struct_bytes`/`struct_refs`（字段的 slot 只是一个恒 `Null`
的占位 dead slot，真数据在字节区）。反射的 `Value::Object` 路径只会读 `slots[field_index]` → 对内联 struct
字段读到 `Null`、对其 `SetValue` 只写死 slot 不写字节区。反射驱动的（反）序列化对**含 struct 字段的堆类型**
仍失效——补齐这条路径，struct 反射面才闭合到「struct 出现的所有位置」。

## What Changes

**纯 runtime，复用 P4b 已建的字节解码/装箱基础设施。**

### 类级内联布局复刻（`struct_reflect::compute_class_inline`）

反射按字段名读内联 struct 值需该字段在**对象**字节区的对象相对 offset。runtime 对象只携带**合成的**
`inline_layout`（`TypeDesc.inline_layout`，P3b 已 wire）——对象相对字节区 size + **无名**引用位图，无字段名映射。
解 = 复刻编译器 **class** 内联布局算法 `StructLayout._computeInlineLayout`（≠ struct 的 `_compute`）：

- class **只把 struct 字段**按声明序打包进 `struct_bytes`（自然对齐，引用叶子展平进 `struct_refs`）；
  **非 struct 字段仍在 slots**（不进字节区）。
- 产出「struct 字段名 → 对象相对 (byte_off, size, 展平引用叶子 offset)」+ 对象相对引用位图。
- 用 P4b 已有的 `ComputedLayout::validate_against` 对交付的 `inline_layout` 做**同款三层校验**抓复刻漂移。

### 反射读写（复用 P4b helper）

- **GetValue**：`struct_field_fq` 判定字段是否内联 struct——是则从对象 `struct_bytes`/`struct_refs` 物化
  **boxed 快照**（值语义，改快照不动对象），共用 P4b 的 `snapshot_struct_leaf`；否则回落普通 slot 读。
- **SetValue**：内联 struct 字段把传入 boxed struct 的字节+引用叶子**就地写穿对象共享字节区**（对象是堆节点
  → 引用身份可见）+ 引用叶子写屏障，共用 P4b 的 `write_struct_leaf`；否则回落普通 slot 写。

### 顺带根因修（write barrier）

抽取共享 `write_struct_leaf` 时补齐了**嵌套 struct 字段引用叶子的写屏障**（P4b 装箱路径的嵌套写此前漏了
barrier——STW 下 no-op 但对未来并发/分代 GC 不健全）。装箱路径与对象路径都受益。

**无 z42c 改动、无新 IR、无格式 bump、self-host 逐字节不变、warm 全程本地可验。**

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/corelib/struct_reflect.rs` | MODIFY | 加 `compute_class_inline`（复刻 `_computeInlineLayout`）+ `struct_field_fq`（字段是否内联 struct） |
| `src/runtime/src/corelib/reflection.rs` | MODIFY | `builtin_field_get_value`/`set_value` 的 `Value::Object` 臂加内联 struct 字段路径；抽 `snapshot_struct_leaf`/`write_struct_leaf` 共享（装箱嵌套路径复用） |
| `src/runtime/src/corelib/struct_reflect_tests.rs` | MODIFY | 加类级内联布局单测（`class_td` helper + 3 用例） |
| `src/tests/reflection/struct_field/{source.z42,expected_output.txt}` | MODIFY | golden 扩对象内联 struct 字段读写用例（含嵌套 `Frame{Line edge}`） |
| `docs/book/src/runtime/struct-value-semantics.md` | MODIFY | 加「对象内联 struct 字段反射」节 + 收敛面标 ✅ |
| `docs/roadmap.md` | MODIFY | Deferred 移除本项 |

## 非目标（Deferred）

- 反射 invoke boxed struct 合成方法（`MethodInfo.Invoke` on Equals/GetHashCode/ToString）。
- static struct 字段反射（罕见 edge）。
- 格式 bump 写 per-field 偏移表进 TYPE 段（方案 C）——未来反射面扩张时一次性收敛。
