<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.core/ -->

# z42.core.String

> 包路径 `src/libraries/z42.core/`；命名空间 `Std`

`src/libraries/z42.core/src/String.z42:33`

内置 `string` 类型的包装类：写 `string s` 声明的值，方法都派发到这里。

不可变；索引与长度按 Unicode 标量（`char`）计（字节数用 `ByteLength`）；越界与
非法实参抛可 catch 的 `Std.Exception`。sealed partial，实现 `IComparable` /
`IEquatable`。

```z42
public sealed partial class String : IComparable, IEquatable {
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

### string Insert(int startIndex, string value)

`src/libraries/z42.core/src/String.Edit.z42:14`

在 `startIndex` 处插入 `value`。`startIndex == Length` 等价追加；越界抛异常， 空 `value` 返回原串。

### string Remove(int startIndex)

`src/libraries/z42.core/src/String.Edit.z42:24`

删除 `[startIndex, Length)`，即截断到 `startIndex`。

### string Remove(int startIndex, int count)

`src/libraries/z42.core/src/String.Edit.z42:33`

删除 `[startIndex, startIndex+count)`；越界抛异常。

### string PadLeft(int totalWidth)

`src/libraries/z42.core/src/String.Edit.z42:45`

右对齐到 `totalWidth`，左侧补空格。串已达宽度则原样返回。

### string PadLeft(int totalWidth, char paddingChar)

`src/libraries/z42.core/src/String.Edit.z42:50`

右对齐到 `totalWidth`，左侧补 `paddingChar`。`totalWidth < 0` 抛异常。

### string PadRight(int totalWidth)

`src/libraries/z42.core/src/String.Edit.z42:65`

左对齐到 `totalWidth`，右侧补空格。

### string PadRight(int totalWidth, char paddingChar)

`src/libraries/z42.core/src/String.Edit.z42:70`

左对齐到 `totalWidth`，右侧补 `paddingChar`。`totalWidth < 0` 抛异常。

### string[] Split(string separator)

`src/libraries/z42.core/src/String.Split.z42:13`

按整串分隔符切分（从左到右，相邻/首尾分隔符产生空段）。空分隔符抛异常。

### string[] Split(string separator, int options)

`src/libraries/z42.core/src/String.Split.z42:26`

按分隔符切分，再按 `options` 后处理（`SplitOptions.TrimEntries` / `RemoveEmptyEntries` 的按位或）。

### string[] Split(char[] separators)

`src/libraries/z42.core/src/String.Split.z42:56`

按 `separators` 中任一单字符切分；空数组按空白切分。

### static string Join(string separator, params string[] values)

`src/libraries/z42.core/src/String.Split.z42:92`

以 `separator` 拼接各段。`params` 使 `Join("-", arr)` 与 `Join("-", "a", "b")` 都可用；零实参 → ""。

### static string Concat(params string[] values)

`src/libraries/z42.core/src/String.Split.z42:104`

无分隔符逐段拼接；零实参 → ""。

### static string Format(string format, params object[] args)

`src/libraries/z42.core/src/String.Split.z42:115`

把 `{0}` / `{1}` … 替换为对应实参的 `Convert.ToString()`。

### extern int Length { get; }

`src/libraries/z42.core/src/String.z42:37`

字符数（Unicode 标量个数），O(n)。需要 O(1) 的字节尺寸用 ByteLength。

### extern int ByteLength { get; }

`src/libraries/z42.core/src/String.z42:41`

UTF-8 字节数，O(1)。非 ASCII 串与 Length 不同（`"你好".Length == 2`， `"你好".ByteLength == 6`）。热路径尺寸查询用本属性。

### extern char CharAt(int index)

`src/libraries/z42.core/src/String.z42:44`

第 `index` 个字符（`s[index]` 同）。越界（含负数）抛 IndexOutOfRangeException。

### char this[int index] { get; }

`src/libraries/z42.core/src/String.z42:47`

索引器：等价于 `CharAt(index)`。`foreach (char c in s)` 也经此迭代。

### static extern string FromChars(char[] chars)

`src/libraries/z42.core/src/String.z42:50`

由 `char[]` 建串。空数组 → 空串。

### string ToString()

`src/libraries/z42.core/src/String.z42:55`

返回自身。

### extern bool Equals(object? other)

`src/libraries/z42.core/src/String.z42:58`

值相等比较。

### extern bool Equals(string other)

`src/libraries/z42.core/src/String.z42:60`

值相等比较。

### extern int CompareTo(string other)

`src/libraries/z42.core/src/String.z42:62`

序数比较，返回负 / 0 / 正。

### extern int GetHashCode()

`src/libraries/z42.core/src/String.z42:64`

内容哈希码。

### bool IsEmpty()

`src/libraries/z42.core/src/String.z42:71`

`Length == 0`。

### bool Contains(string value)

`src/libraries/z42.core/src/String.z42:76`

`value` 是否出现在本串中（等价 `IndexOf(value) >= 0`）。

### bool StartsWith(string prefix)

`src/libraries/z42.core/src/String.z42:81`

是否以 `prefix` 开头。空前缀恒 true。

### bool EndsWith(string suffix)

`src/libraries/z42.core/src/String.z42:92`

是否以 `suffix` 结尾。空后缀恒 true。

### int IndexOf(string value)

`src/libraries/z42.core/src/String.z42:107`

`value` 首次出现的索引（标量单位），未命中 -1；`IndexOf("")` → 0。

### int IndexOf(char value)

`src/libraries/z42.core/src/String.z42:126`

`value` 首次出现的索引，未命中 -1。

### int LastIndexOf(string value)

`src/libraries/z42.core/src/String.z42:136`

`value` 最后一次出现的索引（标量单位），未命中 -1；`LastIndexOf("")` → Length。

### int LastIndexOf(char value)

`src/libraries/z42.core/src/String.z42:154`

`value` 最后一次出现的索引，未命中 -1。

### string Replace(string oldValue, string newValue)

`src/libraries/z42.core/src/String.z42:163`

替换全部出现的 `oldValue` 为 `newValue`。`oldValue` 为空串抛异常。

### string Substring(int start)

`src/libraries/z42.core/src/String.z42:211`

截取 `[start, Length)`。

### string Substring(int start, int length)

`src/libraries/z42.core/src/String.z42:216`

截取 `[start, start+length)`；`start`/`length` 为负或越界抛异常。

### string ToLower()

`src/libraries/z42.core/src/String.z42:228`

ASCII 小写转换，非 ASCII 字母原样保留。

### string ToUpper()

`src/libraries/z42.core/src/String.z42:238`

ASCII 大写转换，非 ASCII 字母原样保留。

### extern char[] ToCharArray()

`src/libraries/z42.core/src/String.z42:250`

物化成 `char[]`；空串 → 空数组。

### static extern string ConcatParts(string[] parts, int count)

`src/libraries/z42.core/src/String.z42:257`

把 `parts[0..count)` 一次拼接；`count` 越界或元素非 string 由 VM 报错。

### string Trim()

`src/libraries/z42.core/src/String.z42:260`

裁掉首尾空白。

### string TrimStart()

`src/libraries/z42.core/src/String.z42:271`

裁掉前导空白。

### string TrimEnd()

`src/libraries/z42.core/src/String.z42:279`

裁掉尾随空白。

### string Trim(char trimChar)

`src/libraries/z42.core/src/String.z42:302`

裁掉首尾 `trimChar`。

### string Trim(char[] trimChars)

`src/libraries/z42.core/src/String.z42:309`

裁掉首尾 `trimChars` 中任一字符；空数组按空白裁。

### string TrimStart(char trimChar)

`src/libraries/z42.core/src/String.z42:322`

裁掉前导 `trimChar`。

### string TrimStart(char[] trimChars)

`src/libraries/z42.core/src/String.z42:329`

裁掉前导 `trimChars` 中任一字符；空数组按空白裁。

### string TrimEnd(char trimChar)

`src/libraries/z42.core/src/String.z42:339`

裁掉尾随 `trimChar`。

### string TrimEnd(char[] trimChars)

`src/libraries/z42.core/src/String.z42:346`

裁掉尾随 `trimChars` 中任一字符；空数组按空白裁。

### static bool IsNullOrEmpty(string? value)

`src/libraries/z42.core/src/String.z42:359`

`value` 为 null 或空串时为 true。

### static bool IsNullOrWhiteSpace(string? value)

`src/libraries/z42.core/src/String.z42:364`

`value` 为 null 或全空白时为 true。

