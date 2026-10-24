<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.TimeZone

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Time`

[`src/libraries/z42.core/src/Time/TimeZone.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeZone.z42#L8)

固定偏移时区：一个带符号的分钟偏移加一个名字。

没有 DST、没有 IANA tzdata。短代码大小写不敏感；任意固定偏移用
`FromOffsetMinutes`。私有构造，只能走工厂。

```z42
public sealed class TimeZone {
    static TimeZone Utc()
    static TimeZone FromOffsetMinutes(int offsetMinutes)
    static TimeZone? FromName(string code)
    int OffsetMinutes()
    string Name()
    string ToOffsetString()
}
```

成员：[`Utc`](#Std.Time.TimeZone.Utc()) · [`FromOffsetMinutes`](#Std.Time.TimeZone.FromOffsetMinutes(int)) · [`FromName`](#Std.Time.TimeZone.FromName(string)) · [`OffsetMinutes`](#Std.Time.TimeZone.OffsetMinutes()) · [`Name`](#Std.Time.TimeZone.Name()) · [`ToOffsetString`](#Std.Time.TimeZone.ToOffsetString())

<a id="Std.Time.TimeZone.Utc()"></a>
### static TimeZone Utc()

[`src/libraries/z42.core/src/Time/TimeZone.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeZone.z42#L20)

UTC（+00:00）。

<a id="Std.Time.TimeZone.FromOffsetMinutes(int)"></a>
### static TimeZone FromOffsetMinutes(int offsetMinutes)

[`src/libraries/z42.core/src/Time/TimeZone.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeZone.z42#L23)

由任意固定偏移构造（分钟，东正西负，+330 = UTC+5:30）。超出 ±840 抛 `ArgumentException`。

<a id="Std.Time.TimeZone.FromName(string)"></a>
### static TimeZone? FromName(string code)

[`src/libraries/z42.core/src/Time/TimeZone.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeZone.z42#L37)

按短代码查找（大小写不敏感），未知代码返回 null。

支持的代码：UTC / GMT / Z；美洲 EST/EDT/CST/CDT/MST/MDT/PST/PDT（标准时与
夏令时分开列，调用方按日期自己挑）；欧洲 CET/CEST/EET/EEST；亚洲 / 大洋洲
JST/KST/IST/ICT/AEST/AEDT/NZST/NZDT。

<a id="Std.Time.TimeZone.OffsetMinutes()"></a>
### int OffsetMinutes()

[`src/libraries/z42.core/src/Time/TimeZone.z42:47`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeZone.z42#L47)

分钟偏移（东正西负）。

<a id="Std.Time.TimeZone.Name()"></a>
### string Name()

[`src/libraries/z42.core/src/Time/TimeZone.z42:49`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeZone.z42#L49)

短代码或 `"+05:30"` 偏移串。

<a id="Std.Time.TimeZone.ToOffsetString()"></a>
### string ToOffsetString()

[`src/libraries/z42.core/src/Time/TimeZone.z42:52`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/TimeZone.z42#L52)

渲染为 `±HH:MM`（偏移为 0 时是 `"Z"`）。

