<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.TimeSpan

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Time`

`src/libraries/z42.core/src/Time/TimeSpan.z42:4`

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

### TimeSpan(long nanoseconds)

`src/libraries/z42.core/src/Time/TimeSpan.z42:8`

由纳秒构造。

### static TimeSpan Zero()

`src/libraries/z42.core/src/Time/TimeSpan.z42:13`

零时长。

### static TimeSpan FromNanoseconds(long ns)

`src/libraries/z42.core/src/Time/TimeSpan.z42:18`

由纳秒构造。

### static TimeSpan FromMilliseconds(long ms)

`src/libraries/z42.core/src/Time/TimeSpan.z42:23`

由毫秒构造。

### static TimeSpan FromSeconds(double sec)

`src/libraries/z42.core/src/Time/TimeSpan.z42:28`

由秒构造（double）。

### static TimeSpan FromMinutes(double min)

`src/libraries/z42.core/src/Time/TimeSpan.z42:33`

由分钟构造（double）。

### static TimeSpan FromHours(double hrs)

`src/libraries/z42.core/src/Time/TimeSpan.z42:38`

由小时构造（double）。

### long TotalNanoseconds()

`src/libraries/z42.core/src/Time/TimeSpan.z42:45`

总纳秒。

### long TotalMilliseconds()

`src/libraries/z42.core/src/Time/TimeSpan.z42:48`

总毫秒（整数除法，向零截断）。

### double TotalSeconds()

`src/libraries/z42.core/src/Time/TimeSpan.z42:51`

总秒（double）。

### double TotalMinutes()

`src/libraries/z42.core/src/Time/TimeSpan.z42:54`

总分钟（double）。

### double TotalHours()

`src/libraries/z42.core/src/Time/TimeSpan.z42:57`

总小时（double）。

### TimeSpan Add(TimeSpan other)

`src/libraries/z42.core/src/Time/TimeSpan.z42:62`

加 `other`。

### TimeSpan Subtract(TimeSpan other)

`src/libraries/z42.core/src/Time/TimeSpan.z42:67`

减 `other`。

### bool IsLessThan(TimeSpan other)

`src/libraries/z42.core/src/Time/TimeSpan.z42:74`

是否小于 `other`。

### bool IsLessEqual(TimeSpan other)

`src/libraries/z42.core/src/Time/TimeSpan.z42:76`

是否小于等于 `other`。

### bool IsGreaterThan(TimeSpan other)

`src/libraries/z42.core/src/Time/TimeSpan.z42:78`

是否大于 `other`。

### bool IsGreaterEqual(TimeSpan other)

`src/libraries/z42.core/src/Time/TimeSpan.z42:80`

是否大于等于 `other`。

### bool Equals(TimeSpan other)

`src/libraries/z42.core/src/Time/TimeSpan.z42:82`

是否相等。

### override string ToString()

`src/libraries/z42.core/src/Time/TimeSpan.z42:85`

形如 `"1500000000ns"`。

