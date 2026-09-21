# 字符串

`string` 是不可变的引用类型，内部按 UTF-8 存储；`char` 是一个 Unicode 标量值（32 位）。
z42 有**三种**字符串字面量：普通串、插值串、原始串。

```z42
string a    = "hello";                 // 普通串：处理转义
string msg  = $"Hello, {a}!";          // 插值串：`{}` 里是表达式
string json = """{"k": "v"}""";        // 原始串：逐字保留，不处理转义
```

## 普通串与转义序列

普通串 `"..."`、`char` 字面量 `'...'`、以及插值串的**文本段**都识别同一套转义序列，
且**只有**这一套：

| 转义 | 码点 | 转义 | 码点 |
|------|------|------|------|
| `\a` | 0x07 | `\n` | 0x0A |
| `\b` | 0x08 | `\r` | 0x0D |
| `\f` | 0x0C | `\t` | 0x09 |
| `\v` | 0x0B | `\0` | 0x00 |
| `\\` | `\` | `\"` | `"` |
| `\'` | `'` | | |

**未知转义是编译错误 `E0102`**（unrecognized escape sequence），不会像旧行为那样静默吞掉
反斜杠：

```z42
string bad = "C:\Users\bin";      // ✗ E0102: unrecognized escape sequence '\U'
string ok1 = "C:\\Users\\bin";    // ✓ 显式 \\
string ok2 = """C:\Users\bin""";  // ✓ 原始串逐字保留
```

> **数字 / Unicode 转义 `\uXXXX` 与 `\xXX` 不支持**，写了同样报 `E0102`。

## 原始串 `"""..."""`

```ebnf
raw_string ::= '"""' ( ? 任意字符，除连续三个 '"' ? )* '"""'
```

分隔符固定是 3 个 `"`，**首次**出现 `"""` 即闭合。内容按字节逐字保留：

| 源代码 | 字面值 |
|--------|--------|
| `"""hello"""` | `hello` |
| `""""""`（6 个 `"`） | 空串 |
| `"""he said "hi"."""` | `he said "hi".` |
| `"""multi\nline"""` | `multi\nline`（11 个字符，`\` 和 `n` 都是字面字符） |
| 跨行的 `"""` 串 | 每个换行符（LF / CRLF）原样保留，不规范化 |

```z42
using Std.IO;

void Main() {
    string json = """
{
  "k": "v\n"
}
""";
    Console.WriteLine(json);          // `\n` 打印成反斜杠加 n，不是换行
    Console.WriteLine("""he said "hi".""");
}
```

**四条限制**（写了不会得到 C# 11 的效果）：

| 限制 | 说明 | 变通 |
|------|------|------|
| 不解析任何转义 | `\n` `\t` `\\` `\"` 全部字面保留 | 需要真换行就用普通串 `"\n"` |
| 分隔符长度固定为 3 | 没有 `""""…"""""` 变长形式，内容不能含连续 3 个 `"` | 拆成两段用 `+` 拼 |
| 没有插值前缀 | `$"""..."""` 不是语法 | `"""前缀""" + v + """后缀"""` |
| 不做缩进剥离 / 首尾换行剥离 | 缩进和首尾换行全部进入字面值 | 左对齐到列 0，或事后 `.Trim()` |

未闭合的原始串报 `E0101`（unterminated raw string literal）。

## 插值串 `$"..."`

```z42
string b = "world";
string msg = $"Hello, {b}! Length = {b.Length}";
```

- `{` 和 `}` 之间是一个**完整表达式**，可以是方法调用、算术、成员访问。
- 字面的花括号写 `{{` 和 `}}`。
- 文本段按上面的转义表处理转义。

> ⚠️ **格式说明符不支持，而且是静默失效的。** `$"{x:X2}"` / `$"{x:F2}"` 这类 C# 写法不会
> 报错，冒号后面的内容被**直接丢弃**，输出等同于 `$"{x}"`：
>
> ```z42
> int r = 255;
> Console.WriteLine($"#{r:X2}");   // 实际输出 #255，不是 #FF
> ```
>
> 需要定宽 / 进制格式时，自己拼接或调用相应的转换方法。

## 常用成员

```z42
using Std.IO;

void Main() {
    string a = "a,b,c";
    Console.WriteLine(a.Length);            // 5 —— 属性，不是方法
    Console.WriteLine(a.ToUpper());         // A,B,C
    Console.WriteLine(a.StartsWith("a"));   // true
    string[] parts = a.Split(",");          // 3 段
    Console.WriteLine(parts.Length);
}
```

两个容易写错的地方：

- **`Length` 是属性**（`a.Length`），不是 `a.Length()`。另有 `ByteLength` 返回 UTF-8 字节数，
  两者对非 ASCII 字符串不同（`"你好".Length == 2`，`"你好".ByteLength == 6`）。
- **`Split` 没有 `Split(char)` 重载**。可用的三个是 `Split(string)`、`Split(string, int)`、
  `Split(char[])`，所以要写 `a.Split(",")` 而不是 `a.Split(',')`。

完整的字符串成员清单见标准库参考的 `Std.String`。

## 逐字符遍历

`foreach` 与下标 `s[i]` 都可以，两者都按**字符**（Unicode 标量）而非字节走：

```z42
using Std.IO;

void Main() {
    string s = "z42";

    foreach (char c in s) {
        Console.WriteLine(c);
    }

    for (int i = 0; i < s.Length; i = i + 1) {
        Console.WriteLine(s[i]);            // 等价于 s.CharAt(i)
    }
}
```

`string` 命中 `foreach` 的**索引路径**（`Length` + `this[int]`），**不物化 `char[]`**；
`Length` 与 `CharAt` 都是 O(1) 摊还，所以整个循环是 O(n)。`foreach` 支持哪些形态见
[迭代](iteration.md)。

> 2026-09 之前 `foreach (char c in s)` 能通过编译却在运行期 trap
> （`expected array, got Str`），`s[i]` 则报 `E0402: index on non-array String`；
> 当时的变通是先 `s.ToCharArray()`。两者现已直接可用。

## 与其他形态的关系

| 形态 | 关系 |
|------|------|
| `"..."` 普通串 | 基准形态；原始串不替代它 |
| `$"..."` 插值串 | 与原始串不可组合（无 `$"""`） |
| `"""..."""` 原始串 | 与普通串完全正交 |
| `'...'` char 字面量 | 单个 Unicode 标量值，转义表同普通串 |
| `@"..."` C# verbatim | **z42 不引入**——原始串覆盖同样的用例 |
| `'''...'''` | **z42 不引入**——统一用 `"` |

## 关联页面

- [基本类型与字面量](types.md) — `string` / `char` 在类型表中的位置
- [运算符](operators.md) — `+` 拼接、`+=`、比较
- [迭代](iteration.md) — `foreach` 的三条路径
