# `Std.String` —— 字符串的方法面

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.core/`（`String.z42` / `String.Split.z42` / `String.Edit.z42`）；
> 命名空间 `Std`

`Std.String` 是内置 `string` 类型的包装类：写 `string s` 声明的值，方法派发到这里。
类是 `sealed partial`，实现 `IComparable` / `IEquatable`。

本页只列**方法与属性**。字面量语法（普通串 / 插值串 `$"..."` / 原始串 `"""..."""`、
转义表、不能对字符串 `foreach`）见[字符串](../language/strings.md)，此处不重复。

三条贯穿全部方法的语义：

- **不可变**：没有任何方法原地修改接收者，一律返回新串。
- **索引与长度以 Unicode 标量（`char`）计**，不是 UTF-8 字节。要字节数用 `ByteLength`。
- **越界与非法实参抛可 catch 的 `Std.Exception`**，不是 VM abort。

## 长度与取字符

```z42
public extern int  Length     { get; }     // Unicode 标量个数，O(n)
public extern int  ByteLength { get; }     // UTF-8 字节数，O(1)
public extern char CharAt(int index);
public bool IsEmpty();
public extern char[] ToCharArray();
```

| 成员 | 说明 |
|---|---|
| `Length` | 字符数。`"你好".Length == 2`。是**属性**，不写括号 |
| `ByteLength` | UTF-8 字节数。`"你好".ByteLength == 6`。热路径尺寸查询用它 |
| `CharAt(index)` | 第 `index` 个字符。越界抛 `Std.Exception`：`__str_char_at: index 9 out of range (length 3)` |
| `IsEmpty()` | `Length == 0`。是**方法**，写括号 |
| `ToCharArray()` | 物化成 `char[]`；空串 → 空数组 |

## 查找与判定

```z42
public bool Contains(string value);
public bool StartsWith(string prefix);
public bool EndsWith(string suffix);
public int  IndexOf(string value);
public int  IndexOf(char value);
public int  LastIndexOf(string value);
public int  LastIndexOf(char value);
```

| 方法 | 边界行为 |
|---|---|
| `StartsWith` / `EndsWith` | 空 needle → `true` |
| `IndexOf` | 未命中 `-1`；`IndexOf("")` → `0` |
| `LastIndexOf` | 未命中 `-1`；`LastIndexOf("")` → `Length`（与 .NET Core 一致）|

`Contains(v)` 就是 `IndexOf(v) >= 0`。没有 `Contains(char)` 重载，也没有带起始位置或
比较模式的重载。

## 截取与编辑

```z42
public string Substring(int start);
public string Substring(int start, int length);
public string Insert(int startIndex, string value);
public string Remove(int startIndex);
public string Remove(int startIndex, int count);
public string Replace(string oldValue, string newValue);
```

| 方法 | 语义与异常 |
|---|---|
| `Substring(start)` | `[start, Length)` |
| `Substring(start, length)` | `[start, start+length)`。`start < 0`、`length < 0` 或越界 → `Substring: out of range (start=…, length=…, Length=…)` |
| `Insert(i, v)` | 在 `i` 处插入；`i == Length` 等价追加；空 `v` 返回原串；越界 → `Insert: startIndex out of range (…)` |
| `Remove(i)` | 截断到 `i`，即删除 `[i, Length)` |
| `Remove(i, count)` | 删除 `[i, i+count)`；越界 → `Remove: out of range (…)` |
| `Replace(old, new)` | 替换**全部**出现；`old` 为空串 → `Replace: oldValue cannot be empty` |

## 大小写与裁剪

```z42
public string ToLower();
public string ToUpper();

public string Trim();
public string Trim(char trimChar);
public string Trim(char[] trimChars);
public string TrimStart();
public string TrimStart(char trimChar);
public string TrimStart(char[] trimChars);
public string TrimEnd();
public string TrimEnd(char trimChar);
public string TrimEnd(char[] trimChars);
```

- `ToLower` / `ToUpper` 只按 **ASCII 规则**转换，非 ASCII 字母原样保留
  （`"héllo".ToUpper() == "HéLLO"`）。没有 locale 敏感的重载。
- 无参 `Trim` 家族按空白裁剪；`char[]` 重载传**空数组**时退化为按空白裁剪（对齐 C#
  `Trim(char[])` 的 `null` 语义）。

## 对齐填充

```z42
public string PadLeft(int totalWidth);
public string PadLeft(int totalWidth, char paddingChar);
public string PadRight(int totalWidth);
public string PadRight(int totalWidth, char paddingChar);
```

`PadLeft` 右对齐（左侧补），`PadRight` 左对齐（右侧补）；不带 `paddingChar` 的重载补
空格。串已达到 `totalWidth` 时**原样返回**（不截断）。`totalWidth < 0` →
`PadLeft: totalWidth must not be negative (…)`。

## 切分

```z42
public string[] Split(string separator);
public string[] Split(string separator, int options);
public string[] Split(char[] separators);
```

| 重载 | 说明 |
|---|---|
| `Split(string)` | 按整串分隔符切。空分隔符 → `string.Split: separator must not be empty` |
| `Split(string, int)` | `options` 是 `Std.SplitOptions.*` 的按位或 |
| `Split(char[])` | 按**任一单字符**切；**空数组 → 按空白切分** |

**没有 `Split(char)` 重载**——写 `s.Split(",")` 而不是 `s.Split(',')`。

相邻、首尾的分隔符会产生空段，默认不过滤：`"a,b,,c".Split(",")` 得 4 段，第 3 段是空串。

`Std.SplitOptions` 是一组 `static int` 常量（不是 enum）：

| 常量 | 值 | 效果 |
|---|---|---|
| `SplitOptions.None` | 0 | 不后处理，等价单参 `Split` |
| `SplitOptions.RemoveEmptyEntries` | 1 | 丢掉长度为 0 的段 |
| `SplitOptions.TrimEntries` | 2 | 对每段做 `Trim()` |

```z42
string[] parts = "a, b, ,c".Split(",", SplitOptions.TrimEntries | SplitOptions.RemoveEmptyEntries);
// → ["a", "b", "c"]（先 trim 再滤空）
```

## 拼接与格式化（静态）

```z42
public static string Join(string separator, params string[] values);
public static string Concat(params string[] values);
public static string Format(string format, params object[] args);
public static extern string FromChars(char[] chars);
public static extern string ConcatParts(string[] parts, int count);
```

| 方法 | 说明 |
|---|---|
| `Join(sep, …)` | `params`，两种写法都行：`Join("-", arr)` 与 `Join("-", "a", "b")` |
| `Concat(…)` | 无分隔符逐段拼；零实参 → `""` |
| `Format(fmt, …)` | 把 `{0}` / `{1}` … 替换为对应实参的 `Convert.ToString()` |
| `FromChars(chars)` | `char[]` → 串；空数组 → 空串 |
| `ConcatParts(parts, count)` | 把 `parts[0..count)` 一次拼接；`count` 越界或元素非 string 由 VM 报错 |

`Format` 的两条边界：

- **不支持格式说明符**：`String.Format("{0:F2}", 3)` 原样输出 `{0:F2}`，不替换。
  （插值串 `$"{x:F2}"` 是另一种失效方式——冒号后的内容被静默丢弃，见
  [字符串](../language/strings.md)。）
- **索引越界的 token 原样保留**：`String.Format("{5}x", 1)` 输出 `{5}x`。
- 替换是**非递归**的：实参文本里的 `{0}` 不会被二次替换。

## 比较与协议成员

```z42
public string ToString();
public extern bool Equals(object? other);
public extern bool Equals(string other);
public extern int  CompareTo(string other);
public extern int  GetHashCode();
```

`CompareTo` 返回负 / 0 / 正（按序数比较）。`ToString()` 返回自身。

## 空值判定（静态）

```z42
public static bool IsNullOrEmpty(string? value);
public static bool IsNullOrWhiteSpace(string? value);
```

两者都对 `null` 返回 `true`；`IsNullOrWhiteSpace` 另对全空白串返回 `true`（空串也是）。

## 不支持

- **没有 `Split(char)` 重载**，也没有 `Split` 的段数上限参数。
- **没有 `Contains(char)`**，没有带起始位置 / 比较模式（`StringComparison`）的
  `IndexOf` / `StartsWith` / `Equals` 重载。
- **没有 locale 敏感的大小写转换**——`ToLower` / `ToUpper` 只处理 ASCII。
- **`String.Format` 与插值串都不支持格式说明符**（`{0:F2}` / `{x:X2}`）。需要定宽、
  进制时自己拼接或用 `PadLeft` / `Std.Convert`。
- **没有正则**——正则在 `z42.regex` 包。
- **没有 `string` 的索引器**：`s[i]` 是编译错误 **E0402**（`index on non-array 'String'`），
  逐字符取用 `CharAt(i)`。

## 关联页面

- [字符串](../language/strings.md) —— 字面量语法、转义、插值、`foreach` 的限制
- [数组](../language/arrays.md) —— `Split` / `ToCharArray` 返回的 `T[]`
