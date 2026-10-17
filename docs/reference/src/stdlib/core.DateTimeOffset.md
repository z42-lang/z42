<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.DateTimeOffset

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Time`

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:5`

UTC 时刻与固定偏移时区的配对（等价 C# `DateTimeOffset`）。九个日历访问器取的是 **本地**（已加偏移）字段。

```z42
public sealed class DateTimeOffset {
    DateTimeOffset(DateTime utc, TimeZone tz)
    static DateTimeOffset Now(TimeZone tz)
    static DateTimeOffset FromLocal(int year, int month, int day, int hour, int minute, int second, int millisecond, TimeZone tz)
    static DateTimeOffset Parse(string s)
    DateTime UtcDateTime()
    DateTime LocalDateTime()
    TimeZone Offset()
    int Year()
    int Month()
    int Day()
    int Hour()
    int Minute()
    int Second()
    int Millisecond()
    int DayOfWeek()
    int DayOfYear()
    string ToIso8601()
    override string ToString()
    bool Equals(DateTimeOffset other)
    bool EqualsExact(DateTimeOffset other)
    bool IsAfter(DateTimeOffset other)
    bool IsBefore(DateTimeOffset other)
    TimeSpan Subtract(DateTimeOffset other)
    DateTimeOffset Add(TimeSpan span)
    DateTimeOffset SubtractSpan(TimeSpan span)
}
```

### DateTimeOffset(DateTime utc, TimeZone tz)

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:10`

以 UTC 时刻 + 时区构造；任一参数为 null 抛 `ArgumentNullException`。

### static DateTimeOffset Now(TimeZone tz)

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:22`

当前时刻在 `tz` 下的 DateTimeOffset。

### static DateTimeOffset FromLocal(int year, int month, int day, int hour, int minute, int second, int millisecond, TimeZone tz)

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:27`

由本地日历分量 + 时区构造；`tz` 为 null 抛 `ArgumentNullException`。

### static DateTimeOffset Parse(string s)

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:40`

解析带**必须**偏移后缀（`Z` / `±HH:MM` / `±HHMM` / `±HH`）的 ISO 8601； 缺后缀抛 `FormatException`。

### DateTime UtcDateTime()

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:96`

底层 UTC 时刻。

### DateTime LocalDateTime()

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:99`

本地墙钟读数（不带偏移信息，塞进一个 DateTime）。

### TimeZone Offset()

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:104`

时区（偏移）。

### int Year()

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:107`

本地年份。

### int Month()

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:109`

本地月份。

### int Day()

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:111`

本地日。

### int Hour()

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:113`

本地时。

### int Minute()

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:115`

本地分。

### int Second()

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:117`

本地秒。

### int Millisecond()

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:119`

本地毫秒。

### int DayOfWeek()

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:121`

本地星期几。

### int DayOfYear()

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:123`

本地一年中的第几天。

### string ToIso8601()

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:126`

带偏移后缀的 ISO 8601 渲染。

### override string ToString()

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:131`

同 `ToIso8601()`。

### bool Equals(DateTimeOffset other)

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:136`

只比 UTC 时刻（忽略偏移）。

### bool EqualsExact(DateTimeOffset other)

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:142`

UTC 时刻 + 偏移都相同。

### bool IsAfter(DateTimeOffset other)

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:149`

是否晚于 `other`。

### bool IsBefore(DateTimeOffset other)

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:151`

是否早于 `other`。

### TimeSpan Subtract(DateTimeOffset other)

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:154`

与 `other` 的差值。

### DateTimeOffset Add(TimeSpan span)

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:159`

加 TimeSpan。

### DateTimeOffset SubtractSpan(TimeSpan span)

`src/libraries/z42.core/src/Time/DateTimeOffset.z42:164`

减 TimeSpan。

