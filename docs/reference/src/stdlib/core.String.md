<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.String

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

[`src/libraries/z42.core/src/String.Edit.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.Edit.z42#L8)

`Std.String` 的 partial 碎片 —— 位置编辑（Insert / Remove）与对齐填充（PadLeft / PadRight）。

碎片归属规则（partial v1）：**同名方法组必须落在同一碎片**——键决策是每-fragment
的局部状态，跨碎片时两个同名方法会各自当「声明序首个」取裸键，后者静默覆盖前者。
故 Remove×2 / PadLeft×2 / PadRight×2 各自成组落在本碎片。

```z42
public sealed partial class String {
    string Insert(int startIndex, string value)
    string Remove(int startIndex)
    string Remove(int startIndex, int count)
    string PadLeft(int totalWidth)
    string PadLeft(int totalWidth, char paddingChar)
    string PadRight(int totalWidth)
    string PadRight(int totalWidth, char paddingChar)
    string[] Split(string separator)
    string[] Split(string separator, int options)
    string[] Split(char[] separators)
    static string Join(string separator, params string[] values)
    static string Concat(params string[] values)
    static string Format(string format, params object[] args)
    extern int Length { get; }
    extern int ByteLength { get; }
    extern char CharAt(int index)
    char this[int index] { get; }
    static extern string FromChars(char[] chars)
    string ToString()
    extern bool Equals(object? other)
    extern bool Equals(string other)
    extern int CompareTo(string other)
    extern int GetHashCode()
    bool IsEmpty()
    bool Contains(string value)
    bool StartsWith(string prefix)
    bool EndsWith(string suffix)
    int IndexOf(string value)
    int IndexOf(char value)
    int LastIndexOf(string value)
    int LastIndexOf(char value)
    string Replace(string oldValue, string newValue)
    string Substring(int start)
    string Substring(int start, int length)
    string ToLower()
    string ToUpper()
    extern char[] ToCharArray()
    static extern string ConcatParts(string[] parts, int count)
    string Trim()
    string TrimStart()
    string TrimEnd()
    string Trim(char trimChar)
    string Trim(char[] trimChars)
    string TrimStart(char trimChar)
    string TrimStart(char[] trimChars)
    string TrimEnd(char trimChar)
    string TrimEnd(char[] trimChars)
    static bool IsNullOrEmpty(string? value)
    static bool IsNullOrWhiteSpace(string? value)
}
```

成员：[`Insert`](#Std.String.Insert(int,string)) · [`Remove`](#Std.String.Remove(int)) · [`Remove`](#Std.String.Remove(int,int)) · [`PadLeft`](#Std.String.PadLeft(int)) · [`PadLeft`](#Std.String.PadLeft(int,char)) · [`PadRight`](#Std.String.PadRight(int)) · [`PadRight`](#Std.String.PadRight(int,char)) · [`Split`](#Std.String.Split(string)) · [`Split`](#Std.String.Split(string,int)) · [`Split`](#Std.String.Split(char[])) · [`Join`](#Std.String.Join(string,string[])) · [`Concat`](#Std.String.Concat(string[])) · [`Format`](#Std.String.Format(string,object[])) · [`Length`](#Std.String.Length) · [`ByteLength`](#Std.String.ByteLength) · [`CharAt`](#Std.String.CharAt(int)) · [`this`](#Std.String.this[int]) · [`FromChars`](#Std.String.FromChars(char[])) · [`ToString`](#Std.String.ToString()) · [`Equals`](#Std.String.Equals(object?)) · [`Equals`](#Std.String.Equals(string)) · [`CompareTo`](#Std.String.CompareTo(string)) · [`GetHashCode`](#Std.String.GetHashCode()) · [`IsEmpty`](#Std.String.IsEmpty()) · [`Contains`](#Std.String.Contains(string)) · [`StartsWith`](#Std.String.StartsWith(string)) · [`EndsWith`](#Std.String.EndsWith(string)) · [`IndexOf`](#Std.String.IndexOf(string)) · [`IndexOf`](#Std.String.IndexOf(char)) · [`LastIndexOf`](#Std.String.LastIndexOf(string)) · [`LastIndexOf`](#Std.String.LastIndexOf(char)) · [`Replace`](#Std.String.Replace(string,string)) · [`Substring`](#Std.String.Substring(int)) · [`Substring`](#Std.String.Substring(int,int)) · [`ToLower`](#Std.String.ToLower()) · [`ToUpper`](#Std.String.ToUpper()) · [`ToCharArray`](#Std.String.ToCharArray()) · [`ConcatParts`](#Std.String.ConcatParts(string[],int)) · [`Trim`](#Std.String.Trim()) · [`TrimStart`](#Std.String.TrimStart()) · [`TrimEnd`](#Std.String.TrimEnd()) · [`Trim`](#Std.String.Trim(char)) · [`Trim`](#Std.String.Trim(char[])) · [`TrimStart`](#Std.String.TrimStart(char)) · [`TrimStart`](#Std.String.TrimStart(char[])) · [`TrimEnd`](#Std.String.TrimEnd(char)) · [`TrimEnd`](#Std.String.TrimEnd(char[])) · [`IsNullOrEmpty`](#Std.String.IsNullOrEmpty(string?)) · [`IsNullOrWhiteSpace`](#Std.String.IsNullOrWhiteSpace(string?))

<a id="Std.String.Insert(int,string)"></a>
### string Insert(int startIndex, string value)

[`src/libraries/z42.core/src/String.Edit.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.Edit.z42#L13)

在 `startIndex` 处插入 `value`。`startIndex == Length` 等价追加；越界抛异常， 空 `value` 返回原串。

<a id="Std.String.Remove(int)"></a>
### string Remove(int startIndex)

[`src/libraries/z42.core/src/String.Edit.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.Edit.z42#L23)

删除 `[startIndex, Length)`，即截断到 `startIndex`。

<a id="Std.String.Remove(int,int)"></a>
### string Remove(int startIndex, int count)

[`src/libraries/z42.core/src/String.Edit.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.Edit.z42#L32)

删除 `[startIndex, startIndex+count)`；越界抛异常。

<a id="Std.String.PadLeft(int)"></a>
### string PadLeft(int totalWidth)

[`src/libraries/z42.core/src/String.Edit.z42:44`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.Edit.z42#L44)

右对齐到 `totalWidth`，左侧补空格。串已达宽度则原样返回。

<a id="Std.String.PadLeft(int,char)"></a>
### string PadLeft(int totalWidth, char paddingChar)

[`src/libraries/z42.core/src/String.Edit.z42:49`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.Edit.z42#L49)

右对齐到 `totalWidth`，左侧补 `paddingChar`。`totalWidth < 0` 抛异常。

<a id="Std.String.PadRight(int)"></a>
### string PadRight(int totalWidth)

[`src/libraries/z42.core/src/String.Edit.z42:64`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.Edit.z42#L64)

左对齐到 `totalWidth`，右侧补空格。

<a id="Std.String.PadRight(int,char)"></a>
### string PadRight(int totalWidth, char paddingChar)

[`src/libraries/z42.core/src/String.Edit.z42:69`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.Edit.z42#L69)

左对齐到 `totalWidth`，右侧补 `paddingChar`。`totalWidth < 0` 抛异常。

<a id="Std.String.Split(string)"></a>
### string[] Split(string separator)

[`src/libraries/z42.core/src/String.Split.z42:13`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.Split.z42#L13)

按整串分隔符切分（从左到右，相邻/首尾分隔符产生空段）。空分隔符抛异常。

<a id="Std.String.Split(string,int)"></a>
### string[] Split(string separator, int options)

[`src/libraries/z42.core/src/String.Split.z42:26`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.Split.z42#L26)

按分隔符切分，再按 `options` 后处理（`SplitOptions.TrimEntries` / `RemoveEmptyEntries` 的按位或）。

<a id="Std.String.Split(char[])"></a>
### string[] Split(char[] separators)

[`src/libraries/z42.core/src/String.Split.z42:56`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.Split.z42#L56)

按 `separators` 中任一单字符切分；空数组按空白切分。

<a id="Std.String.Join(string,string[])"></a>
### static string Join(string separator, params string[] values)

[`src/libraries/z42.core/src/String.Split.z42:93`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.Split.z42#L93)

以 `separator` 拼接各段。

`params` 使 `Join("-", arr)` 与 `Join("-", "a", "b")` 都可用；零实参 → ""。

<a id="Std.String.Concat(string[])"></a>
### static string Concat(params string[] values)

[`src/libraries/z42.core/src/String.Split.z42:105`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.Split.z42#L105)

无分隔符逐段拼接；零实参 → ""。

<a id="Std.String.Format(string,object[])"></a>
### static string Format(string format, params object[] args)

[`src/libraries/z42.core/src/String.Split.z42:116`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.Split.z42#L116)

把 `{0}` / `{1}` … 替换为对应实参的 `Convert.ToString()`。

<a id="Std.String.Length"></a>
### extern int Length { get; }

[`src/libraries/z42.core/src/String.z42:37`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L37)

字符数（Unicode 标量个数），O(n)。需要 O(1) 的字节尺寸用 ByteLength。

<a id="Std.String.ByteLength"></a>
### extern int ByteLength { get; }

[`src/libraries/z42.core/src/String.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L43)

UTF-8 字节数，O(1)。

非 ASCII 串与 Length 不同（`"你好".Length == 2`，`"你好".ByteLength == 6`）。
热路径尺寸查询用本属性。

<a id="Std.String.CharAt(int)"></a>
### extern char CharAt(int index)

[`src/libraries/z42.core/src/String.z42:46`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L46)

第 `index` 个字符（`s[index]` 同）。越界（含负数）抛 IndexOutOfRangeException。

<a id="Std.String.this[int]"></a>
### char this[int index] { get; }

[`src/libraries/z42.core/src/String.z42:49`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L49)

索引器：等价于 `CharAt(index)`。`foreach (char c in s)` 也经此迭代。

<a id="Std.String.FromChars(char[])"></a>
### static extern string FromChars(char[] chars)

[`src/libraries/z42.core/src/String.z42:52`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L52)

由 `char[]` 建串。空数组 → 空串。

<a id="Std.String.ToString()"></a>
### string ToString()

[`src/libraries/z42.core/src/String.z42:60`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L60)

返回自身。

<a id="Std.String.Equals(object?)"></a>
### extern bool Equals(object? other)

[`src/libraries/z42.core/src/String.z42:63`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L63)

值相等比较。

<a id="Std.String.Equals(string)"></a>
### extern bool Equals(string other)

[`src/libraries/z42.core/src/String.z42:65`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L65)

值相等比较。

<a id="Std.String.CompareTo(string)"></a>
### extern int CompareTo(string other)

[`src/libraries/z42.core/src/String.z42:67`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L67)

序数比较，返回负 / 0 / 正。

<a id="Std.String.GetHashCode()"></a>
### extern int GetHashCode()

[`src/libraries/z42.core/src/String.z42:69`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L69)

内容哈希码。

<a id="Std.String.IsEmpty()"></a>
### bool IsEmpty()

[`src/libraries/z42.core/src/String.z42:76`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L76)

`Length == 0`。

<a id="Std.String.Contains(string)"></a>
### bool Contains(string value)

[`src/libraries/z42.core/src/String.z42:81`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L81)

`value` 是否出现在本串中（等价 `IndexOf(value) >= 0`）。

<a id="Std.String.StartsWith(string)"></a>
### bool StartsWith(string prefix)

[`src/libraries/z42.core/src/String.z42:86`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L86)

是否以 `prefix` 开头。空前缀恒 true。

<a id="Std.String.EndsWith(string)"></a>
### bool EndsWith(string suffix)

[`src/libraries/z42.core/src/String.z42:97`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L97)

是否以 `suffix` 结尾。空后缀恒 true。

<a id="Std.String.IndexOf(string)"></a>
### int IndexOf(string value)

[`src/libraries/z42.core/src/String.z42:112`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L112)

`value` 首次出现的索引（标量单位），未命中 -1；`IndexOf("")` → 0。

<a id="Std.String.IndexOf(char)"></a>
### int IndexOf(char value)

[`src/libraries/z42.core/src/String.z42:131`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L131)

`value` 首次出现的索引，未命中 -1。

<a id="Std.String.LastIndexOf(string)"></a>
### int LastIndexOf(string value)

[`src/libraries/z42.core/src/String.z42:141`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L141)

`value` 最后一次出现的索引（标量单位），未命中 -1；`LastIndexOf("")` → Length。

<a id="Std.String.LastIndexOf(char)"></a>
### int LastIndexOf(char value)

[`src/libraries/z42.core/src/String.z42:159`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L159)

`value` 最后一次出现的索引，未命中 -1。

<a id="Std.String.Replace(string,string)"></a>
### string Replace(string oldValue, string newValue)

[`src/libraries/z42.core/src/String.z42:168`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L168)

替换全部出现的 `oldValue` 为 `newValue`。`oldValue` 为空串抛异常。

<a id="Std.String.Substring(int)"></a>
### string Substring(int start)

[`src/libraries/z42.core/src/String.z42:226`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L226)

截取 `[start, Length)`。

<a id="Std.String.Substring(int,int)"></a>
### string Substring(int start, int length)

[`src/libraries/z42.core/src/String.z42:231`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L231)

截取 `[start, start+length)`；`start`/`length` 为负或越界抛异常。

<a id="Std.String.ToLower()"></a>
### string ToLower()

[`src/libraries/z42.core/src/String.z42:243`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L243)

ASCII 小写转换，非 ASCII 字母原样保留。

<a id="Std.String.ToUpper()"></a>
### string ToUpper()

[`src/libraries/z42.core/src/String.z42:253`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L253)

ASCII 大写转换，非 ASCII 字母原样保留。

<a id="Std.String.ToCharArray()"></a>
### extern char[] ToCharArray()

[`src/libraries/z42.core/src/String.z42:265`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L265)

物化成 `char[]`；空串 → 空数组。

<a id="Std.String.ConcatParts(string[],int)"></a>
### static extern string ConcatParts(string[] parts, int count)

[`src/libraries/z42.core/src/String.z42:272`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L272)

把 `parts[0..count)` 一次拼接；`count` 越界或元素非 string 由 VM 报错。

<a id="Std.String.Trim()"></a>
### string Trim()

[`src/libraries/z42.core/src/String.z42:275`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L275)

裁掉首尾空白。

<a id="Std.String.TrimStart()"></a>
### string TrimStart()

[`src/libraries/z42.core/src/String.z42:286`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L286)

裁掉前导空白。

<a id="Std.String.TrimEnd()"></a>
### string TrimEnd()

[`src/libraries/z42.core/src/String.z42:294`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L294)

裁掉尾随空白。

<a id="Std.String.Trim(char)"></a>
### string Trim(char trimChar)

[`src/libraries/z42.core/src/String.z42:317`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L317)

裁掉首尾 `trimChar`。

<a id="Std.String.Trim(char[])"></a>
### string Trim(char[] trimChars)

[`src/libraries/z42.core/src/String.z42:324`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L324)

裁掉首尾 `trimChars` 中任一字符；空数组按空白裁。

<a id="Std.String.TrimStart(char)"></a>
### string TrimStart(char trimChar)

[`src/libraries/z42.core/src/String.z42:337`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L337)

裁掉前导 `trimChar`。

<a id="Std.String.TrimStart(char[])"></a>
### string TrimStart(char[] trimChars)

[`src/libraries/z42.core/src/String.z42:344`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L344)

裁掉前导 `trimChars` 中任一字符；空数组按空白裁。

<a id="Std.String.TrimEnd(char)"></a>
### string TrimEnd(char trimChar)

[`src/libraries/z42.core/src/String.z42:354`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L354)

裁掉尾随 `trimChar`。

<a id="Std.String.TrimEnd(char[])"></a>
### string TrimEnd(char[] trimChars)

[`src/libraries/z42.core/src/String.z42:361`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L361)

裁掉尾随 `trimChars` 中任一字符；空数组按空白裁。

<a id="Std.String.IsNullOrEmpty(string?)"></a>
### static bool IsNullOrEmpty(string? value)

[`src/libraries/z42.core/src/String.z42:374`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L374)

`value` 为 null 或空串时为 true。

<a id="Std.String.IsNullOrWhiteSpace(string?)"></a>
### static bool IsNullOrWhiteSpace(string? value)

[`src/libraries/z42.core/src/String.z42:379`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.core/src/String.z42#L379)

`value` 为 null 或全空白时为 true。

