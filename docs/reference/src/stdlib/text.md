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

`src/libraries/z42.text/src/Levenshtein.z42:5`


```z42
public static class Levenshtein {
    static int Distance(string a, string b)
    static double SimilarityRatio(string a, string b)
}
```

### static int Distance(string a, string b)

`src/libraries/z42.text/src/Levenshtein.z42:7`

`a` 变成 `b` 所需的单字符插入 / 删除 / 替换的最少次数。对称；相同字符串返回 `0`，一侧为空时返回另一侧长度。

### static double SimilarityRatio(string a, string b)

`src/libraries/z42.text/src/Levenshtein.z42:56`

相似度，落在 `[0.0, 1.0]`：`1.0 - Distance(a, b) / max(len(a), len(b))`。两个空串返回 `1.0`。

## StringBuilder

`src/libraries/z42.text/src/StringBuilder.z42:25`


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

### StringBuilder()

`src/libraries/z42.text/src/StringBuilder.z42:32`

无参构造，初始容量 4。

### StringBuilder Append(string value)

`src/libraries/z42.text/src/StringBuilder.z42:42`

追加一个字符串。

### StringBuilder Append(char value)

`src/libraries/z42.text/src/StringBuilder.z42:54`

追加一个字符。

### StringBuilder Append(object value)

`src/libraries/z42.text/src/StringBuilder.z42:61`

追加任意值，经 `Convert.ToString` 转成字符串再追加。

### StringBuilder AppendLine(object value)

`src/libraries/z42.text/src/StringBuilder.z42:66`

追加 `value` 再追加一个换行。

### StringBuilder AppendLine()

`src/libraries/z42.text/src/StringBuilder.z42:72`

只追加一个换行。

### int GetLength()

`src/libraries/z42.text/src/StringBuilder.z42:77`

当前字符数。

### int Length { get; }

`src/libraries/z42.text/src/StringBuilder.z42:87`

当前字符数（只读属性）。

### char this[int index] { get; set; }

`src/libraries/z42.text/src/StringBuilder.z42:93`

按字符位置读写。越界抛 `Exception`。

### StringBuilder Clear()

`src/libraries/z42.text/src/StringBuilder.z42:127`

清空（`Length` 归 0），保留已分配的容量。

### StringBuilder Insert(int index, string value)

`src/libraries/z42.text/src/StringBuilder.z42:134`

在字符位置 `index` 插入；`index == Length` 等同追加。

### StringBuilder Insert(int index, char value)

`src/libraries/z42.text/src/StringBuilder.z42:142`

在字符位置 `index` 插入一个字符。

### StringBuilder Insert(int index, object value)

`src/libraries/z42.text/src/StringBuilder.z42:149`

在字符位置 `index` 插入任意值（经 `Convert.ToString`）。

### StringBuilder Remove(int startIndex, int length)

`src/libraries/z42.text/src/StringBuilder.z42:154`

删除 `[startIndex, startIndex + length)` 区间的字符。

### StringBuilder Replace(string oldValue, string newValue)

`src/libraries/z42.text/src/StringBuilder.z42:162`

替换缓冲里全部出现的 `oldValue`。

### StringBuilder AppendFormat(string format, params object[] args)

`src/libraries/z42.text/src/StringBuilder.z42:170`

追加 `String.Format(format, args)` 的结果，占位符是 `{0}` / `{1}` 风格。

| 参数 | 说明 |
|---|---|
| `format` | 含 `{0}` 风格占位符的格式串 |
| `args` | 填入占位符的值 |

## Strings

`src/libraries/z42.text/src/Strings.z42:6`


```z42
public static class Strings {
    static string PadLeft(string s, int width, char fill)
    static string PadRight(string s, int width, char fill)
    static string Repeat(string s, int count)
    static int IndexOfAny(string s, char[] chars)
    static string TrimChars(string s, char[] chars)
}
```

### static string PadLeft(string s, int width, char fill)

`src/libraries/z42.text/src/Strings.z42:11`

左侧补 `fill` 到长度 `width`。`s` 已够长时原样返回，**不截断**。

例：`PadLeft("42", 5, '0')` → `"00042"`；`PadLeft("hello", 3, '*')` → `"hello"`。

### static string PadRight(string s, int width, char fill)

`src/libraries/z42.text/src/Strings.z42:28`

右侧补 `fill` 到长度 `width`。`s` 已够长时原样返回，不截断。

例：`PadRight("hi", 5, '.')` → `"hi..."`。

### static string Repeat(string s, int count)

`src/libraries/z42.text/src/Strings.z42:44`

重复 `s` `count` 次。`count <= 0` 或空 `s` → `""`。

例：`Repeat("ab", 3)` → `"ababab"`。

### static int IndexOfAny(string s, char[] chars)

`src/libraries/z42.text/src/Strings.z42:66`

`s` 中第一个出现在 `chars` 里的字符的下标；无匹配返回 `-1`。`chars` 为空数组恒返回 `-1`。

例：`IndexOfAny("hello", new char[]{'l', 'x'})` → `2`。

### static string TrimChars(string s, char[] chars)

`src/libraries/z42.text/src/Strings.z42:83`

从**两端**去掉所有属于 `chars` 的字符。`chars` 为空数组时原样返回。

例：`TrimChars("***hi***", new char[]{'*'})` → `"hi"`。

