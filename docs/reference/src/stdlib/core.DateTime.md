<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.DateTime

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Time`

`src/libraries/z42.core/src/Time/DateTime.z42:5`

UTC 时刻值，内部单位 Unix epoch 毫秒（i64）。不可变：算术方法返回新对象。 `ToString()` 返回 Unix 毫秒数字串；可读输出用 `ToIso8601()`。

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

### DateTime(long unixMs)

`src/libraries/z42.core/src/Time/DateTime.z42:9`

由 Unix 毫秒构造。

### static DateTime UtcNow()

`src/libraries/z42.core/src/Time/DateTime.z42:19`

当前 UTC 时刻。

### static DateTime FromUnixMs(long unixMs)

`src/libraries/z42.core/src/Time/DateTime.z42:22`

由 Unix 毫秒构造。

### static DateTime UnixEpoch()

`src/libraries/z42.core/src/Time/DateTime.z42:27`

Unix 纪元（1970-01-01T00:00:00Z）。

### long UnixMs()

`src/libraries/z42.core/src/Time/DateTime.z42:32`

Unix 毫秒。

### int Year()

`src/libraries/z42.core/src/Time/DateTime.z42:38`

年份（Gregorian，含公元前返回负数；公历没有 0 年）。

### int Month()

`src/libraries/z42.core/src/Time/DateTime.z42:45`

月份，1..12。

### int Day()

`src/libraries/z42.core/src/Time/DateTime.z42:52`

日，1..31。

### int Hour()

`src/libraries/z42.core/src/Time/DateTime.z42:59`

时，0..23（UTC）。

### int Minute()

`src/libraries/z42.core/src/Time/DateTime.z42:64`

分，0..59（UTC）。

### int Second()

`src/libraries/z42.core/src/Time/DateTime.z42:69`

秒，0..59（UTC）。

### int Millisecond()

`src/libraries/z42.core/src/Time/DateTime.z42:74`

毫秒，0..999。

### int DayOfWeek()

`src/libraries/z42.core/src/Time/DateTime.z42:80`

星期几，0=Sunday .. 6=Saturday（对齐 .NET / JS `getDay()`）。

### int DayOfYear()

`src/libraries/z42.core/src/Time/DateTime.z42:87`

一年中的第几天，1..366。

### static bool IsLeapYear(int year)

`src/libraries/z42.core/src/Time/DateTime.z42:95`

是否闰年：能被 4 整除且（不能被 100 整除或能被 400 整除）。

### static int DaysInMonth(int year, int month)

`src/libraries/z42.core/src/Time/DateTime.z42:102`

给定年月的天数。Month 1..12；越界抛 `ArgumentException`。

### static DateTime Utc(int year, int month, int day, int hour, int minute, int second, int millis)

`src/libraries/z42.core/src/Time/DateTime.z42:117`

由显式日历分量构造 UTC 时刻。越界抛 `ArgumentException`。

### static DateTime UtcDate(int year, int month, int day)

`src/libraries/z42.core/src/Time/DateTime.z42:148`

给定日期的 UTC 零点。

### DateTime AddDays(int days)

`src/libraries/z42.core/src/Time/DateTime.z42:153`

加 `days` 天（可为负）。

### DateTime AddHours(int hours)

`src/libraries/z42.core/src/Time/DateTime.z42:158`

加 `hours` 小时。

### DateTime AddMinutes(int minutes)

`src/libraries/z42.core/src/Time/DateTime.z42:163`

加 `minutes` 分钟。

### DateTime AddSeconds(int seconds)

`src/libraries/z42.core/src/Time/DateTime.z42:168`

加 `seconds` 秒。

### DateTime AddMilliseconds(long millis)

`src/libraries/z42.core/src/Time/DateTime.z42:173`

加 `millis` 毫秒。

### DateTime AddMonths(int months)

`src/libraries/z42.core/src/Time/DateTime.z42:179`

加 `months` 个日历月；日号截断到目标月最后一天（1 月 31 日 + 1 月 = 2 月 28/29 日）。 时分秒毫秒原样保留。

### DateTime AddYears(int years)

`src/libraries/z42.core/src/Time/DateTime.z42:204`

加 `years` 年；闰年 2 月 29 日在非闰年截断为 2 月 28 日。

### TimeSpan Subtract(DateTime other)

`src/libraries/z42.core/src/Time/DateTime.z42:211`

与 `other` 的差值（TimeSpan）。

### DateTime Add(TimeSpan span)

`src/libraries/z42.core/src/Time/DateTime.z42:216`

加 TimeSpan。

### DateTime SubtractSpan(TimeSpan span)

`src/libraries/z42.core/src/Time/DateTime.z42:221`

减 TimeSpan。

### bool IsAfter(DateTime other)

`src/libraries/z42.core/src/Time/DateTime.z42:228`

是否晚于 `other`。

### bool IsBefore(DateTime other)

`src/libraries/z42.core/src/Time/DateTime.z42:230`

是否早于 `other`。

### bool Equals(DateTime other)

`src/libraries/z42.core/src/Time/DateTime.z42:232`

是否同一时刻。

### override string ToString()

`src/libraries/z42.core/src/Time/DateTime.z42:235`

返回 Unix 毫秒数字串。

### string ToIso8601()

`src/libraries/z42.core/src/Time/DateTime.z42:242`

格式化为 `"YYYY-MM-DDTHH:MM:SS.sssZ"`（毫秒 3 位，UTC `Z`）。

### string ToIso8601Basic()

`src/libraries/z42.core/src/Time/DateTime.z42:247`

格式化为 `"YYYY-MM-DDTHH:MM:SSZ"`（不带毫秒）。

### string ToIso8601With(TimeZone tz)

`src/libraries/z42.core/src/Time/DateTime.z42:252`

以给定固定偏移时区渲染本地墙钟时刻，后缀 `±HH:MM`（UTC 为 `Z`）。总是带毫秒。

### static DateTime ParseIso8601(string s)

`src/libraries/z42.core/src/Time/DateTime.z42:276`

解析 RFC 3339 / ISO 8601 子集，返回 UTC 时刻。缺时区后缀按 UTC 处理；畸形输入抛 `ArgumentException`。接受的形状： YYYY-MM-DD                  → 当天 UTC 零点 YYYY-MM-DDTHH:MM:SS         → 无后缀按 UTC YYYY-MM-DDTHH:MM:SSZ        → UTC 显式 YYYY-MM-DDTHH:MM:SS.sss     → 1~9 位小数秒，截断到毫秒 YYYY-MM-DDTHH:MM:SS±HH:MM   → 带偏移 分隔符 `T` 也可以是单个空格。

