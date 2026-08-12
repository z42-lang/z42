# Proposal: 装箱 struct 引用身份（C# 对齐）+ struct 字段反射（P4b）

## Why

struct 值类型功能面在 interp + JIT 已闭合。但装箱面有两个相扣的缺口：

### 缺口 1：装箱 struct 没有引用身份（与 C# 不一致）

C# 里 `object o = someStruct;` 装箱产出一个**共享堆引用**——`object b = o;` 别名同一个盒、传参改盒可见。
z42 的 `Value::BoxedStruct(Box<BoxedStructData>)` 是**值**（`Box` 独占所有权）：`.cloned()` 深拷贝 →
`object b = a` 是两份独立盒、传进函数改盒调用方看不见。**只要是 `object`（引用类型）就该共享**——这是
User 定的 C# 对齐原则；只有**未装箱 struct 直接传递**才是复制。现状违反了这条直觉。

### 缺口 2：反射读不了 struct 字段值

`builtin_field_get_value`/`set_value`（`corelib/reflection.rs:1739/1758`）只认 `Value::Object` 的
`field_index → slots`。装箱 struct 的字段在 `bytes`/`refs`（无 slots）；P3b 对象内联 struct 字段的真数据
在 `struct_bytes`/`struct_refs`（对象槽是 dead-slot Null）。→ 反射 target 传 struct 值 `bail`，反射对象
的内联 struct 字段读到 `Null`。反射驱动的（反）序列化对含 struct 的类型失效。

**两个缺口同源**：装箱表示是「值」而非「共享对象」。修缺口 1（给盒引用身份）后，缺口 2 的反射 SetValue
自然写穿（盒共享）。故**合为一个 change**。

## What Changes

### 装箱引用身份（路 B：装箱进 `ScriptObject`，User 裁决）

- **`Value::BoxedStruct(Box<BoxedStructData>)` → `Value::BoxedStruct(GcRef<ScriptObject>)`**（B2 变体保留、
  payload 改共享堆句柄）。装箱 = 分配一个 **struct 类型的 `ScriptObject`**（`type_desc` = struct 类型，
  struct blob 存进对象已有的 `struct_bytes`/`struct_refs`，`slots` 空）。**复用 `region_object` + 全部 GC
  机制，零 GC 核心改动**。引用身份靠 `GcRef` 共享白拿——`object b = a` 别名同一对象、反射 SetValue 写穿。
- **`inline_region_sizes()`** 对 `is_struct()` 类型改读 `struct_layout`（size + ref_count）——使 struct 类型的
  ScriptObject 自动把 struct blob 分配进 `struct_bytes`/`struct_refs`。
- **删 `BoxedStructData`**（其数据搬进 ScriptObject）。约 44 处 `Value::BoxedStruct(b)` 触及点改读法
  （`b.type_name`/`b.bytes`/`b.refs` → 经对象 `type_desc.name`/`struct_bytes`/`struct_refs`）+ GC 的
  BoxedStruct 臂改成「随 `GcRef` 标记/追踪」（与 `Object` 同路）。
- **不给 struct 类型加 base/vtable**（PR2a 决定不变）——只是把值装进已有对象容器、用 struct 自己的 TypeDesc；
  对象协议方法派发仍走现有 boxed 特判臂。

### struct 字段反射（建在引用身份之上）

- **`FieldInfo.GetValue(target)`**：target = boxed struct（`Value::BoxedStruct`）或对象内联 struct 字段 →
  按字段名从 `struct_bytes`/`struct_refs` 读（基元 decode / 引用叶子侧表 / 嵌套 struct → boxed 副本）。
- **`FieldInfo.SetValue(target, value)`**：boxed struct 共享对象 → **就地写穿**（调用方可见，C# 语义达成）；
  对象内联 struct 字段 → 写进对象内联区 + 引用叶子写屏障。
- **核心难点**：反射按字段名读值 struct 需 per-field 字节 offset + tag，但 runtime `StructTypeLayout` 只有
  size + 无名引用位图。解 = **Rust 复刻编译器 `StructLayout._compute`**（用 `TypeDesc.fields` 的
  (name, type_tag) 逐字段算），并**三层校验**与交付的 `struct_layout` 一致以抓漂移（design D1）。

**纯 runtime**：无 z42c 改动、无新 IR、无格式 bump、self-host 逐字节不变（codegen 发 `__box_struct` 不变，
只换其 Rust 实现）、warm 全程本地可验。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/metadata/types.rs` | MODIFY | `Value::BoxedStruct` payload → `GcRef<ScriptObject>`；删 `BoxedStructData`；`inline_region_sizes` 对 is_struct 读 struct_layout；GC visit/PartialEq/size/value_to_str 的 BoxedStruct 臂 |
| `src/runtime/src/gc/arc_heap.rs` | MODIFY | mark/trace/scan_object_refs 的 BoxedStruct 臂随 GcRef（同 Object）；`__box_struct` 用的 alloc 路径（如需新 alloc 入口） |
| `src/runtime/src/corelib/convert.rs` | MODIFY | `__box_struct` 分配 struct 类型 ScriptObject；`__struct_hash_code` 改读对象 |
| `src/runtime/src/interp/exec_struct.rs` | MODIFY | `unbox_struct` 改读对象 struct_bytes/refs；复用 decode/encode helper |
| `src/runtime/src/interp/exec_object.rs`, `exec_vcall.rs`, `exec_array.rs` | MODIFY | is/as/GetType/vcall/array get_boxed·set_boxed 的 BoxedStruct 臂改读法 |
| `src/runtime/src/jit/helpers/{vcall,object,array}.rs` | MODIFY | JIT 对称臂改读法 |
| `src/runtime/src/corelib/object.rs` | MODIFY | GetType 等 BoxedStruct 臂 |
| `src/runtime/src/corelib/struct_reflect.rs` | NEW | Rust 复刻 `_compute` + 校验 + Canon/Tag.FromName 镜像 |
| `src/runtime/src/corelib/reflection.rs` | MODIFY | `builtin_field_get_value`/`set_value` 加 BoxedStruct + 对象内联 struct 字段路径 |
| `src/runtime/src/corelib/mod.rs` | MODIFY | 挂 `mod struct_reflect;` |
| `src/tests/reflection/struct_field/`, `src/tests/types/struct_boxing_identity.z42` | NEW | golden：引用身份别名/传参 + 反射 GetValue/SetValue |
| `docs/book/.../struct-value-semantics.md`, `reflection.md`, `docs/roadmap.md` | MODIFY | 引用身份表示 + 反射机制 + P4b 完成 |

## 非目标（Deferred）

- 反射 invoke boxed struct 合成方法（`MethodInfo.Invoke` on Equals/GetHashCode/ToString）——独立 change。
- static struct 字段反射（罕见 edge）。
- 格式 bump 写 per-field 偏移表进 TYPE 段（方案 C）——未来反射面扩张时一次性收敛。
- B-radical 消灭 Z42PrimType（阶梯后续）。
