<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.Stopwatch

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Time`

[`src/libraries/z42.core/src/Time/Stopwatch.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/Stopwatch.z42#L4)

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

成员：[`Stopwatch`](#Std.Time.Stopwatch.ctor()) · [`StartNew`](#Std.Time.Stopwatch.StartNew()) · [`IsRunning`](#Std.Time.Stopwatch.IsRunning()) · [`Start`](#Std.Time.Stopwatch.Start()) · [`Stop`](#Std.Time.Stopwatch.Stop()) · [`Restart`](#Std.Time.Stopwatch.Restart()) · [`Elapsed`](#Std.Time.Stopwatch.Elapsed())

<a id="Std.Time.Stopwatch.ctor()"></a>
### Stopwatch()

[`src/libraries/z42.core/src/Time/Stopwatch.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/Stopwatch.z42#L13)

新建，未启动，`Elapsed()` 为 0。

<a id="Std.Time.Stopwatch.StartNew()"></a>
### static Stopwatch StartNew()

[`src/libraries/z42.core/src/Time/Stopwatch.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/Stopwatch.z42#L20)

新建并立即启动。

<a id="Std.Time.Stopwatch.IsRunning()"></a>
### bool IsRunning()

[`src/libraries/z42.core/src/Time/Stopwatch.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/Stopwatch.z42#L27)

是否在跑。

<a id="Std.Time.Stopwatch.Start()"></a>
### void Start()

[`src/libraries/z42.core/src/Time/Stopwatch.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/Stopwatch.z42#L30)

已在跑则无操作；否则从当前时刻继续累计。

<a id="Std.Time.Stopwatch.Stop()"></a>
### void Stop()

[`src/libraries/z42.core/src/Time/Stopwatch.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/Stopwatch.z42#L37)

未在跑则无操作；否则把本段累加进总量。

<a id="Std.Time.Stopwatch.Restart()"></a>
### void Restart()

[`src/libraries/z42.core/src/Time/Stopwatch.z42:44`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/Stopwatch.z42#L44)

清零并重新计时。

<a id="Std.Time.Stopwatch.Elapsed()"></a>
### TimeSpan Elapsed()

[`src/libraries/z42.core/src/Time/Stopwatch.z42:51`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/Stopwatch.z42#L51)

累计时长（跑着时每次调用都变大）。

