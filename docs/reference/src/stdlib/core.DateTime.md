<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.DateTime

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Time`

[`src/libraries/z42.core/src/Time/DateTime.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L6)

UTC 时刻值，内部单位 Unix epoch 毫秒（i64）。不可变：算术方法返回新对象。

`ToString()` 返回 Unix 毫秒数字串；可读输出用 `ToIso8601()`。

```z42
public sealed class DateTime {
    DateTime(long unixMs)
    static DateTime UtcNow()
    static DateTime FromUnixMs(long unixMs)
    static DateTime UnixEpoch()
    long UnixMs()
    int Year()
    int Month()
    int Day()
    int Hour()
    int Minute()
    int Second()
    int Millisecond()
    int DayOfWeek()
    int DayOfYear()
    static bool IsLeapYear(int year)
    static int DaysInMonth(int year, int month)
    static DateTime Utc(int year, int month, int day, int hour, int minute, int second, int millis)
    static DateTime UtcDate(int year, int month, int day)
    DateTime AddDays(int days)
    DateTime AddHours(int hours)
    DateTime AddMinutes(int minutes)
    DateTime AddSeconds(int seconds)
    DateTime AddMilliseconds(long millis)
    DateTime AddMonths(int months)
    DateTime AddYears(int years)
    TimeSpan Subtract(DateTime other)
    DateTime Add(TimeSpan span)
    DateTime SubtractSpan(TimeSpan span)
    bool IsAfter(DateTime other)
    bool IsBefore(DateTime other)
    bool Equals(DateTime other)
    override string ToString()
    string ToIso8601()
    string ToIso8601Basic()
    string ToIso8601With(TimeZone tz)
    static DateTime ParseIso8601(string s)
}
```

成员：[`DateTime`](#Std.Time.DateTime.ctor(long)) · [`UtcNow`](#Std.Time.DateTime.UtcNow()) · [`FromUnixMs`](#Std.Time.DateTime.FromUnixMs(long)) · [`UnixEpoch`](#Std.Time.DateTime.UnixEpoch()) · [`UnixMs`](#Std.Time.DateTime.UnixMs()) · [`Year`](#Std.Time.DateTime.Year()) · [`Month`](#Std.Time.DateTime.Month()) · [`Day`](#Std.Time.DateTime.Day()) · [`Hour`](#Std.Time.DateTime.Hour()) · [`Minute`](#Std.Time.DateTime.Minute()) · [`Second`](#Std.Time.DateTime.Second()) · [`Millisecond`](#Std.Time.DateTime.Millisecond()) · [`DayOfWeek`](#Std.Time.DateTime.DayOfWeek()) · [`DayOfYear`](#Std.Time.DateTime.DayOfYear()) · [`IsLeapYear`](#Std.Time.DateTime.IsLeapYear(int)) · [`DaysInMonth`](#Std.Time.DateTime.DaysInMonth(int,int)) · [`Utc`](#Std.Time.DateTime.Utc(int,int,int,int,int,int,int)) · [`UtcDate`](#Std.Time.DateTime.UtcDate(int,int,int)) · [`AddDays`](#Std.Time.DateTime.AddDays(int)) · [`AddHours`](#Std.Time.DateTime.AddHours(int)) · [`AddMinutes`](#Std.Time.DateTime.AddMinutes(int)) · [`AddSeconds`](#Std.Time.DateTime.AddSeconds(int)) · [`AddMilliseconds`](#Std.Time.DateTime.AddMilliseconds(long)) · [`AddMonths`](#Std.Time.DateTime.AddMonths(int)) · [`AddYears`](#Std.Time.DateTime.AddYears(int)) · [`Subtract`](#Std.Time.DateTime.Subtract(DateTime)) · [`Add`](#Std.Time.DateTime.Add(TimeSpan)) · [`SubtractSpan`](#Std.Time.DateTime.SubtractSpan(TimeSpan)) · [`IsAfter`](#Std.Time.DateTime.IsAfter(DateTime)) · [`IsBefore`](#Std.Time.DateTime.IsBefore(DateTime)) · [`Equals`](#Std.Time.DateTime.Equals(DateTime)) · [`ToString`](#Std.Time.DateTime.ToString()) · [`ToIso8601`](#Std.Time.DateTime.ToIso8601()) · [`ToIso8601Basic`](#Std.Time.DateTime.ToIso8601Basic()) · [`ToIso8601With`](#Std.Time.DateTime.ToIso8601With(TimeZone)) · [`ParseIso8601`](#Std.Time.DateTime.ParseIso8601(string))

<a id="Std.Time.DateTime.ctor(long)"></a>
### DateTime(long unixMs)

[`src/libraries/z42.core/src/Time/DateTime.z42:10`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L10)

由 Unix 毫秒构造。

<a id="Std.Time.DateTime.UtcNow()"></a>
### static DateTime UtcNow()

[`src/libraries/z42.core/src/Time/DateTime.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L20)

当前 UTC 时刻。

<a id="Std.Time.DateTime.FromUnixMs(long)"></a>
### static DateTime FromUnixMs(long unixMs)

[`src/libraries/z42.core/src/Time/DateTime.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L23)

由 Unix 毫秒构造。

<a id="Std.Time.DateTime.UnixEpoch()"></a>
### static DateTime UnixEpoch()

[`src/libraries/z42.core/src/Time/DateTime.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L28)

Unix 纪元（1970 年 1 月 1 日 00:00:00 UTC）。

<a id="Std.Time.DateTime.UnixMs()"></a>
### long UnixMs()

[`src/libraries/z42.core/src/Time/DateTime.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L33)

Unix 毫秒。

<a id="Std.Time.DateTime.Year()"></a>
### int Year()

[`src/libraries/z42.core/src/Time/DateTime.z42:39`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L39)

年份（Gregorian，含公元前返回负数；公历没有 0 年）。

<a id="Std.Time.DateTime.Month()"></a>
### int Month()

[`src/libraries/z42.core/src/Time/DateTime.z42:46`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L46)

月份，1..12。

<a id="Std.Time.DateTime.Day()"></a>
### int Day()

[`src/libraries/z42.core/src/Time/DateTime.z42:53`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L53)

日，1..31。

<a id="Std.Time.DateTime.Hour()"></a>
### int Hour()

[`src/libraries/z42.core/src/Time/DateTime.z42:60`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L60)

时，0..23（UTC）。

<a id="Std.Time.DateTime.Minute()"></a>
### int Minute()

[`src/libraries/z42.core/src/Time/DateTime.z42:65`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L65)

分，0..59（UTC）。

<a id="Std.Time.DateTime.Second()"></a>
### int Second()

[`src/libraries/z42.core/src/Time/DateTime.z42:70`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L70)

秒，0..59（UTC）。

<a id="Std.Time.DateTime.Millisecond()"></a>
### int Millisecond()

[`src/libraries/z42.core/src/Time/DateTime.z42:75`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L75)

毫秒，0..999。

<a id="Std.Time.DateTime.DayOfWeek()"></a>
### int DayOfWeek()

[`src/libraries/z42.core/src/Time/DateTime.z42:81`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L81)

星期几，0=Sunday .. 6=Saturday（对齐 .NET / JS `getDay()`）。

<a id="Std.Time.DateTime.DayOfYear()"></a>
### int DayOfYear()

[`src/libraries/z42.core/src/Time/DateTime.z42:88`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L88)

一年中的第几天，1..366。

<a id="Std.Time.DateTime.IsLeapYear(int)"></a>
### static bool IsLeapYear(int year)

[`src/libraries/z42.core/src/Time/DateTime.z42:96`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L96)

是否闰年：能被 4 整除且（不能被 100 整除或能被 400 整除）。

<a id="Std.Time.DateTime.DaysInMonth(int,int)"></a>
### static int DaysInMonth(int year, int month)

[`src/libraries/z42.core/src/Time/DateTime.z42:103`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L103)

给定年月的天数。Month 1..12；越界抛 `ArgumentException`。

<a id="Std.Time.DateTime.Utc(int,int,int,int,int,int,int)"></a>
### static DateTime Utc(int year, int month, int day, int hour, int minute, int second, int millis)

[`src/libraries/z42.core/src/Time/DateTime.z42:118`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L118)

由显式日历分量构造 UTC 时刻。越界抛 `ArgumentException`。

<a id="Std.Time.DateTime.UtcDate(int,int,int)"></a>
### static DateTime UtcDate(int year, int month, int day)

[`src/libraries/z42.core/src/Time/DateTime.z42:149`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L149)

给定日期的 UTC 零点。

<a id="Std.Time.DateTime.AddDays(int)"></a>
### DateTime AddDays(int days)

[`src/libraries/z42.core/src/Time/DateTime.z42:154`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L154)

加 `days` 天（可为负）。

<a id="Std.Time.DateTime.AddHours(int)"></a>
### DateTime AddHours(int hours)

[`src/libraries/z42.core/src/Time/DateTime.z42:159`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L159)

加 `hours` 小时。

<a id="Std.Time.DateTime.AddMinutes(int)"></a>
### DateTime AddMinutes(int minutes)

[`src/libraries/z42.core/src/Time/DateTime.z42:164`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L164)

加 `minutes` 分钟。

<a id="Std.Time.DateTime.AddSeconds(int)"></a>
### DateTime AddSeconds(int seconds)

[`src/libraries/z42.core/src/Time/DateTime.z42:169`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L169)

加 `seconds` 秒。

<a id="Std.Time.DateTime.AddMilliseconds(long)"></a>
### DateTime AddMilliseconds(long millis)

[`src/libraries/z42.core/src/Time/DateTime.z42:174`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L174)

加 `millis` 毫秒。

<a id="Std.Time.DateTime.AddMonths(int)"></a>
### DateTime AddMonths(int months)

[`src/libraries/z42.core/src/Time/DateTime.z42:180`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L180)

加 `months` 个日历月；日号截断到目标月最后一天（1 月 31 日 + 1 月 = 2 月 28/29 日）。 时分秒毫秒原样保留。

<a id="Std.Time.DateTime.AddYears(int)"></a>
### DateTime AddYears(int years)

[`src/libraries/z42.core/src/Time/DateTime.z42:205`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L205)

加 `years` 年；闰年 2 月 29 日在非闰年截断为 2 月 28 日。

<a id="Std.Time.DateTime.Subtract(DateTime)"></a>
### TimeSpan Subtract(DateTime other)

[`src/libraries/z42.core/src/Time/DateTime.z42:212`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L212)

与 `other` 的差值（TimeSpan）。

<a id="Std.Time.DateTime.Add(TimeSpan)"></a>
### DateTime Add(TimeSpan span)

[`src/libraries/z42.core/src/Time/DateTime.z42:217`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L217)

加 TimeSpan。

<a id="Std.Time.DateTime.SubtractSpan(TimeSpan)"></a>
### DateTime SubtractSpan(TimeSpan span)

[`src/libraries/z42.core/src/Time/DateTime.z42:222`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L222)

减 TimeSpan。

<a id="Std.Time.DateTime.IsAfter(DateTime)"></a>
### bool IsAfter(DateTime other)

[`src/libraries/z42.core/src/Time/DateTime.z42:229`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L229)

是否晚于 `other`。

<a id="Std.Time.DateTime.IsBefore(DateTime)"></a>
### bool IsBefore(DateTime other)

[`src/libraries/z42.core/src/Time/DateTime.z42:231`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L231)

是否早于 `other`。

<a id="Std.Time.DateTime.Equals(DateTime)"></a>
### bool Equals(DateTime other)

[`src/libraries/z42.core/src/Time/DateTime.z42:233`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L233)

是否同一时刻。

<a id="Std.Time.DateTime.ToString()"></a>
### override string ToString()

[`src/libraries/z42.core/src/Time/DateTime.z42:236`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L236)

返回 Unix 毫秒数字串。

<a id="Std.Time.DateTime.ToIso8601()"></a>
### string ToIso8601()

[`src/libraries/z42.core/src/Time/DateTime.z42:243`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L243)

格式化为 `"YYYY-MM-DDTHH:MM:SS.sssZ"`（毫秒 3 位，UTC `Z`）。

<a id="Std.Time.DateTime.ToIso8601Basic()"></a>
### string ToIso8601Basic()

[`src/libraries/z42.core/src/Time/DateTime.z42:248`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L248)

格式化为 `"YYYY-MM-DDTHH:MM:SSZ"`（不带毫秒）。

<a id="Std.Time.DateTime.ToIso8601With(TimeZone)"></a>
### string ToIso8601With(TimeZone tz)

[`src/libraries/z42.core/src/Time/DateTime.z42:253`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L253)

以给定固定偏移时区渲染本地墙钟时刻，后缀 `±HH:MM`（UTC 为 `Z`）。总是带毫秒。

<a id="Std.Time.DateTime.ParseIso8601(string)"></a>
### static DateTime ParseIso8601(string s)

[`src/libraries/z42.core/src/Time/DateTime.z42:278`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/DateTime.z42#L278)

解析 RFC 3339 / ISO 8601 子集，返回 UTC 时刻。

缺时区后缀按 UTC 处理；畸形输入抛 `ArgumentException`。接受的形状：
  YYYY-MM-DD                  → 当天 UTC 零点
  YYYY-MM-DDTHH:MM:SS         → 无后缀按 UTC
  YYYY-MM-DDTHH:MM:SSZ        → UTC 显式
  YYYY-MM-DDTHH:MM:SS.sss     → 1~9 位小数秒，截断到毫秒
  YYYY-MM-DDTHH:MM:SS±HH:MM   → 带偏移
  分隔符 `T` 也可以是单个空格。

