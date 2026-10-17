<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Stopwatch

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Time`

`src/libraries/z42.core/src/Time/Stopwatch.z42:4`

单调时钟计时器（测耗时），匹配 C# `System.Diagnostics.Stopwatch`。挂钟跳变（NTP 调时）不影响它。

```z42
public sealed class Stopwatch {
    Stopwatch()
    static Stopwatch StartNew()
    bool IsRunning()
    void Start()
    void Stop()
    void Restart()
    TimeSpan Elapsed()
}
```

### Stopwatch()

`src/libraries/z42.core/src/Time/Stopwatch.z42:13`

新建，未启动，`Elapsed()` 为 0。

### static Stopwatch StartNew()

`src/libraries/z42.core/src/Time/Stopwatch.z42:20`

新建并立即启动。

### bool IsRunning()

`src/libraries/z42.core/src/Time/Stopwatch.z42:27`

是否在跑。

### void Start()

`src/libraries/z42.core/src/Time/Stopwatch.z42:30`

已在跑则无操作；否则从当前时刻继续累计。

### void Stop()

`src/libraries/z42.core/src/Time/Stopwatch.z42:37`

未在跑则无操作；否则把本段累加进总量。

### void Restart()

`src/libraries/z42.core/src/Time/Stopwatch.z42:44`

清零并重新计时。

### TimeSpan Elapsed()

`src/libraries/z42.core/src/Time/Stopwatch.z42:51`

累计时长（跑着时每次调用都变大）。

