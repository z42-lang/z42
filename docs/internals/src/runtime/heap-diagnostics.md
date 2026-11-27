# 堆保留诊断（Heap Retention Diagnostics）

> 上位设计（context 卸载专项诊断）：[load-context.md](load-context-design.md) §5。

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
- `RootRef`：`Kind: RootKind`（`StaticField` / `StackFrame` / `Pinned`）。
- target 非堆对象（primitive/null）→ 空数组。

## 机制 / 实现

### 反向引用图
GC 只有**正向**追踪（`Value::trace_children`）；诊断需**反向**。查询时做一次堆扫描：

```
diagnostics::retention: collect_full()  // ① 在暂停内做全堆 GC（分代下是 major）
profiler::heap::retention::build_graph(heap): // ② 借暂停堆视图建反向图
  region_var 对象块: 每对象的字段里每条堆-ref child → rev[child].push(parent)
  region_var 数组块: 每数组的元素里每条堆-ref child → rev[child].push(parent)
  分类根 + pinned 根: 每个根 Value → root_ptrs[obj].push(RootKind)
L1 direct_referrers(target) = rev[target]（按 id 去重）
L2 retaining_roots(target)  = 从 target 沿 rev 反向 BFS，收集途经对象被根直接指向的 RootKind（按类别去重、有序）
```

对象身份用块头地址（`ObjRef::as_ptr` / `ArrRef::as_ptr` as usize）作 key。反向 BFS 用 `visited` 集处理环。
图构建、查询与纯图逻辑在 `crates/z42-profiler/src/profiler/heap/retention.rs`（`RetentionGraph`）。
`PausedHeap` 提供对象、数组和类别根遍历。区域回调在锁内执行，不得重入堆。
对象和数组分两遍枚举，直接借用区域内的值。

### 分类根

`threads` 为每个 VM 登记一个持有弱 Core 引用的 `GcRoots` 提供者。collector 读取根值，堆诊断
通过同一提供者读取根值和类别。静态字段和驻留串归 `StaticField`，每线程异常、寄存器帧和 arena
归 `StackFrame`。堆先枚举 pin 和强句柄根，类别为 `Pinned`。
独立堆注册的普通闭包只提供标记根。

### 准确性
诊断层通过 `request_gc_pause` 获取暂停，在 `diagnostics/retention.rs` 中先回收再把
`GcPauseGuard` 借出的 `PausedHeap` 交给 profiler。视图限定在暂停期和收集线程上，
因此读取其他线程的帧根时，其所有者已停在安全点或 native 等待区。另一收集器正在运行时，
当前线程先等待其结束，再重新申请暂停。完整回收之后才扫描存活区域，避免把浮动垃圾计入反向图。

## 边界 / 后续

- **L3 完整引用链**（根 → … → target 整条路径 + 多路径 + 环去重展示）—— 后续 change。
- **具体根名**（哪个 static 字段名 / 哪个局部变量）—— 需 root-source 精确标签，本能力只到**类别级**。
- **常态零开销的保留边注册**（load-context.md §5 第 1 层「框架边」）—— 本能力走按需堆扫描（第 2 层路线）。

## 关联

- context 卸载诊断应用：[load-context.md](load-context.md)（`AssemblyLoadContext` 卸载不回收时用本工具查保留者）。
