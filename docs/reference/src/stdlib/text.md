# z42.text —— 字符串构建与文本工具

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.text/`；命名空间 `Std.Text`

三个类型：可变字符串缓冲 `StringBuilder`、字符串 shaping 静态工具 `Strings`、
编辑距离静态工具 `Levenshtein`。

`string` 本身的方法（`Substring` / `Split` / `Replace` / `IndexOf` / `CharAt` …）在 prelude 的
`Std.String` 上，不需要导入本包，见 [字符串](../language/strings.md)。本包装的是
**不适合塞进 prelude 的按需工具**。正则表达式**不在本包**（另有 `z42.regex`）。

用之前要 `using Std.Text;`，工程清单里声明依赖：

```toml
[dependencies]
"z42.text" = "0.1.0"
```

## `StringBuilder`

可变字符串缓冲。大量 `Append` 后一次 `ToString()`，避免反复拼接字符串的开销。

```z42
public class StringBuilder {
    public StringBuilder();

    public int Length { get; }                 // 只有 getter
    public char this[int index] { get; set; }

    public StringBuilder Append(string value);
    public StringBuilder Append(char value);
    public StringBuilder Append(object value);
    public StringBuilder AppendLine(object value);
    public StringBuilder AppendLine();
    public StringBuilder AppendFormat(string format, params object[] args);

    public StringBuilder Insert(int index, string value);
    public StringBuilder Insert(int index, char value);
    public StringBuilder Insert(int index, object value);
    public StringBuilder Remove(int startIndex, int length);
    public StringBuilder Replace(string oldValue, string newValue);
    public StringBuilder Clear();

    public int GetLength();
    override string ToString();
}
```

| 成员 | 说明 |
|---|---|
| `Append(string)` / `Append(char)` | 直接追加 |
| `Append(object)` | 任意值，经 `Convert.ToString` 转成字符串再追加（`42` → `"42"`，`true` → `"true"`，`3.5` → `"3.5"`） |
| `AppendLine(value)` | 追加 `value` 再追加 `"\n"` |
| `AppendLine()` | 只追加 `"\n"` |
| `AppendFormat(format, args)` | 追加 `String.Format(format, args)` 的结果，占位符是 `{0}` / `{1}` 风格 |
| `Length` | 当前字符数（**只读属性**，没有 setter） |
| `GetLength()` | 与 `Length` 等价的方法形态 |
| `this[int index]` | 按字符位置读写。**越界抛可 `catch` 的 `Exception`**：`StringBuilder index out of range: {i} (Length={n})` |
| `Insert(index, value)` | 在字符位置 `index` 处插入；`index == Length` 等同追加 |
| `Remove(startIndex, length)` | 删除 `[startIndex, startIndex+length)` 区间的字符 |
| `Replace(oldValue, newValue)` | 替换缓冲里**全部**出现的 `oldValue` |
| `Clear()` | 清空（`Length` 归 0） |
| `ToString()` | 物化出当前字符串；空缓冲返回 `""` |

**除 `Length` / `GetLength` / `ToString` 外的所有方法都返回 `this`**，可以链式调用：

```z42
var sb = new StringBuilder();
sb.Append("a").Append('b').Append(42).AppendLine();
Console.WriteLine(sb.ToString());   // "ab42\n"
```

`Append` 有 `string` / `char` / `object` 三个重载；**没有 `int` / `double` / `bool` 的专用重载**，
这些值走 `Append(object)`。`AppendLine` 只有 `object` 与无参两个形态。

`Length` 没有 setter：截断请用 `Remove(n, Length - n)`，清空请用 `Clear()`。

## `Strings`

静态工具类，补 `Std.String` 上没有的几个常见 shaping 操作。

```z42
public static class Strings {
    public static string PadLeft(string s, int width, char fill);
    public static string PadRight(string s, int width, char fill);
    public static string Repeat(string s, int count);
    public static int    IndexOfAny(string s, char[] chars);
    public static string TrimChars(string s, char[] chars);
}
```

| 方法 | 说明 | 例 |
|---|---|---|
| `PadLeft(s, width, fill)` | 左侧补 `fill` 到长度 `width`。**`s` 已够长时原样返回，不截断** | `PadLeft("42", 5, '0')` → `"00042"`；`PadLeft("hello", 3, '*')` → `"hello"` |
| `PadRight(s, width, fill)` | 右侧补，其余同上 | `PadRight("hi", 5, '.')` → `"hi..."` |
| `Repeat(s, count)` | 重复 `count` 次。`count <= 0` 或 `s` 为空 → `""` | `Repeat("ab", 3)` → `"ababab"` |
| `IndexOfAny(s, chars)` | `s` 中第一个出现在 `chars` 里的字符的下标；无匹配 `-1`。**`chars` 为空数组恒返回 `-1`** | `IndexOfAny("hello", ['l','x'])` → `2` |
| `TrimChars(s, chars)` | 从**两端**去掉所有属于 `chars` 的字符。`chars` 为空数组时原样返回 | `TrimChars("***hi***", ['*'])` → `"hi"` |

`TrimChars` 只有对称版本；要单边修剪自己配合 `Substring` 使用。三个 `fill` / `chars`
形参都是 `char` / `char[]`，不接受字符串。

## `Levenshtein`

静态工具类，编辑距离与归一化相似度。

```z42
public static class Levenshtein {
    public static int    Distance(string a, string b);
    public static double SimilarityRatio(string a, string b);
}
```

| 方法 | 说明 |
|---|---|
| `Distance(a, b)` | 把 `a` 变成 `b` 所需的单字符插入 / 删除 / 替换的最少次数。对称；相同字符串返回 `0`；一侧为空时返回另一侧长度 |
| `SimilarityRatio(a, b)` | `1.0 - Distance(a, b) / max(len(a), len(b))`，落在 `[0.0, 1.0]`。**两个空串返回 `1.0`** |

```z42
Levenshtein.Distance("kitten", "sitting");         // 3
Levenshtein.SimilarityRatio("kitten", "sitting");  // 0.5714285714285714
Levenshtein.SimilarityRatio("", "");               // 1.0
```

典型用途：命令拼写纠正（"did you mean: status?"）、配置键的容错查找。
时间复杂度 O(|a|·|b|)——对长文本（KB 级）逐对比较会很慢。

## 用法

```z42
using Std.IO;
using Std.Text;

void Main() {
    var sb = new StringBuilder();
    sb.Append("id=").Append(7).Append(" name=").Append("z42");
    sb.AppendFormat(" score={0}", 99);
    Console.WriteLine(sb.ToString());          // id=7 name=z42 score=99
    Console.WriteLine(sb.Length);

    Console.WriteLine(Strings.PadLeft("7", 3, '0'));            // 007
    Console.WriteLine(Strings.Repeat("-", 20));
    Console.WriteLine(Strings.TrimChars("  [x]  ", [' ', '[', ']']));  // x

    if (Levenshtein.SimilarityRatio("stauts", "status") > 0.6) {
        Console.WriteLine("did you mean: status?");   // 比值 0.666…，命中
    }
}
```

## 不支持

- **`StringBuilder.Length` 没有 setter**（不能用 `sb.Length = 0` 截断 / 补 `'\0'`）。
- **`StringBuilder` 没有容量构造器**（`new StringBuilder(capacity)` / `new StringBuilder(string)`），
  只有无参构造器；也没有 `Capacity` / `EnsureCapacity` / `MaxCapacity`。
- **`StringBuilder` 没有区间重载**：`Append(string, start, count)`、`Replace(old, new, start, count)`、
  `ToString(start, length)`、`CopyTo` 都没有。
- **`StringBuilder.Replace(char, char)`** 没有，只有 `Replace(string, string)`。
- **`StringBuilder` 不支持 `foreach`**（无 `GetEnumerator()`）。要逐字符处理，先取出 `char[]`
  **存进局部变量**再遍历——把 `ToCharArray()` 直接写在 `foreach` 的 `in` 位置上，每个元素会取到 `null`：

  ```z42
  char[] cs = sb.ToString().ToCharArray();
  foreach (var c in cs) { ... }              // ✓
  foreach (var c in sb.ToString().ToCharArray()) { ... }   // ✗ 每个 c 都是 null
  ```
- **`Strings` 没有默认 `fill`**：`PadLeft` / `PadRight` 必须显式给填充字符，没有「默认补空格」的重载。
- **`Strings` 没有 `TrimStart` / `TrimEnd` / `LastIndexOfAny` / `Join` / `Split` 系列**。
- **`Levenshtein` 没有距离上限剪枝参数**（`Distance(a, b, maxDistance)`），也没有
  Damerau-Levenshtein（不把相邻换位算作一次编辑）。
- **正则不在本包**——在 `z42.regex`。

## 相关

- [字符串](../language/strings.md)——`string` 字面量、插值与 `Std.String` 自身的方法
- [参数修饰符（`ref` / `out` / `in` / `params`）](../language/parameter-modifiers.md)——`AppendFormat` 的 `params object[]`
- [工程清单 z42.toml](../toolchain/z42-toml.md)——`[dependencies]` 怎么写
