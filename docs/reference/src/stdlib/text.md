<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.text/ -->

# z42.text —— 字符串构建与文本工具。

> 包路径 `src/libraries/z42.text/`；命名空间 `Std.Text`

三个类型：可变字符串缓冲 [`StringBuilder`]、字符串 shaping 静态工具 [`Strings`]、编辑距离静态工具
[`Levenshtein`]。`string` 本身的方法（`Substring` / `Split` / `Replace` / `IndexOf` / `CharAt` …）
在 prelude 的 `Std.String` 上，不需要导入本包；正则表达式在 [`Std.Regex.Regex`]，不在本包。

## 不支持

- `StringBuilder.Length` 没有 setter（不能用 `sb.Length = 0` 截断 / 补 `'\0'`）；截断用
  `Remove(n, Length - n)`，清空用 `Clear()`。
- `StringBuilder` 没有容量构造器与 `Capacity` / `EnsureCapacity` / `MaxCapacity`，只有无参构造器。
- `StringBuilder` 没有区间重载：`Append(string, start, count)`、`Replace(old, new, start, count)`、
  `ToString(start, length)`、`CopyTo` 都没有；也没有 `Replace(char, char)`。
- `StringBuilder` 不支持 `foreach`（无 `GetEnumerator()`）。要逐字符处理，先把 `ToCharArray()`
  存进局部变量再遍历（直接写在 `foreach` 的 `in` 位置会取到 `null`）。
- `Strings` 没有默认 `fill`：`PadLeft` / `PadRight` 必须显式给填充字符。
- `Strings` 没有 `TrimStart` / `TrimEnd` / `LastIndexOfAny` / `Join` / `Split` 系列。
- `Levenshtein` 没有距离上限剪枝参数，也没有 Damerau–Levenshtein（相邻换位不算一次编辑）。

## Levenshtein

[`src/libraries/z42.text/src/Levenshtein.z42:6`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/Levenshtein.z42#L6)

Levenshtein 编辑距离 + 相似度。O(|a|·|b|) 时间、O(min(|a|,|b|)) 空间（两行 DP）。

用途：命令拼写纠正（"did you mean: status?"）、拼写检查、配置键的容错查找。

```z42
public static class Levenshtein {
    static int Distance(string a, string b)
    static double SimilarityRatio(string a, string b)
}
```

成员：[`Distance`](#Std.Text.Levenshtein.Distance(string,string)) · [`SimilarityRatio`](#Std.Text.Levenshtein.SimilarityRatio(string,string))

<a id="Std.Text.Levenshtein.Distance(string,string)"></a>
### static int Distance(string a, string b)

[`src/libraries/z42.text/src/Levenshtein.z42:8`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/Levenshtein.z42#L8)

`a` 变成 `b` 所需的单字符插入 / 删除 / 替换的最少次数。对称；相同字符串返回 `0`，一侧为空时返回另一侧长度。

<a id="Std.Text.Levenshtein.SimilarityRatio(string,string)"></a>
### static double SimilarityRatio(string a, string b)

[`src/libraries/z42.text/src/Levenshtein.z42:57`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/Levenshtein.z42#L57)

相似度，落在 `[0.0, 1.0]`：`1.0 - Distance(a, b) / max(len(a), len(b))`。两个空串返回 `1.0`。

## StringBuilder

[`src/libraries/z42.text/src/StringBuilder.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/StringBuilder.z42#L25)

可变字符串构建器：Append 收集进 `string[]` buffer（2× 扩容），ToString 一次性合并。

基于 z42.core 原语（`string[]` + 手动扩容 + `String.FromChars`），避免每次 Append 都
拼接的 O(n²) 开销。不用 `List<string>` 是因为 stdlib 字段类型暂不支持泛型实例化语法。

```z42
public class StringBuilder {
    StringBuilder()
    StringBuilder Append(string value)
    StringBuilder Append(char value)
    StringBuilder Append(object value)
    StringBuilder AppendLine(object value)
    StringBuilder AppendLine()
    int GetLength()
    int Length { get; }
    char this[int index] { get; set; }
    StringBuilder Clear()
    StringBuilder Insert(int index, string value)
    StringBuilder Insert(int index, char value)
    StringBuilder Insert(int index, object value)
    StringBuilder Remove(int startIndex, int length)
    StringBuilder Replace(string oldValue, string newValue)
    StringBuilder AppendFormat(string format, params object[] args)
}
```

成员：[`StringBuilder`](#Std.Text.StringBuilder.ctor()) · [`Append`](#Std.Text.StringBuilder.Append(string)) · [`Append`](#Std.Text.StringBuilder.Append(char)) · [`Append`](#Std.Text.StringBuilder.Append(object)) · [`AppendLine`](#Std.Text.StringBuilder.AppendLine(object)) · [`AppendLine`](#Std.Text.StringBuilder.AppendLine()) · [`GetLength`](#Std.Text.StringBuilder.GetLength()) · [`Length`](#Std.Text.StringBuilder.Length) · [`this`](#Std.Text.StringBuilder.this[int]) · [`Clear`](#Std.Text.StringBuilder.Clear()) · [`Insert`](#Std.Text.StringBuilder.Insert(int,string)) · [`Insert`](#Std.Text.StringBuilder.Insert(int,char)) · [`Insert`](#Std.Text.StringBuilder.Insert(int,object)) · [`Remove`](#Std.Text.StringBuilder.Remove(int,int)) · [`Replace`](#Std.Text.StringBuilder.Replace(string,string)) · [`AppendFormat`](#Std.Text.StringBuilder.AppendFormat(string,object[]))

<a id="Std.Text.StringBuilder.ctor()"></a>
### StringBuilder()

[`src/libraries/z42.text/src/StringBuilder.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/StringBuilder.z42#L32)

无参构造，初始容量 4。

<a id="Std.Text.StringBuilder.Append(string)"></a>
### StringBuilder Append(string value)

[`src/libraries/z42.text/src/StringBuilder.z42:42`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/StringBuilder.z42#L42)

追加一个字符串。

<a id="Std.Text.StringBuilder.Append(char)"></a>
### StringBuilder Append(char value)

[`src/libraries/z42.text/src/StringBuilder.z42:54`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/StringBuilder.z42#L54)

追加一个字符。

<a id="Std.Text.StringBuilder.Append(object)"></a>
### StringBuilder Append(object value)

[`src/libraries/z42.text/src/StringBuilder.z42:61`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/StringBuilder.z42#L61)

追加任意值，经 `Convert.ToString` 转成字符串再追加。

<a id="Std.Text.StringBuilder.AppendLine(object)"></a>
### StringBuilder AppendLine(object value)

[`src/libraries/z42.text/src/StringBuilder.z42:66`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/StringBuilder.z42#L66)

追加 `value` 再追加一个换行。

<a id="Std.Text.StringBuilder.AppendLine()"></a>
### StringBuilder AppendLine()

[`src/libraries/z42.text/src/StringBuilder.z42:72`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/StringBuilder.z42#L72)

只追加一个换行。

<a id="Std.Text.StringBuilder.GetLength()"></a>
### int GetLength()

[`src/libraries/z42.text/src/StringBuilder.z42:77`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/StringBuilder.z42#L77)

当前字符数。

<a id="Std.Text.StringBuilder.Length"></a>
### int Length { get; }

[`src/libraries/z42.text/src/StringBuilder.z42:87`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/StringBuilder.z42#L87)

当前字符数（只读属性）。

<a id="Std.Text.StringBuilder.this[int]"></a>
### char this[int index] { get; set; }

[`src/libraries/z42.text/src/StringBuilder.z42:93`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/StringBuilder.z42#L93)

按字符位置读写。越界抛 `Exception`。

<a id="Std.Text.StringBuilder.Clear()"></a>
### StringBuilder Clear()

[`src/libraries/z42.text/src/StringBuilder.z42:127`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/StringBuilder.z42#L127)

清空（`Length` 归 0），保留已分配的容量。

<a id="Std.Text.StringBuilder.Insert(int,string)"></a>
### StringBuilder Insert(int index, string value)

[`src/libraries/z42.text/src/StringBuilder.z42:134`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/StringBuilder.z42#L134)

在字符位置 `index` 插入；`index == Length` 等同追加。

<a id="Std.Text.StringBuilder.Insert(int,char)"></a>
### StringBuilder Insert(int index, char value)

[`src/libraries/z42.text/src/StringBuilder.z42:142`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/StringBuilder.z42#L142)

在字符位置 `index` 插入一个字符。

<a id="Std.Text.StringBuilder.Insert(int,object)"></a>
### StringBuilder Insert(int index, object value)

[`src/libraries/z42.text/src/StringBuilder.z42:149`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/StringBuilder.z42#L149)

在字符位置 `index` 插入任意值（经 `Convert.ToString`）。

<a id="Std.Text.StringBuilder.Remove(int,int)"></a>
### StringBuilder Remove(int startIndex, int length)

[`src/libraries/z42.text/src/StringBuilder.z42:154`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/StringBuilder.z42#L154)

删除 `[startIndex, startIndex + length)` 区间的字符。

<a id="Std.Text.StringBuilder.Replace(string,string)"></a>
### StringBuilder Replace(string oldValue, string newValue)

[`src/libraries/z42.text/src/StringBuilder.z42:162`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/StringBuilder.z42#L162)

替换缓冲里全部出现的 `oldValue`。

<a id="Std.Text.StringBuilder.AppendFormat(string,object[])"></a>
### StringBuilder AppendFormat(string format, params object[] args)

[`src/libraries/z42.text/src/StringBuilder.z42:170`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/StringBuilder.z42#L170)

追加 `String.Format(format, args)` 的结果，占位符是 `{0}` / `{1}` 风格。

| 参数 | 说明 |
|---|---|
| `format` | 含 `{0}` 风格占位符的格式串 |
| `args` | 填入占位符的值 |

## Strings

[`src/libraries/z42.text/src/Strings.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/Strings.z42#L7)

`Std.String` 上没有的常见字符串 shaping 静态工具。

放在 z42.text 而不是 prelude 的 `Std.String` 上，是因为 prelude 的每个方法都会进所有
脚本的命名空间；这些工具有用但不够通用，按需 `using Std.Text` 才看到更合适。

```z42
public static class Strings {
    static string PadLeft(string s, int width, char fill)
    static string PadRight(string s, int width, char fill)
    static string Repeat(string s, int count)
    static int IndexOfAny(string s, char[] chars)
    static string TrimChars(string s, char[] chars)
}
```

成员：[`PadLeft`](#Std.Text.Strings.PadLeft(string,int,char)) · [`PadRight`](#Std.Text.Strings.PadRight(string,int,char)) · [`Repeat`](#Std.Text.Strings.Repeat(string,int)) · [`IndexOfAny`](#Std.Text.Strings.IndexOfAny(string,char[])) · [`TrimChars`](#Std.Text.Strings.TrimChars(string,char[]))

<a id="Std.Text.Strings.PadLeft(string,int,char)"></a>
### static string PadLeft(string s, int width, char fill)

[`src/libraries/z42.text/src/Strings.z42:12`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/Strings.z42#L12)

左侧补 `fill` 到长度 `width`。`s` 已够长时原样返回，**不截断**。

例：`PadLeft("42", 5, '0')` → `"00042"`；`PadLeft("hello", 3, '*')` → `"hello"`。

<a id="Std.Text.Strings.PadRight(string,int,char)"></a>
### static string PadRight(string s, int width, char fill)

[`src/libraries/z42.text/src/Strings.z42:29`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/Strings.z42#L29)

右侧补 `fill` 到长度 `width`。`s` 已够长时原样返回，不截断。

例：`PadRight("hi", 5, '.')` → `"hi..."`。

<a id="Std.Text.Strings.Repeat(string,int)"></a>
### static string Repeat(string s, int count)

[`src/libraries/z42.text/src/Strings.z42:45`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/Strings.z42#L45)

重复 `s` `count` 次。`count <= 0` 或空 `s` → `""`。

例：`Repeat("ab", 3)` → `"ababab"`。

<a id="Std.Text.Strings.IndexOfAny(string,char[])"></a>
### static int IndexOfAny(string s, char[] chars)

[`src/libraries/z42.text/src/Strings.z42:67`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/Strings.z42#L67)

`s` 中第一个出现在 `chars` 里的字符的下标；无匹配返回 `-1`。`chars` 为空数组恒返回 `-1`。

例：`IndexOfAny("hello", new char[]{'l', 'x'})` → `2`。

<a id="Std.Text.Strings.TrimChars(string,char[])"></a>
### static string TrimChars(string s, char[] chars)

[`src/libraries/z42.text/src/Strings.z42:84`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.text/src/Strings.z42#L84)

从**两端**去掉所有属于 `chars` 的字符。`chars` 为空数组时原样返回。

例：`TrimChars("***hi***", new char[]{'*'})` → `"hi"`。

