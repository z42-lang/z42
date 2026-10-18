<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.DateTimeOffset

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Time`

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:5`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L5)

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

成员：[`DateTimeOffset`](#Std.Time.DateTimeOffset.ctor(DateTime,TimeZone)) · [`Now`](#Std.Time.DateTimeOffset.Now(TimeZone)) · [`FromLocal`](#Std.Time.DateTimeOffset.FromLocal(int,int,int,int,int,int,int,TimeZone)) · [`Parse`](#Std.Time.DateTimeOffset.Parse(string)) · [`UtcDateTime`](#Std.Time.DateTimeOffset.UtcDateTime()) · [`LocalDateTime`](#Std.Time.DateTimeOffset.LocalDateTime()) · [`Offset`](#Std.Time.DateTimeOffset.Offset()) · [`Year`](#Std.Time.DateTimeOffset.Year()) · [`Month`](#Std.Time.DateTimeOffset.Month()) · [`Day`](#Std.Time.DateTimeOffset.Day()) · [`Hour`](#Std.Time.DateTimeOffset.Hour()) · [`Minute`](#Std.Time.DateTimeOffset.Minute()) · [`Second`](#Std.Time.DateTimeOffset.Second()) · [`Millisecond`](#Std.Time.DateTimeOffset.Millisecond()) · [`DayOfWeek`](#Std.Time.DateTimeOffset.DayOfWeek()) · [`DayOfYear`](#Std.Time.DateTimeOffset.DayOfYear()) · [`ToIso8601`](#Std.Time.DateTimeOffset.ToIso8601()) · [`ToString`](#Std.Time.DateTimeOffset.ToString()) · [`Equals`](#Std.Time.DateTimeOffset.Equals(DateTimeOffset)) · [`EqualsExact`](#Std.Time.DateTimeOffset.EqualsExact(DateTimeOffset)) · [`IsAfter`](#Std.Time.DateTimeOffset.IsAfter(DateTimeOffset)) · [`IsBefore`](#Std.Time.DateTimeOffset.IsBefore(DateTimeOffset)) · [`Subtract`](#Std.Time.DateTimeOffset.Subtract(DateTimeOffset)) · [`Add`](#Std.Time.DateTimeOffset.Add(TimeSpan)) · [`SubtractSpan`](#Std.Time.DateTimeOffset.SubtractSpan(TimeSpan))

<a id="Std.Time.DateTimeOffset.ctor(DateTime,TimeZone)"></a>
### DateTimeOffset(DateTime utc, TimeZone tz)

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L10)

以 UTC 时刻 + 时区构造；任一参数为 null 抛 `ArgumentNullException`。

<a id="Std.Time.DateTimeOffset.Now(TimeZone)"></a>
### static DateTimeOffset Now(TimeZone tz)

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L22)

当前时刻在 `tz` 下的 DateTimeOffset。

<a id="Std.Time.DateTimeOffset.FromLocal(int,int,int,int,int,int,int,TimeZone)"></a>
### static DateTimeOffset FromLocal(int year, int month, int day, int hour, int minute, int second, int millisecond, TimeZone tz)

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L27)

由本地日历分量 + 时区构造；`tz` 为 null 抛 `ArgumentNullException`。

<a id="Std.Time.DateTimeOffset.Parse(string)"></a>
### static DateTimeOffset Parse(string s)

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:40`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L40)

解析带**必须**偏移后缀（`Z` / `±HH:MM` / `±HHMM` / `±HH`）的 ISO 8601； 缺后缀抛 `FormatException`。

<a id="Std.Time.DateTimeOffset.UtcDateTime()"></a>
### DateTime UtcDateTime()

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:96`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L96)

底层 UTC 时刻。

<a id="Std.Time.DateTimeOffset.LocalDateTime()"></a>
### DateTime LocalDateTime()

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:99`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L99)

本地墙钟读数（不带偏移信息，塞进一个 DateTime）。

<a id="Std.Time.DateTimeOffset.Offset()"></a>
### TimeZone Offset()

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:104`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L104)

时区（偏移）。

<a id="Std.Time.DateTimeOffset.Year()"></a>
### int Year()

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:107`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L107)

本地年份。

<a id="Std.Time.DateTimeOffset.Month()"></a>
### int Month()

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:109`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L109)

本地月份。

<a id="Std.Time.DateTimeOffset.Day()"></a>
### int Day()

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:111`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L111)

本地日。

<a id="Std.Time.DateTimeOffset.Hour()"></a>
### int Hour()

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:113`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L113)

本地时。

<a id="Std.Time.DateTimeOffset.Minute()"></a>
### int Minute()

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:115`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L115)

本地分。

<a id="Std.Time.DateTimeOffset.Second()"></a>
### int Second()

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:117`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L117)

本地秒。

<a id="Std.Time.DateTimeOffset.Millisecond()"></a>
### int Millisecond()

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:119`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L119)

本地毫秒。

<a id="Std.Time.DateTimeOffset.DayOfWeek()"></a>
### int DayOfWeek()

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:121`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L121)

本地星期几。

<a id="Std.Time.DateTimeOffset.DayOfYear()"></a>
### int DayOfYear()

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:123`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L123)

本地一年中的第几天。

<a id="Std.Time.DateTimeOffset.ToIso8601()"></a>
### string ToIso8601()

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:126`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L126)

带偏移后缀的 ISO 8601 渲染。

<a id="Std.Time.DateTimeOffset.ToString()"></a>
### override string ToString()

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:131`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L131)

同 `ToIso8601()`。

<a id="Std.Time.DateTimeOffset.Equals(DateTimeOffset)"></a>
### bool Equals(DateTimeOffset other)

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:136`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L136)

只比 UTC 时刻（忽略偏移）。

<a id="Std.Time.DateTimeOffset.EqualsExact(DateTimeOffset)"></a>
### bool EqualsExact(DateTimeOffset other)

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:142`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L142)

UTC 时刻 + 偏移都相同。

<a id="Std.Time.DateTimeOffset.IsAfter(DateTimeOffset)"></a>
### bool IsAfter(DateTimeOffset other)

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:149`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L149)

是否晚于 `other`。

<a id="Std.Time.DateTimeOffset.IsBefore(DateTimeOffset)"></a>
### bool IsBefore(DateTimeOffset other)

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:151`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L151)

是否早于 `other`。

<a id="Std.Time.DateTimeOffset.Subtract(DateTimeOffset)"></a>
### TimeSpan Subtract(DateTimeOffset other)

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:154`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L154)

与 `other` 的差值。

<a id="Std.Time.DateTimeOffset.Add(TimeSpan)"></a>
### DateTimeOffset Add(TimeSpan span)

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:159`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L159)

加 TimeSpan。

<a id="Std.Time.DateTimeOffset.SubtractSpan(TimeSpan)"></a>
### DateTimeOffset SubtractSpan(TimeSpan span)

[`src/libraries/z42.core/src/Time/DateTimeOffset.z42:164`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTimeOffset.z42#L164)

减 TimeSpan。

