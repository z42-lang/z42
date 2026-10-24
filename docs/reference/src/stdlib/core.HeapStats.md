<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.HeapStats

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/GC/HeapStats.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/HeapStats.z42#L8)

`Std.GC.GetStats()` 的返回类型，1:1 投影 GC 的 7 个统计字段。

实例由 corelib `__gc_stats` builtin 直接产出（不暴露 ctor），保证字段值与 GC
实时状态一致。

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

成员：[`Allocations`](#Std.HeapStats.Allocations) · [`GcCycles`](#Std.HeapStats.GcCycles) · [`UsedBytes`](#Std.HeapStats.UsedBytes) · [`MaxBytes`](#Std.HeapStats.MaxBytes) · [`RootsPinned`](#Std.HeapStats.RootsPinned) · [`FinalizersPending`](#Std.HeapStats.FinalizersPending) · [`Observers`](#Std.HeapStats.Observers)

<a id="Std.HeapStats.Allocations"></a>
### long Allocations { get; }

[`src/libraries/z42.core/src/GC/HeapStats.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/HeapStats.z42#L10)

自堆创建以来累计的 alloc 次数。

<a id="Std.HeapStats.GcCycles"></a>
### long GcCycles { get; }

[`src/libraries/z42.core/src/GC/HeapStats.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/HeapStats.z42#L13)

`collect_cycles` / `force_collect` 触发次数。

<a id="Std.HeapStats.UsedBytes"></a>
### long UsedBytes { get; }

[`src/libraries/z42.core/src/GC/HeapStats.z42:16`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/HeapStats.z42#L16)

估计活字节（RC 模式下 `Rc<T>` Drop 不更新，单调增）。

<a id="Std.HeapStats.MaxBytes"></a>
### long MaxBytes { get; }

[`src/libraries/z42.core/src/GC/HeapStats.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/HeapStats.z42#L19)

堆字节上限；`-1` 表示 unlimited。

<a id="Std.HeapStats.RootsPinned"></a>
### long RootsPinned { get; }

[`src/libraries/z42.core/src/GC/HeapStats.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/HeapStats.z42#L22)

当前 pinned root 数量（host pin_root 计数 + frame pins）。

<a id="Std.HeapStats.FinalizersPending"></a>
### long FinalizersPending { get; }

[`src/libraries/z42.core/src/GC/HeapStats.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/HeapStats.z42#L25)

等待 finalizer 触发的注册数。

<a id="Std.HeapStats.Observers"></a>
### long Observers { get; }

[`src/libraries/z42.core/src/GC/HeapStats.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/HeapStats.z42#L28)

活动 GC 事件 observer 数。

