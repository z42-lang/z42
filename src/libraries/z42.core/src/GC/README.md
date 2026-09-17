# z42.core/src/GC

## 职责

`Std.*` 命名空间下的 GC 控制接口、句柄类型、堆统计。底层走 corelib
`__gc_*` builtin → `MagrGC` trait（heap.rs）。

| 文件 | 内容 |
|------|------|
| `GC.z42` | `Std.GC` 静态类：Collect / UsedBytes / ForceCollect / GetStats |
| `GCHandle.z42` | `Std.GCHandle` struct（_slot: long，corelib HandleTable backing）+ `Std.GCHandleType` enum（Weak / Strong）|
| `HeapStats.z42` | `Std.HeapStats` class（7 long 字段：Allocations / GcCycles / UsedBytes / MaxBytes / RootsPinned / FinalizersPending / Observers）|
| `WeakHandle.z42` | `Std.WeakHandle` 轻量弱引用 primitive（无 Free，自动 GC）；`Delegates/SubscriptionRefs.z42` 内部消费 |

## 入口点

- `Std.GC.Collect()` / `UsedBytes()` / `ForceCollect()` / `GetStats()`
- `Std.GCHandle.Alloc(target, GCHandleType)` / `AllocWeak(target)` / `AllocStrong(target)`
- `Std.WeakHandle.MakeWeak(target)` / `Upgrade(handle)`

## GCHandle vs WeakHandle 选型

| 需求 | 选 |
|------|----|
| 简单弱引用，靠 GC 自动清理 | `WeakHandle` |
| 强引用 + 显式 Free 控制释放点 | `GCHandle` (Strong) |
| 弱引用 + 显式 Free（slot 复用） | `GCHandle` (Weak) |
| 在 Strong / Weak 模式间切换 | `GCHandle` |
| 跨 native 边界 anchor 对象 | `GCHandle` (Strong) |

详见 `docs/internals/src/runtime/gc-handle.md`（HandleSlab 数据结构、强句柄如何进 mark 根集、
struct 共享 backing 模式、加一种 handle kind 要动哪几处）。

## 依赖

`Object`（`object` 参数 + 实例字段类型）+ `Primitives`（long / bool）。
WeakHandle 与 GCHandle.AllocWeak 走独立 corelib path，互不依赖。
