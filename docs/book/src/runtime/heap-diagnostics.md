# 堆保留诊断（Heap Retention Diagnostics）

> 对齐：2026-08-06（change `add-heap-retention-diagnostics`）。
> 上位设计（context 卸载专项诊断）：[load-context.md](../../../design/runtime/load-context.md) §5。

## 为什么

z42 自有**精确 GC**，能回答 .NET GC 回答不了的问题：**"这个对象为什么还活着 / 被谁钉住"**。
用途：内存泄漏定位、collectible `AssemblyLoadContext` 卸载迟迟不回收的排查（后者是这个通用工具的
一个应用场景——本能力**不绑定** AssemblyLoadContext）。

## API（`Std.Diagnostics.Heap`）

| 层级 | 方法 | 回答 |
|------|------|------|
| **L1** | `DirectReferrers(object) -> Retainer[]` | 哪些**堆对象**直接持有 target 的引用 |
| **L2** | `RetainingRoots(object) -> RootRef[]` | 从 target 反向可达的 **GC 根**（类别级） |

- `Retainer`：`TypeName`（引用者 FQ 类型名，数组以 `[]` 结尾）+ `Id`（堆身份）。
- `RootRef`：`Kind: RootKind`（`StaticField` / `StackFrame` / `FuncRefSlot` / `Pinned`）。
- target 非堆对象（primitive/null）→ 空数组。

## 机制 / 实现

### 反向引用图
GC 只有**正向**追踪（`Value::trace_children`）；诊断需**反向**。查询时做一次堆扫描：

```
force_collect()                       // ① 先触发 full GC —— 之后存活即可达，无浮动垃圾误报
build_retention_graph():              // ② 一次堆扫描建反向图
  region_object.iterate_alive: 每对象的 slots 里每条堆-ref child → rev[child].push(parent)
  region_array.iterate_alive:  每数组的 elements 里每条堆-ref child → rev[child].push(parent)
  分类根 + pinned 根: 每个根 Value → root_ptrs[obj].push(RootKind)
L1 direct_referrers(target) = rev[target]（按 id 去重）
L2 retaining_roots(target)  = 从 target 沿 rev 反向 BFS，收集途经对象被根直接指向的 RootKind（按类别去重、有序）
```

对象身份用 data ptr（`GcRef::data_ptr_unlocked` as usize）作 key。反向 BFS 用 `visited` 集处理环。
纯图逻辑在 `gc/retention.rs`（`RetentionGraph`），堆遍历在 `arc_heap.rs::build_retention_graph`。

### 分类根 scanner（L2 报根的前提）
mark 阶段的 `external_root_scanner` 只吐**匿名** Value，报不出根类别。故新增**分类根 scanner**
（`CategorizedRootScanner`，`VmCore` 接线捕 `Weak<VmCore>`）：按类别枚举 `static_fields → StaticField`、
每线程帧 regs/env/stack-arena → `StackFrame`、`func_ref_slots → FuncRefSlot`；pinned 根（GC 内部）→
`Pinned`。**只在诊断查询时调用**，mark 热路径不变（零回归）。

### 准确性
先 `force_collect()` → `iterate_alive` 只见可达对象 → 反向图不含浮动垃圾。诊断非热路径，一次额外 GC 可接受。

## 边界 / 后续

- **L3 完整引用链**（根 → … → target 整条路径 + 多路径 + 环去重展示）—— 后续 change。
- **具体根名**（哪个 static 字段名 / 哪个局部变量）—— 需 root-source 精确标签，本能力只到**类别级**。
- **常态零开销的保留边注册**（load-context.md §5 第 1 层「框架边」）—— 本能力走按需堆扫描（第 2 层路线）。

## 关联

- 引入：change `add-heap-retention-diagnostics`（`docs/spec/archive/`）。
- context 卸载诊断应用：[load-context.md](load-context.md)（`AssemblyLoadContext` 卸载不回收时用本工具查保留者）。
