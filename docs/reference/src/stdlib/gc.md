# `Std.GC` 与 GC 句柄 —— 脚本侧的显式内存控制

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.core/`（`src/GC/GC.z42`、`GCHandle.z42`、`HeapStats.z42`、
> `WeakHandle.z42`、`SoftHandle.z42`）；命名空间 `Std`
>
> 启动期就固定的 GC 旋钮（`gc-mode` / `gc-max-bytes` / `gc-trace` …）不在这里，
> 见[运行时设置](../toolchain/runtime-settings.md)。

z42 的内存由 GC 管，正常代码**不需要**碰本页的任何 API。需要它们的只有四类场景：

- 在基准测试 / 断言里读堆用量与停顿数据；
- 给嵌入场景或测试设一个硬堆上限，越界就抛而不是吃满内存；
- 把文件 / socket / 原生 handle 之类的资源**在确定的时刻**释放掉，而不是等回收；
- 跨 native 边界把对象**钉住**（strong handle），或持有**不阻止回收**的引用
  （weak / soft handle）。

全部类型都在 `Std` 命名空间，`z42.core` 隐式加载，**不需要写 `using`**。

本页只写调用面。GC 内部怎么标记、怎么分代、怎么清扫属实现细节，与这里的契约无关，
也不应被代码依赖。

## `Std.GC`

```z42
public static class GC {
    public static extern void   Collect();
    public static extern long   ForceCollect();
    public static extern long   UsedBytes();
    public static extern HeapStats GetStats();

    public static extern bool   Finalize(Object target);

    public static extern void   SetMaxHeapBytes(long bytes);
    public static extern void   SetStrictOOM(bool enabled);

    public static extern long[] PauseHistogram();
    public static extern long[] PauseStatsRaw();
    public static extern long[] RecentPauses();
    public static extern long   PauseWindowCapacity();

    public static extern long   WriteHeapSnapshot(string path);
}
```

### 触发回收

| 方法 | 返回 | 语义 |
|---|---|---|
| `Collect()` | — | **建议性**。可能被合并到一次正在进行的回收里，也可能什么都不做 |
| `ForceCollect()` | 释放的字节数 | 请求一次完整回收；返回本次估算的释放量 |

**`ForceCollect()` 返回 `0` 不等于「堆里没有垃圾」**——另一次回收已在进行时，本次调用
让位并直接返回 `0`。想确认回收真的发生过，比对前后的 `GetStats().GcCycles`。

两个方法都**不保证**某个具体对象在调用后被回收。没有「回收这一个对象」的 API；要在确定
时刻释放资源用 `Finalize()`。

### 堆用量

| 方法 | 返回 |
|---|---|
| `UsedBytes()` | 当前估算的活字节数 |
| `GetStats()` | 一个 `Std.HeapStats` 快照（含 `UsedBytes` 在内的 7 个字段）|

`UsedBytes()` 与 `GetStats().UsedBytes` 读的是同一个量。只要一个数就用前者。

两者都是**估算值**：分配即计入，回收后按释放量扣减，字符串拼接之类的临时分配也算在内。
连续两次调用之间数字会变，别用等号去断言，用区间。

### 堆上限与 OOM

| 方法 | 语义 |
|---|---|
| `SetMaxHeapBytes(bytes)` | 设堆字节上限。`bytes <= 0` 清除上限（无界）|
| `SetStrictOOM(enabled)` | 打开后，越过上限的分配抛 `Std.OutOfMemoryException`；默认关闭 |

设了上限但没开 strict 模式，越限分配照样成功，上限只是收紧回收配额。两个都要调才有
「越限即失败」的效果，顺序不限。

上限的当前值读 `GetStats().MaxBytes`，无上限时是 **`-1`**。

```z42
GC.SetMaxHeapBytes(64 * 1024 * 1024);
GC.SetStrictOOM(true);
try {
    // ... 吃内存的工作 ...
} catch (OutOfMemoryException e) {
    // e.Message 形如 "cannot allocate array[4000000]: heap limit exceeded"
}
GC.SetStrictOOM(false);
GC.SetMaxHeapBytes(0);
```

> ⚠️ **strict OOM 当前只在 `--mode interp` 下兑现。** 默认的 JIT 模式下，越限的分配
> 不抛异常，而是**静默得到 `null`**——随后第一次使用它的地方才会报出
> `expected array, got Null` 之类的运行期错误。靠 strict OOM 做失败注入的测试，要
> 显式加 `--mode interp`。

这里设的上限与命令行 / 环境变量的 `gc-max-bytes` 旋钮是同一个量，本 API 的调用覆盖启动
时的取值。

### 即时释放资源

```z42
public static extern bool Finalize(Object target);
```

同步触发 `target` 上注册的 finalizer 并立刻作废这个对象，语义对齐 .NET 的
`IDisposable.Dispose()` / Java 的 `AutoCloseable.close()`：你主动释放，GC 只是兜底。

| 返回 | 情况 |
|---|---|
| `true` | finalizer 确实被触发了 |
| `false` | 对象没注册 finalizer、不是堆上的引用类型、或者已经被释放过；`null` 也返回 `false` |

**调用之后这个对象就不能再用了。** 任何仍指向它的强引用再去访问都是错误用法，**不保证
抛出可捕获的异常**，可能直接终止进程；weak / soft 句柄取它会得到 `null`。所以
`Finalize()` 只能在你确知没有别处还在用它时调用。

普通的 z42 对象没有 finalizer，对它们调用只会得到 `false`，没有副作用。

### 停顿数据

四个方法都是纯读，随时可调。

```z42
public static extern long[] PauseHistogram();       // 长度恒为 8
public static extern long[] PauseStatsRaw();        // 长度恒为 4
public static extern long[] RecentPauses();         // 长度 ≤ PauseWindowCapacity()
public static extern long   PauseWindowCapacity();
```

`PauseHistogram()` 的第 `i` 项是落进第 `i` 个桶的回收次数。桶边界是半开区间
`[lower, upper)`，单位微秒：

| `i` | 范围 |
|---|---|
| 0 | `[0, 10) µs` |
| 1 | `[10, 100) µs` |
| 2 | `[100 µs, 1 ms)` |
| 3 | `[1, 10) ms` |
| 4 | `[10, 100) ms` |
| 5 | `[100 ms, 1 s)` |
| 6 | `[1, 10) s` |
| 7 | `[10 s, ∞)` |

直方图自进程启动起累积，**不会重置**，也不随 `gc-mode` 切换清零。没有清零的 API。

`PauseStatsRaw()` 返回 `[min_us, max_us, total_us, count]`。**`count == 0` 时
`min_us` 是哨兵值 `-1`，不是 0**——读 `min_us` 前先判 `count`：

```z42
long[] p = GC.PauseStatsRaw();
if (p[3] > 0) {
    Console.WriteLine("min=" + p[0] + "µs max=" + p[1] + "µs avg=" + (p[2] / p[3]) + "µs");
}
```

`RecentPauses()` 是滑动窗口内的**逐次**停顿样本（微秒），按时间从旧到新。窗口容量由
`gc-pause-window` 旋钮决定，默认 1024，取值被夹到 `[1, 65536]`；用
`PauseWindowCapacity()` 读实际容量。`RecentPauses().Length` 等于容量时意味着窗口已满，
更早的样本已经被挤掉——此时「一共跑了多少次」只能看 `PauseStatsRaw()[3]`。

### 堆快照

```z42
public static extern long WriteHeapSnapshot(string path);
```

把当前堆的对象引用图写成 V8 `.heapsnapshot` JSON 文件，返回写入的字节数。生成的文件
可直接拖进 Chrome DevTools 的 Memory 面板（Load），看 retainer 图、dominator 树、
到根的最短路径。

路径不可写（目录不存在、无权限）时抛 `Std.Exception`，消息里带路径与 OS 错误文本。

两个用得上时要知道的局限：

- **没有分配点栈回溯**——能看到「谁引用了谁」，看不到「这个对象是在哪一行分配的」。
- **弱引用不进图**，不算作保留关系。

## `Std.HeapStats`

`GC.GetStats()` 的返回类型。只读，没有公开构造函数——拿它的唯一途径是 `GetStats()`。

```z42
public class HeapStats {
    public long Allocations       { get; }
    public long GcCycles          { get; }
    public long UsedBytes         { get; }
    public long MaxBytes          { get; }
    public long RootsPinned       { get; }
    public long FinalizersPending { get; }
    public long Observers         { get; }
}
```

| 字段 | 含义 |
|---|---|
| `Allocations` | 自堆创建以来累计的分配次数 |
| `GcCycles` | 累计回收次数 |
| `UsedBytes` | 当前估算活字节数，同 `GC.UsedBytes()` |
| `MaxBytes` | 堆字节上限；**无上限时为 `-1`** |
| `RootsPinned` | 当前被钉住的根数量 |
| `FinalizersPending` | 已注册、等待触发的 finalizer 数 |
| `Observers` | 当前活动的 GC 事件观察者数 |

每次 `GetStats()` 都是新快照，字段值之间不保证是同一瞬间的一致读数。

**`HeapStats` 只有这 7 个字段。** minor / major 回收次数、累计回收字节数**不在这里**——
要读它们走 `Std.Diagnostics.RuntimeStats.Counters()`（`MinorCollections` /
`MajorCollections` / `ReclaimedBytes`），见[日志与运行时自省](diagnostics.md)。

## 句柄：`GCHandle` / `WeakHandle` / `SoftHandle`

三种句柄，先按需求选：

| 需求 | 用 |
|---|---|
| 把对象钉住，直到我显式放手（跨 native 边界、跨 scope anchor）| `GCHandle` + `Strong` |
| 简单的「不阻止回收」的引用 | `WeakHandle` |
| 想缓存、但内存吃紧时允许被清掉 | `SoftHandle` |
| 需要在 strong / weak 之间切换，或需要显式 `Free()` | `GCHandle` |

### `Std.GCHandle` / `Std.GCHandleType`

```z42
public enum GCHandleType {
    Weak,     // = 0
    Strong,   // = 1
}

public struct GCHandle {
    public static extern GCHandle Alloc(object target, GCHandleType type);
    public static GCHandle AllocWeak(object target);
    public static GCHandle AllocStrong(object target);

    public extern object        Target      { get; }
    public extern bool          IsAllocated { get; }
    public extern GCHandleType  Kind        { get; }
    public extern void          Free();
}
```

| 成员 | 语义 |
|---|---|
| `Alloc(target, type)` | 占一个槽位并返回句柄；`AllocWeak` / `AllocStrong` 是它的两个薄封装 |
| `Target` | 槽位当前持有的对象。已 `Free()` / weak 目标已被回收 → `null` |
| `IsAllocated` | 槽位是否还占着。**weak 目标被回收后它仍为 `true`**，只有 `Free()` 才转 `false` |
| `Kind` | 分配时的模式。`Free()` 之后这个值无意义（实测回落成 `Strong`）——先判 `IsAllocated` |
| `Free()` | 释放槽位。**幂等**，重复调用安全 |

`Target` 返回 `object`，取回具体类型要自己 cast：

```z42
GCHandle h = GCHandle.AllocStrong(node);
// ... 中间跨了任意多层调用 / native 边界 ...
Node back = (Node)h.Target;
h.Free();
```

**`GCHandle` 是 struct，但拷贝共享同一个槽位。** 值拷贝复制的是槽位编号，所以任意一个
副本调 `Free()`，**所有**副本一起失效：

```z42
GCHandle a = GCHandle.AllocStrong(x);
GCHandle b = a;        // 值拷贝，同一槽位
a.Free();
// 此时 b.IsAllocated == false，b.Target == null
```

分配会失败（得到一个 `IsAllocated == false` 的句柄）的情况：

| 目标 | `Strong` | `Weak` |
|---|---|---|
| 对象 / 数组 | 成功 | 成功 |
| 原子值（`int` / `string` / `bool` / 委托 …）| 成功，槽位存一份**值的拷贝** | **失败** |
| `null` | **失败** | **失败** |

**`Alloc*` 从不抛异常也从不返回 `null`**，失败只体现为 `IsAllocated == false`。所以
`GCHandle.AllocStrong(maybeNull)` 是静默失败的——要区分「目标为空」和「钉住成功」，
必须查 `IsAllocated`。

### `Std.WeakHandle`

```z42
public class WeakHandle {
    public static extern WeakHandle MakeWeak(object target);
    public static extern object     Upgrade(WeakHandle handle);
}
```

`Upgrade` 是**静态方法**，句柄当参数传，不是 `handle.Upgrade()`。

| 调用 | 返回 |
|---|---|
| `MakeWeak(对象 / 数组)` | 一个 `WeakHandle` |
| `MakeWeak(原子值)` | **`null`**（原子值不可弱化）|
| `MakeWeak(null)` | `null` |
| `Upgrade(handle)` | 目标对象，或目标已被回收时 `null` |
| `Upgrade(null)` | `null`（不抛）|

所以标准写法要判两次 `null`：

```z42
WeakHandle w = WeakHandle.MakeWeak(target);
// ... 稍后 ...
object alive = WeakHandle.Upgrade(w);      // w 为 null 也安全
if (alive != null) { /* 还活着 */ }
```

`WeakHandle` 没有 `Free()`，也不需要——它本身就是普通对象，自己会被回收。

### `Std.SoftHandle`

```z42
public class SoftHandle {
    public static extern SoftHandle Create(object target);
    public extern object            Get();
}
```

比 weak 强一档的引用：内存不吃紧时目标一直留着，堆压力越过阈值后 GC 可以把它清掉。
适合做可重建的缓存。

| 调用 | 返回 |
|---|---|
| `Create(对象 / 数组)` | 一个 `SoftHandle` |
| `Create(原子值 / null)` | 一个 `SoftHandle`，但它的 `Get()` **恒为 `null`** |
| `Get()` | 目标对象，或已被清掉时 `null` |

**`Create()` 从不返回 `null`**——失败体现为 `Get()` 恒空，这一点与 `WeakHandle.MakeWeak`
相反，别照搬判空写法。

**没设堆上限时软引用永不被清除**（`GC.SetMaxHeapBytes()` 或 `gc-max-bytes` 旋钮）。
清除的压力阈值由 `gc-soft-threshold` 旋钮控制。`SoftHandle` 没有显式释放的方法。

## 不支持

- **没有 `GC.SuppressFinalize` / `ReRegisterForFinalize` / `KeepAlive` / `WaitForPendingFinalizers`**，
  也没有代（generation）参数的 `Collect(int gen)`、没有 `GC.GetGeneration(obj)`。
- **没有办法回收某一个指定对象**。`Collect()` / `ForceCollect()` 作用于整个堆；要在确定
  时刻放掉资源只有 `Finalize()`（并且之后该对象不可再用）。
- **弱引用与软引用的失效时机不可预测，也不保证发生。** 实测在当前版本上，目标变得不可
  达并经过多次 `ForceCollect()` 之后，脚本创建的 `WeakHandle` / `GCHandle(Weak)` 仍可能
  照样取得对象。**不要把资源释放、缓存淘汰、生命周期判定建立在「弱引用会变空」上**；
  需要确定性就用 `Finalize()` 或自己维护显式的失效标记。
- **strict OOM 在默认（JIT）模式下不抛异常**，见上文「堆上限与 OOM」。
- **`PauseStatsRaw()` 的 `min_us` 哨兵是 `-1`，不是 `long.MaxValue`。** 判空一律看
  `count`（下标 3）。
- **停顿直方图与滑动窗口不能清零**，也读不到「每次回收分别是 minor 还是 major」。
- **`HeapStats` 只有 7 个字段**，没有 minor / major 计数与累计回收字节；那三项在
  `Std.Diagnostics.RuntimeStats.Counters()`。
- **声明但未赋值的 `GCHandle` 局部变量不能访问成员**——`GCHandle h;` 之后直接读
  `h.IsAllocated` 会在运行期抛错，不是得到一个「未分配」的句柄。一律经
  `Alloc` / `AllocWeak` / `AllocStrong` 取得句柄。
- **没有 GC 事件回调 / 观察者的脚本 API**：`HeapStats.Observers` 只能读到数量，注册不了。
- **`WriteHeapSnapshot` 没有分配点栈回溯**，也不导出弱引用边。
- **句柄不能跨进程、不能序列化**：`GCHandle` 的槽位编号只在当前进程当前堆里有意义。
