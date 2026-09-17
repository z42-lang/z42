# Std.Time —— 时刻、时间段、计时器与时区

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.core/src/Time/`；命名空间 `Std.Time`
>
> 时间类型住在 **`z42.core`** 里，没有独立的时间包。core 是隐式 prelude，
> 写 `using Std.Time;` 即可，不必声明额外依赖。

五个类型：

| 类型 | 是什么 | 内部单位 |
|---|---|---|
| `DateTime` | UTC 时刻（挂钟时间，可能跳变） | Unix epoch 毫秒（i64） |
| `DateTimeOffset` | `DateTime` + 固定偏移时区 | 同上 + 分钟偏移 |
| `TimeSpan` | 时间段 | 纳秒（i64），范围约 ±292 年 |
| `Stopwatch` | 单调高精度计时器（测耗时） | 纳秒（i64） |
| `TimeZone` | **固定偏移**时区（无 DST、无 IANA tzdata） | 分钟（i32，东正西负） |

全部是引用类型（`sealed class`），**不可变**（`Stopwatch` 除外）；算术方法返回新对象。

z42 目前没有命名属性 getter，所有访问器都是方法：`UnixMs()` / `TotalSeconds()` /
`Elapsed()` / `Year()`。

## DateTime

```z42
public sealed class DateTime {
    public DateTime(long unixMs)

    // 工厂
    public static DateTime UtcNow()
    public static DateTime FromUnixMs(long unixMs)
    public static DateTime UnixEpoch()
    public static DateTime Utc(int year, int month, int day,
                               int hour, int minute, int second, int millis)
    public static DateTime UtcDate(int year, int month, int day)
    public static DateTime ParseIso8601(string s)

    // 谓词
    public static bool IsLeapYear(int year)
    public static int  DaysInMonth(int year, int month)

    // 访问器（全部按 UTC 取值）
    public long UnixMs()
    public int  Year()
    public int  Month()          // 1..12
    public int  Day()            // 1..31
    public int  Hour()           // 0..23
    public int  Minute()         // 0..59
    public int  Second()         // 0..59
    public int  Millisecond()    // 0..999
    public int  DayOfWeek()      // 0=Sunday .. 6=Saturday
    public int  DayOfYear()      // 1..366

    // 算术
    public DateTime AddDays(int days)
    public DateTime AddHours(int hours)
    public DateTime AddMinutes(int minutes)
    public DateTime AddSeconds(int seconds)
    public DateTime AddMilliseconds(long millis)
    public DateTime AddMonths(int months)
    public DateTime AddYears(int years)
    public TimeSpan Subtract(DateTime other)
    public DateTime Add(TimeSpan span)
    public DateTime SubtractSpan(TimeSpan span)

    // 比较
    public bool IsAfter(DateTime other)
    public bool IsBefore(DateTime other)
    public bool Equals(DateTime other)

    // 格式化
    public string ToIso8601()              // "2026-09-17T14:32:31.789Z"
    public string ToIso8601Basic()         // "2026-09-17T14:32:31Z"
    public string ToIso8601With(TimeZone tz)
    public override string ToString()      // Unix 毫秒的十进制串
}
```

几条实测行为：

- `ToString()` 返回的是 **Unix 毫秒数字串**（`"1789655551789"`），不是 ISO 串。要可读
  输出请显式调 `ToIso8601()`。
- 负 epoch 正常工作：`FromUnixMs(-1)` → `1969-12-31T23:59:59.999Z`。
- `AddMonths` / `AddYears` 的 day-of-month **截断到目标月最后一天**：
  `2026-01-31 + 1 月` → `2026-02-28`；`2024-02-29 + 1 年` → `2025-02-28`。时分秒毫秒原样保留。
- `Utc(...)` 越界抛 `ArgumentException`（月 1..12、日按该月实际天数、时 0..23、
  分/秒 0..59、毫秒 0..999）；`DaysInMonth` 月份越界同样抛。
- `Year()` 用 Gregorian 公历（含公元前，返回负数年；公历没有 0 年）。

### ParseIso8601

接受的形状（RFC 3339 / ISO 8601 子集）：

```
YYYY-MM-DD                      → 当天 UTC 零点
YYYY-MM-DDTHH:MM:SS             → 无后缀，按 UTC
YYYY-MM-DDTHH:MM:SSZ
YYYY-MM-DDTHH:MM:SS.sss         → 1~9 位小数秒，截断到毫秒
YYYY-MM-DDTHH:MM:SS±HH:MM       → 也接受 ±HHMM 和 ±HH
```

- 日期与时间之间的分隔符可以是 `T`、`t` 或**一个空格**；`Z` 也接受小写 `z`。
- 缺时区后缀按 UTC 处理。
- 闰秒 `:60` 接受，但归到 `:59`。
- `2026-02-31` 这种日期会被拒（按该月实际天数校验），不会静默滚到下月。
- 任何畸形输入抛 `ArgumentException`，消息指明位置，例如
  `ParseIso8601: expected 'T' or space at position 10, got 'X'`、
  `ParseIso8601: trailing characters after 'Z': 'x'`。

## TimeSpan

```z42
public sealed class TimeSpan {
    public TimeSpan(long nanoseconds)

    public static TimeSpan Zero()
    public static TimeSpan FromNanoseconds(long ns)
    public static TimeSpan FromMilliseconds(long ms)
    public static TimeSpan FromSeconds(double sec)
    public static TimeSpan FromMinutes(double min)
    public static TimeSpan FromHours(double hrs)

    public long   TotalNanoseconds()
    public long   TotalMilliseconds()
    public double TotalSeconds()
    public double TotalMinutes()
    public double TotalHours()

    public TimeSpan Add(TimeSpan other)
    public TimeSpan Subtract(TimeSpan other)

    public bool IsLessThan(TimeSpan other)
    public bool IsLessEqual(TimeSpan other)
    public bool IsGreaterThan(TimeSpan other)
    public bool IsGreaterEqual(TimeSpan other)
    public bool Equals(TimeSpan other)

    public override string ToString()    // "1500000000ns"
}
```

注意工厂方法的参数类型不齐：`FromNanoseconds` / `FromMilliseconds` 收 `long`，
`FromSeconds` / `FromMinutes` / `FromHours` 收 `double`。
`TotalMilliseconds()` 是整数除法（向零截断），亚毫秒部分丢失。

## Stopwatch

单调时钟，测耗时用。挂钟跳变（NTP 调时）不影响它。

```z42
public sealed class Stopwatch {
    public Stopwatch()
    public static Stopwatch StartNew()

    public bool     IsRunning()
    public void     Start()
    public void     Stop()
    public void     Restart()
    public TimeSpan Elapsed()
}
```

| 成员 | 说明 |
|---|---|
| `Stopwatch()` | 新建，**未启动**，`Elapsed()` 为 0 |
| `StartNew()` | 新建并立即启动 |
| `Start` | 已在跑则无操作；否则从当前时刻继续累计 |
| `Stop` | 未在跑则无操作；否则把本段累加进总量 |
| `Restart` | 清零并重新开始计时 |
| `Elapsed` | 累计时长；跑着的时候每次调用都变大，停下后稳定 |

## TimeZone

**固定偏移**时区：一个带符号的分钟偏移加一个名字。没有 DST 切换、没有 IANA tzdata。

```z42
public sealed class TimeZone {
    public static TimeZone Utc()
    public static TimeZone FromOffsetMinutes(int offsetMinutes)
    public static TimeZone FromName(string code)

    public int    OffsetMinutes()
    public string Name()
    public string ToOffsetString()    // "±HH:MM"，偏移为 0 时是 "Z"
}
```

- 构造函数是私有的，只能走三个工厂。
- `FromOffsetMinutes` 东正西负（`+330` = UTC+5:30，`-480` = UTC-8:00）；超出 ±840
  抛 `ArgumentException`。这样造出来的 `Name()` 就是 `"+05:30"` 这种偏移串。
- `FromName` **大小写不敏感**，未知代码返回 `null`（不抛异常），命中时 `Name()` 是
  大写后的代码。

支持的短代码（共 24 个，`EST`/`EDT` 这类标准时与夏令时**分开列**，由调用方按日期自己挑）：

| 组 | 代码 |
|---|---|
| UTC 族 | `UTC` `GMT` `Z`（均 +00:00） |
| 美洲 | `EST` −05:00 · `EDT` −04:00 · `CST` −06:00 · `CDT` −05:00 · `MST` −07:00 · `MDT` −06:00 · `PST` −08:00 · `PDT` −07:00 |
| 欧洲 | `CET` +01:00 · `CEST` +02:00 · `EET` +02:00 · `EEST` +03:00 · `BST` +01:00（英国夏令时） |
| 亚洲 / 大洋洲 | `JST` +09:00 · `KST` +09:00 · `IST` +05:30（印度） · `ICT` +07:00 · `AEST` +10:00 · `AEDT` +11:00 · `NZST` +12:00 · `NZDT` +13:00 |

`IST`（印度 / 以色列 / 爱尔兰）与 `BST`（英国 / 孟加拉）是现实里的歧义缩写，这张表各取
一种解释；要另一种就用 `FromOffsetMinutes`。

## DateTimeOffset

`DateTime`（UTC 时刻）与 `TimeZone`（偏移）的配对。

```z42
public sealed class DateTimeOffset {
    public DateTimeOffset(DateTime utc, TimeZone tz)
    public static DateTimeOffset Now(TimeZone tz)
    public static DateTimeOffset FromLocal(int year, int month, int day,
                                           int hour, int minute, int second,
                                           int millisecond, TimeZone tz)
    public static DateTimeOffset Parse(string s)

    public DateTime UtcDateTime()
    public DateTime LocalDateTime()
    public TimeZone Offset()

    public int Year()
    public int Month()
    public int Day()
    public int Hour()
    public int Minute()
    public int Second()
    public int Millisecond()
    public int DayOfWeek()
    public int DayOfYear()

    public string ToIso8601()
    public override string ToString()     // 同 ToIso8601()

    public bool Equals(DateTimeOffset other)        // 只比 UTC 时刻
    public bool EqualsExact(DateTimeOffset other)   // UTC 时刻 + 偏移都要相同
    public bool IsAfter(DateTimeOffset other)
    public bool IsBefore(DateTimeOffset other)

    public TimeSpan       Subtract(DateTimeOffset other)
    public DateTimeOffset Add(TimeSpan span)
    public DateTimeOffset SubtractSpan(TimeSpan span)
}
```

- 九个日历访问器取的都是**本地**（已加偏移）字段。
- 构造函数任一参数为 `null` 抛 `ArgumentNullException`；`FromLocal` 的 `tz` 为 `null`
  同样抛。
- `Parse` 要求**必须带偏移后缀**（`Z` / `±HH:MM` / `±HHMM` / `±HH`），缺后缀抛
  `FormatException`（注意与 `DateTime.ParseIso8601` 不同 —— 后者缺后缀按 UTC 处理，
  且抛 `ArgumentException`）。
- `LocalDateTime()` 返回的是「本地墙钟读数塞进一个 `DateTime`」，它自己**不带偏移信息**，
  对它调 `ToIso8601()` 会得到带 `Z` 的串。要渲染带偏移的串请用
  `DateTimeOffset.ToIso8601()`。

## 原始时钟

`Std.Runtime.Clock`（`src/libraries/z42.core/src/Clock.z42`）是 VM 两个时钟原语的唯一
声明点。上面所有类型都建在它之上；只有在需要裸数值时才直接用它。

```z42
namespace Std.Runtime;

public static class Clock {
    public static extern long WallMillis();   // 挂钟，Unix 毫秒（可能跳变）
    public static extern long MonoNanos();    // 单调高精度纳秒（不跳变）
}
```

## 用法

```z42
using Std.IO;
using Std.Time;

void Main() {
    DateTime now = DateTime.UtcNow();
    Console.WriteLine(now.ToIso8601());                  // 2026-09-17T09:09:58.748Z

    TimeZone jst = TimeZone.FromName("JST");
    Console.WriteLine(now.ToIso8601With(jst));           // ...+09:00

    DateTime deadline = DateTime.Utc(2026, 12, 31, 23, 59, 59, 0);
    TimeSpan left = deadline.Subtract(now);
    Console.WriteLine("剩 " + left.TotalHours().ToString() + " 小时");

    Stopwatch sw = Stopwatch.StartNew();
    DoWork();
    sw.Stop();
    Console.WriteLine("耗时 " + sw.Elapsed().TotalMilliseconds().ToString() + " ms");

    DateTime parsed = DateTime.ParseIso8601("2026-09-17T14:32:31.123+05:30");
    Console.WriteLine(parsed.ToIso8601());               // 2026-09-17T09:02:31.123Z

    DateTimeOffset local = DateTimeOffset.FromLocal(2026, 9, 17, 23, 32, 31, 789, jst);
    Console.WriteLine(local.ToIso8601());                // 2026-09-17T23:32:31.789+09:00
    Console.WriteLine(local.UtcDateTime().ToIso8601());  // 2026-09-17T14:32:31.789Z
}
```

## 不支持

- **IANA tzdata / DST 自动切换**：没有 `America/New_York` 这类命名时区，也没有夏令时
  切换计算。只有固定偏移 + 24 个手维护的短代码；标准时与夏令时得自己按日期挑
  （`EST` vs `EDT`）。因此夏令时切换日的歧义 / 不存在本地时间也无从处理。
- **本地时区自动探测**：没有「取系统当前时区」的 API，时区必须显式给出。
- **ISO-8601 之外的格式化 / 解析**：没有 `strftime` 风格或 C# format string 的
  `ToString(format)`，也没有对应的自定义解析。
- **`DateTime` 的本地日历访问器**：`Year()` 等全按 UTC；要本地字段请用
  `DateTimeOffset`。
- **周 / 季度 / ISO 周号**：没有 `WeekOfYear` / `Quarter` 之类。
- **`Sleep` / `Timer`**：不在这里，见 `Std.Threading`（`Thread.Sleep(ms)` 与 `Timer`）。
- **亚毫秒精度的 `DateTime`**：`DateTime` 是毫秒分辨率，解析时更高精度的小数秒会被截断。
  要纳秒用 `TimeSpan` / `Stopwatch`。
- **运算符重载**：`DateTime` / `TimeSpan` 不支持 `+` `-` `<` `==`，一律用
  `Add` / `Subtract` / `IsBefore` / `Equals` 等方法。
