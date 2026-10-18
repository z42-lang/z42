<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.GC

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/GC/GC.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GC.z42#L7)

让 z42 脚本显式控制 GC（触发回收、读堆用量、设上限、取停顿数据、导快照）。

```z42
public static class GC {
    static extern void Collect()
    static extern long UsedBytes()
    static extern long ForceCollect()
    static extern HeapStats GetStats()
    static extern bool Finalize(Object target)
    static extern long[] PauseHistogram()
    static extern long[] PauseStatsRaw()
    static extern long WriteHeapSnapshot(string path)
    static extern long[] RecentPauses()
    static extern long PauseWindowCapacity()
    static extern void SetMaxHeapBytes(long bytes)
    static extern void SetStrictOOM(bool enabled)
}
```

成员：[`Collect`](#Std.GC.Collect()) · [`UsedBytes`](#Std.GC.UsedBytes()) · [`ForceCollect`](#Std.GC.ForceCollect()) · [`GetStats`](#Std.GC.GetStats()) · [`Finalize`](#Std.GC.Finalize(Object)) · [`PauseHistogram`](#Std.GC.PauseHistogram()) · [`PauseStatsRaw`](#Std.GC.PauseStatsRaw()) · [`WriteHeapSnapshot`](#Std.GC.WriteHeapSnapshot(string)) · [`RecentPauses`](#Std.GC.RecentPauses()) · [`PauseWindowCapacity`](#Std.GC.PauseWindowCapacity()) · [`SetMaxHeapBytes`](#Std.GC.SetMaxHeapBytes(long)) · [`SetStrictOOM`](#Std.GC.SetStrictOOM(bool))

<a id="Std.GC.Collect()"></a>
### static extern void Collect()

[`src/libraries/z42.core/src/GC/GC.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GC.z42#L10)

触发环检测与回收（基于 Bacon-Rajan trial-deletion）。GC 被 pause 时本调用是 no-op。

<a id="Std.GC.UsedBytes()"></a>
### static extern long UsedBytes()

[`src/libraries/z42.core/src/GC/GC.z42:15`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GC.z42#L15)

当前堆 used_bytes 估算值。

<a id="Std.GC.ForceCollect()"></a>
### static extern long ForceCollect()

[`src/libraries/z42.core/src/GC/GC.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GC.z42#L19)

强制完整 collect 并返回估算的 freed_bytes。与 Collect() 不同，不被 pause 跳过。

<a id="Std.GC.GetStats()"></a>
### static extern HeapStats GetStats()

[`src/libraries/z42.core/src/GC/GC.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GC.z42#L23)

当前 heap 全字段 stats 快照，返回 `Std.HeapStats` 实例。`MaxBytes` 用 `-1` 表示 unlimited。

<a id="Std.GC.Finalize(Object)"></a>
### static extern bool Finalize(Object target)

[`src/libraries/z42.core/src/GC/GC.z42:29`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GC.z42#L29)

显式触发 `target` 的 finalizer 并立即作废该对象（RAII 风格即时释放，对齐 .NET `IDisposable.Dispose()` / Java `AutoCloseable.close()`）。返回 true 表示 finalizer 真触发； false 表示对象没注册 finalizer、非堆引用类型、已释放或 null。**调用之后对象不可再用**。

<a id="Std.GC.PauseHistogram()"></a>
### static extern long[] PauseHistogram()

[`src/libraries/z42.core/src/GC/GC.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GC.z42#L35)

返回 8 元素 `long[]`，第 `i` 项是落入第 `i` 桶的 collect 次数。桶边界（半开区间 `[lower, upper)`， 微秒）：0 `[0, 10)` · 1 `[10, 100)` · 2 `[100 µs, 1 ms)` · 3 `[1, 10) ms` · 4 `[10, 100) ms` · 5 `[100 ms, 1 s)` · 6 `[1, 10) s` · 7 `[10 s, ∞)`。自 heap 创建起累积，跨 GcMode 切换保留（不重置）。

<a id="Std.GC.PauseStatsRaw()"></a>
### static extern long[] PauseStatsRaw()

[`src/libraries/z42.core/src/GC/GC.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GC.z42#L40)

返回 4 元素 `long[]`：`[min_us, max_us, total_us, count]`。`count == 0` 时 `min_us` 是哨兵 `-1`，其余三项为 0；读 `min_us` 前先判 `count`。

<a id="Std.GC.WriteHeapSnapshot(string)"></a>
### static extern long WriteHeapSnapshot(string path)

[`src/libraries/z42.core/src/GC/GC.z42:47`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GC.z42#L47)

把当前堆的对象引用图写成 V8 `.heapsnapshot` JSON 文件，返回写入字节数。文件可被 Chrome DevTools（Memory → Load）加载看 retainer 图 / dominator 树 / 到根最短路径。 路径不可写时抛异常。

<a id="Std.GC.RecentPauses()"></a>
### static extern long[] RecentPauses()

[`src/libraries/z42.core/src/GC/GC.z42:52`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GC.z42#L52)

返回 rolling window 内的所有 pause 样本（µs），按时间从旧到新。 长度 ≤ `PauseWindowCapacity()`。

<a id="Std.GC.PauseWindowCapacity()"></a>
### static extern long PauseWindowCapacity()

[`src/libraries/z42.core/src/GC/GC.z42:56`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GC.z42#L56)

返回 rolling window 的容量（`Z42_GC_PAUSE_WINDOW` 决定，默认 1024，clamp 到 [1, 65536]）。

<a id="Std.GC.SetMaxHeapBytes(long)"></a>
### static extern void SetMaxHeapBytes(long bytes)

[`src/libraries/z42.core/src/GC/GC.z42:61`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GC.z42#L61)

设置堆上限（bytes）。传 0 或负数则清除上限（无界）。strict OOM 开启时，超过上限的 alloc 抛 `Std.OutOfMemoryException`。

<a id="Std.GC.SetStrictOOM(bool)"></a>
### static extern void SetStrictOOM(bool enabled)

[`src/libraries/z42.core/src/GC/GC.z42:66`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/GC/GC.z42#L66)

开启 / 关闭 strict OOM 模式。true = alloc 越过 max_heap_bytes 时抛 OutOfMemoryException； false（默认）= 无限制。

