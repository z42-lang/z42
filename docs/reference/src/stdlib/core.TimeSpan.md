<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.TimeSpan

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Time`

[`src/libraries/z42.core/src/Time/TimeSpan.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L4)

时间段值，内部单位纳秒（i64），范围约 ±292 年。不可变：算术方法返回新对象。

```z42
public sealed class TimeSpan {
    TimeSpan(long nanoseconds)
    static TimeSpan Zero()
    static TimeSpan FromNanoseconds(long ns)
    static TimeSpan FromMilliseconds(long ms)
    static TimeSpan FromSeconds(double sec)
    static TimeSpan FromMinutes(double min)
    static TimeSpan FromHours(double hrs)
    long TotalNanoseconds()
    long TotalMilliseconds()
    double TotalSeconds()
    double TotalMinutes()
    double TotalHours()
    TimeSpan Add(TimeSpan other)
    TimeSpan Subtract(TimeSpan other)
    bool IsLessThan(TimeSpan other)
    bool IsLessEqual(TimeSpan other)
    bool IsGreaterThan(TimeSpan other)
    bool IsGreaterEqual(TimeSpan other)
    bool Equals(TimeSpan other)
    override string ToString()
}
```

成员：[`TimeSpan`](#Std.Time.TimeSpan.ctor(long)) · [`Zero`](#Std.Time.TimeSpan.Zero()) · [`FromNanoseconds`](#Std.Time.TimeSpan.FromNanoseconds(long)) · [`FromMilliseconds`](#Std.Time.TimeSpan.FromMilliseconds(long)) · [`FromSeconds`](#Std.Time.TimeSpan.FromSeconds(double)) · [`FromMinutes`](#Std.Time.TimeSpan.FromMinutes(double)) · [`FromHours`](#Std.Time.TimeSpan.FromHours(double)) · [`TotalNanoseconds`](#Std.Time.TimeSpan.TotalNanoseconds()) · [`TotalMilliseconds`](#Std.Time.TimeSpan.TotalMilliseconds()) · [`TotalSeconds`](#Std.Time.TimeSpan.TotalSeconds()) · [`TotalMinutes`](#Std.Time.TimeSpan.TotalMinutes()) · [`TotalHours`](#Std.Time.TimeSpan.TotalHours()) · [`Add`](#Std.Time.TimeSpan.Add(TimeSpan)) · [`Subtract`](#Std.Time.TimeSpan.Subtract(TimeSpan)) · [`IsLessThan`](#Std.Time.TimeSpan.IsLessThan(TimeSpan)) · [`IsLessEqual`](#Std.Time.TimeSpan.IsLessEqual(TimeSpan)) · [`IsGreaterThan`](#Std.Time.TimeSpan.IsGreaterThan(TimeSpan)) · [`IsGreaterEqual`](#Std.Time.TimeSpan.IsGreaterEqual(TimeSpan)) · [`Equals`](#Std.Time.TimeSpan.Equals(TimeSpan)) · [`ToString`](#Std.Time.TimeSpan.ToString())

<a id="Std.Time.TimeSpan.ctor(long)"></a>
### TimeSpan(long nanoseconds)

[`src/libraries/z42.core/src/Time/TimeSpan.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L8)

由纳秒构造。

<a id="Std.Time.TimeSpan.Zero()"></a>
### static TimeSpan Zero()

[`src/libraries/z42.core/src/Time/TimeSpan.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L13)

零时长。

<a id="Std.Time.TimeSpan.FromNanoseconds(long)"></a>
### static TimeSpan FromNanoseconds(long ns)

[`src/libraries/z42.core/src/Time/TimeSpan.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L18)

由纳秒构造。

<a id="Std.Time.TimeSpan.FromMilliseconds(long)"></a>
### static TimeSpan FromMilliseconds(long ms)

[`src/libraries/z42.core/src/Time/TimeSpan.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L23)

由毫秒构造。

<a id="Std.Time.TimeSpan.FromSeconds(double)"></a>
### static TimeSpan FromSeconds(double sec)

[`src/libraries/z42.core/src/Time/TimeSpan.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L28)

由秒构造（double）。

<a id="Std.Time.TimeSpan.FromMinutes(double)"></a>
### static TimeSpan FromMinutes(double min)

[`src/libraries/z42.core/src/Time/TimeSpan.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L33)

由分钟构造（double）。

<a id="Std.Time.TimeSpan.FromHours(double)"></a>
### static TimeSpan FromHours(double hrs)

[`src/libraries/z42.core/src/Time/TimeSpan.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L38)

由小时构造（double）。

<a id="Std.Time.TimeSpan.TotalNanoseconds()"></a>
### long TotalNanoseconds()

[`src/libraries/z42.core/src/Time/TimeSpan.z42:45`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L45)

总纳秒。

<a id="Std.Time.TimeSpan.TotalMilliseconds()"></a>
### long TotalMilliseconds()

[`src/libraries/z42.core/src/Time/TimeSpan.z42:48`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L48)

总毫秒（整数除法，向零截断）。

<a id="Std.Time.TimeSpan.TotalSeconds()"></a>
### double TotalSeconds()

[`src/libraries/z42.core/src/Time/TimeSpan.z42:51`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L51)

总秒（double）。

<a id="Std.Time.TimeSpan.TotalMinutes()"></a>
### double TotalMinutes()

[`src/libraries/z42.core/src/Time/TimeSpan.z42:54`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L54)

总分钟（double）。

<a id="Std.Time.TimeSpan.TotalHours()"></a>
### double TotalHours()

[`src/libraries/z42.core/src/Time/TimeSpan.z42:57`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L57)

总小时（double）。

<a id="Std.Time.TimeSpan.Add(TimeSpan)"></a>
### TimeSpan Add(TimeSpan other)

[`src/libraries/z42.core/src/Time/TimeSpan.z42:62`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L62)

加 `other`。

<a id="Std.Time.TimeSpan.Subtract(TimeSpan)"></a>
### TimeSpan Subtract(TimeSpan other)

[`src/libraries/z42.core/src/Time/TimeSpan.z42:67`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L67)

减 `other`。

<a id="Std.Time.TimeSpan.IsLessThan(TimeSpan)"></a>
### bool IsLessThan(TimeSpan other)

[`src/libraries/z42.core/src/Time/TimeSpan.z42:74`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L74)

是否小于 `other`。

<a id="Std.Time.TimeSpan.IsLessEqual(TimeSpan)"></a>
### bool IsLessEqual(TimeSpan other)

[`src/libraries/z42.core/src/Time/TimeSpan.z42:76`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L76)

是否小于等于 `other`。

<a id="Std.Time.TimeSpan.IsGreaterThan(TimeSpan)"></a>
### bool IsGreaterThan(TimeSpan other)

[`src/libraries/z42.core/src/Time/TimeSpan.z42:78`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L78)

是否大于 `other`。

<a id="Std.Time.TimeSpan.IsGreaterEqual(TimeSpan)"></a>
### bool IsGreaterEqual(TimeSpan other)

[`src/libraries/z42.core/src/Time/TimeSpan.z42:80`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L80)

是否大于等于 `other`。

<a id="Std.Time.TimeSpan.Equals(TimeSpan)"></a>
### bool Equals(TimeSpan other)

[`src/libraries/z42.core/src/Time/TimeSpan.z42:82`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L82)

是否相等。

<a id="Std.Time.TimeSpan.ToString()"></a>
### override string ToString()

[`src/libraries/z42.core/src/Time/TimeSpan.z42:85`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeSpan.z42#L85)

形如 `"1500000000ns"`。

