<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.HeapStats

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/GC/HeapStats.z42:6`

`Std.GC.GetStats()` 的返回类型，1:1 投影 GC 的 7 个统计字段。实例由 corelib `__gc_stats` builtin 直接产出（不暴露 ctor），保证字段值与 GC 实时状态一致。

```z42
public class HeapStats {
    long Allocations { get; }
    long GcCycles { get; }
    long UsedBytes { get; }
    long MaxBytes { get; }
    long RootsPinned { get; }
    long FinalizersPending { get; }
    long Observers { get; }
}
```

### long Allocations { get; }

`src/libraries/z42.core/src/GC/HeapStats.z42:8`

自堆创建以来累计的 alloc 次数。

### long GcCycles { get; }

`src/libraries/z42.core/src/GC/HeapStats.z42:11`

`collect_cycles` / `force_collect` 触发次数。

### long UsedBytes { get; }

`src/libraries/z42.core/src/GC/HeapStats.z42:14`

估计活字节（RC 模式下 `Rc<T>` Drop 不更新，单调增）。

### long MaxBytes { get; }

`src/libraries/z42.core/src/GC/HeapStats.z42:17`

堆字节上限；`-1` 表示 unlimited。

### long RootsPinned { get; }

`src/libraries/z42.core/src/GC/HeapStats.z42:20`

当前 pinned root 数量（host pin_root 计数 + frame pins）。

### long FinalizersPending { get; }

`src/libraries/z42.core/src/GC/HeapStats.z42:23`

等待 finalizer 触发的注册数。

### long Observers { get; }

`src/libraries/z42.core/src/GC/HeapStats.z42:26`

活动 GC 事件 observer 数。

