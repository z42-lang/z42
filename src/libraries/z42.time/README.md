# z42.time — UTC 时间 + 计时

## 职责

UTC 时刻（`DateTime`）、时间段（`TimeSpan`）、单调高精度计时器（`Stopwatch`）。
**纯脚本 + 单一 builtin 桥接**：内部存储 `long` (i64)，通过 `__time_*` 系列
builtin 拿 OS 时钟，业务逻辑全部脚本化。

本包**不含**以下内容（见 Deferred 段）：
- 用户可调用的日历分解（年月日 getter；内部 `_civilFromDays` 算法已落地，仅用于 ISO 格式化）
- ISO-8601 之外的格式化（`strftime` / C# format string）、ISO 解析
- `Sleep`（已落地 z42.threading.Thread.Sleep）/ `Timer` / `Delay`（异步定时器）
- `DateTimeOffset`（封装 DateTime + TimeZone 的便利类型；当前用户手写 `dt.ToIso8601With(tz)` 凑合）
- Timezone database（IANA tzdata；当前只有手维护的 ~22 个短代码 + 任意 fixed offset）

设计参考：详见 [`docs/design/stdlib/time.md`](../../../docs/design/stdlib/time.md)
（如不存在等待 spec 落地）。

## src/ 核心文件

| 文件 | 类型 | 说明 |
|------|------|------|
| `TimeSpan.z42`  | `struct TimeSpan`  | 时间段值，内部 i64 ns；factory + 总量访问器 |
| `DateTime.z42`  | `sealed class DateTime`  | UTC 时刻值，内部 i64 Unix epoch ms；factory + 解构访问器 + `ToIso8601()` / `ToIso8601Basic()` / `ToIso8601With(tz)` |
| `DateTimeOffset.z42` | `sealed class DateTimeOffset` | 带 UTC 偏移的时刻 + `Parse`（从 DateTime.z42 拆出，review §6） |
| `_DateTimeHelpers.z42` | `static class _DateTimeHelpers` | 日历换算（civil↔days）/ ISO 解析·格式化 纯静态辅助（从 DateTime.z42 拆出，review §6） |
| `TimeZone.z42`  | `class TimeZone`   | 固定 offset 时区 + 短代码查表（UTC/GMT/EST/PST/JST/IST/...）；`FromName` / `FromOffsetMinutes` / `Utc` factory。**无 DST、无 IANA tzdata**（详 Deferred） |
| `Stopwatch.z42` | `class Stopwatch`  | 单调高精度计时器（不受系统时钟跳变影响），基于 `__time_now_mono_ns` |

## 入口点

### `Std.Time.TimeSpan`

```z42
// 工厂
TimeSpan.FromMilliseconds(long ms) -> TimeSpan
TimeSpan.FromSeconds(double s) -> TimeSpan
TimeSpan.FromNanoseconds(long ns) -> TimeSpan
TimeSpan.Zero() -> TimeSpan

// 访问器（统一前缀 `Total*` 表示"该单位下的全部时长"）
TimeSpan.TotalNanoseconds() -> long
TimeSpan.TotalMilliseconds() -> long
TimeSpan.TotalSeconds() -> double
```

### `Std.Time.DateTime`

```z42
// 工厂
DateTime.UtcNow() -> DateTime                  // OS 当前 UTC 时刻
DateTime.FromUnixMs(long ms) -> DateTime       // 从 Unix epoch ms 构造
DateTime.UnixEpoch() -> DateTime               // 1970-01-01T00:00:00Z

// 访问器
DateTime.UnixMs() -> long                      // since Unix epoch

// ISO-8601 格式化（add-datetime-iso8601 2026-05-26；UTC `Z` 后缀硬编码）
DateTime.ToIso8601()       -> string           // "YYYY-MM-DDTHH:MM:SS.sssZ"
DateTime.ToIso8601Basic()  -> string           // "YYYY-MM-DDTHH:MM:SSZ"（无毫秒）
```

### `Std.Time.Stopwatch`

```z42
// 工厂
Stopwatch.StartNew() -> Stopwatch              // 创建并立即开始

// 控制
Stopwatch.Start() -> void
Stopwatch.Stop() -> void
Stopwatch.Restart() -> void                    // Reset + Start

// 访问器
Stopwatch.IsRunning() -> bool
Stopwatch.Elapsed() -> TimeSpan                // 累计运行时长
```

> 注：z42 当前不支持命名属性 getter（`Name { get { ... } }`），所有访问器
> 均为方法形式 `Foo()` 而非 `Foo`。后续 spec 提案 add-property-getter
> 落地后可平滑过渡。

## 依赖关系

依赖 `z42.core`（`long` / `double` primitive + `Std.Object`）；无其他 stdlib
依赖。通过 VM 内 `__time_unix_ms` / `__time_now_mono_ns` builtin 桥接 OS 时钟
（POSIX `clock_gettime` / Win32 `GetSystemTimeAsFileTime`）。

## Deferred / Future Work

按 ROI 大致排序：

### Calendar decomposition (Year/Month/Day/Hour/Min/Sec)

- **来源**：所有用户场景（日志格式化 / 数据库导入 / 报表）
- **触发原因**：需要 Gregorian calendar 算法（>200 LOC，含 leap year + 4/100/400 rule）；早期 stdlib 只暴露 epoch ms 是有意收窄
- **前置依赖**：spec 决策"calendar API shape"（C# / Java / Rust 都不一致）

### Formatting / Parsing

- ~~`DateTime.ToIsoString()`~~ ✅ **已落地** 2026-05-26 `add-datetime-iso8601` — `ToIso8601()` + `ToIso8601Basic()`
- `DateTime.ParseIso(string)` / `Format(string pattern)`
- **来源**：与 calendar decomposition 同 spec 一起做
- **触发原因**：format string 子语法（strftime-style vs C# style）需要单独设计

### TimeZone / DateTimeOffset

- **来源**：跨时区应用
- **触发原因**：需要 IANA tzdata 数据库 + 时区 ID 解析；要么 embed 全表（~600KB），要么调 OS API（每平台不一样）
- **前置依赖**：z42.os syscall 抽象 + 数据资源打包机制

### Sleep / Delay / Timer

- **来源**：基础控制流
- **触发原因**：阻塞 `Sleep(TimeSpan)` 简单（builtin），但 async `Delay` + `Timer` 需要 async runtime
- **前置依赖**：z42 async / await（roadmap 0.8.x）

### High-resolution timestamp (`Ticks`)

- **来源**：BCL `DateTime.Ticks` (100ns units)
- **触发原因**：z42 用 ms 精度 + 单独 `__time_now_mono_ns` 已经覆盖；只在跨 .NET interop 时才需要
- **现状**：不阻塞，按需

## 测试

`tests/`：3 个 `.z42` 测试文件 ——

- `datetime.z42` — `UtcNow` / `FromUnixMs` / `UnixEpoch` round-trip + 边界
- `timespan.z42` — factory + 累计运算 + 单位转换精度
- `stopwatch.z42` — `Start` / `Stop` / `Restart` + `Elapsed` 单调性

运行：

```bash
z42 xtask.zpkg test lib         # 完整 stdlib 测试套
```
