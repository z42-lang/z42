<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core._DateTimeHelpers

> 包路径 `src/libraries/z42.core/`；命名空间 `Std.Time`

[`src/libraries/z42.core/src/Time/_DateTimeHelpers.z42:4`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/_DateTimeHelpers.z42#L4)

日历换算 / ISO 解析·格式化的纯静态辅助（从 DateTime.z42 拆出）。

```z42
public static class _DateTimeHelpers {
    static long _daysFromCivil(long y, long m, long d)
    static int _parseFixed(string s, int start, int width, string fieldName)
    static void _expect(string s, int pos, char want)
    static bool _isDigit(char c)
    static string _charStr(char c)
    static long[] _civilFromDays(long days)
    static long _floorDiv(long a, long b)
    static long _floorMod(long a, long b)
    static string _pad2(long n)
    static string _pad3(long n)
    static string _pad4(long n)
}
```

成员：[`_daysFromCivil`](#Std.Time._DateTimeHelpers._daysFromCivil(long,long,long)) · [`_parseFixed`](#Std.Time._DateTimeHelpers._parseFixed(string,int,int,string)) · [`_expect`](#Std.Time._DateTimeHelpers._expect(string,int,char)) · [`_isDigit`](#Std.Time._DateTimeHelpers._isDigit(char)) · [`_charStr`](#Std.Time._DateTimeHelpers._charStr(char)) · [`_civilFromDays`](#Std.Time._DateTimeHelpers._civilFromDays(long)) · [`_floorDiv`](#Std.Time._DateTimeHelpers._floorDiv(long,long)) · [`_floorMod`](#Std.Time._DateTimeHelpers._floorMod(long,long)) · [`_pad2`](#Std.Time._DateTimeHelpers._pad2(long)) · [`_pad3`](#Std.Time._DateTimeHelpers._pad3(long)) · [`_pad4`](#Std.Time._DateTimeHelpers._pad4(long))

<a id="Std.Time._DateTimeHelpers._daysFromCivil(long,long,long)"></a>
### static long _daysFromCivil(long y, long m, long d)

[`src/libraries/z42.core/src/Time/_DateTimeHelpers.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/_DateTimeHelpers.z42#L8)

(y, m, d) → 自 Unix 纪元（1970 年 1 月 1 日）起的天数。

Howard Hinnant 的 days_from_civil 算法。

<a id="Std.Time._DateTimeHelpers._parseFixed(string,int,int,string)"></a>
### static int _parseFixed(string s, int start, int width, string fieldName)

[`src/libraries/z42.core/src/Time/_DateTimeHelpers.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/_DateTimeHelpers.z42#L23)

从 `start` 起读 `width` 位十进制数字；越界或非数字抛 ArgumentException。

`fieldName` 只进报错信息（"ParseIso8601: <fieldName> …"）。

<a id="Std.Time._DateTimeHelpers._expect(string,int,char)"></a>
### static void _expect(string s, int pos, char want)

[`src/libraries/z42.core/src/Time/_DateTimeHelpers.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/_DateTimeHelpers.z42#L43)

断言 `s` 的 `pos` 处恰是 `want`；越界或不符抛 ArgumentException（报 got）。

<a id="Std.Time._DateTimeHelpers._isDigit(char)"></a>
### static bool _isDigit(char c)

[`src/libraries/z42.core/src/Time/_DateTimeHelpers.z42:53`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/_DateTimeHelpers.z42#L53)

是否 ASCII 十进制数字（'0'..'9'）。

<a id="Std.Time._DateTimeHelpers._charStr(char)"></a>
### static string _charStr(char c)

[`src/libraries/z42.core/src/Time/_DateTimeHelpers.z42:56`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/_DateTimeHelpers.z42#L56)

单个 char 包成 string（报错信息里拼字符字面量用）。

<a id="Std.Time._DateTimeHelpers._civilFromDays(long)"></a>
### static long[] _civilFromDays(long days)

[`src/libraries/z42.core/src/Time/_DateTimeHelpers.z42:65`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/_DateTimeHelpers.z42#L65)

Unix 纪元起的天数 → [year, month, day]。

Howard Hinnant 的 civil_from_days 算法。

<a id="Std.Time._DateTimeHelpers._floorDiv(long,long)"></a>
### static long _floorDiv(long a, long b)

[`src/libraries/z42.core/src/Time/_DateTimeHelpers.z42:86`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/_DateTimeHelpers.z42#L86)

向负无穷舍入的除法（Unix 纪元之前的时刻需要；C 风格 `/` 向零截断会算错日界）。

<a id="Std.Time._DateTimeHelpers._floorMod(long,long)"></a>
### static long _floorMod(long a, long b)

[`src/libraries/z42.core/src/Time/_DateTimeHelpers.z42:94`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/_DateTimeHelpers.z42#L94)

与 `_floorDiv` 同口径的取模（余数与除数同号）。

<a id="Std.Time._DateTimeHelpers._pad2(long)"></a>
### static string _pad2(long n)

[`src/libraries/z42.core/src/Time/_DateTimeHelpers.z42:100`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/_DateTimeHelpers.z42#L100)

左补零到 2 位（月 / 日、时 / 分 / 秒）。

<a id="Std.Time._DateTimeHelpers._pad3(long)"></a>
### static string _pad3(long n)

[`src/libraries/z42.core/src/Time/_DateTimeHelpers.z42:106`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/_DateTimeHelpers.z42#L106)

左补零到 3 位（毫秒）。

<a id="Std.Time._DateTimeHelpers._pad4(long)"></a>
### static string _pad4(long n)

[`src/libraries/z42.core/src/Time/_DateTimeHelpers.z42:113`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/Time/_DateTimeHelpers.z42#L113)

左补零到 4 位（年份）；负数先输出 `-` 再按绝对值补零。

