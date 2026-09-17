# z42.regex —— 正则表达式编译与匹配

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.regex/`；命名空间 `Std.Regex`（`RegexException` 在 `Std`）

把 pattern 字符串编译成 `Regex` 对象，再做匹配 / 查找 / 替换 / 切分。语法是常见正则的
**子集**——够用于日志行解析、字段抽取、简单校验；不够用于需要 lookaround、backreference
或 Unicode 属性类的场景。

⚠️ **两条必须先知道的边界**：

1. **引擎是回溯式的**，pathological pattern（如 `(a+)+x` 配 `aaaa…b`）会指数爆炸。
   不要拿它跑不受信任的 pattern 或不受信任的超长输入。
2. **group 内部的选择不回溯**——`(a|ab)c` 匹配不上 `"abc"`。详见[回溯的边界](#回溯的边界)。

## Regex

编译后的 pattern。线程内复用；同一个 `Regex` 实例不要跨线程并发调用（匹配过程写实例字段）。

```z42
public static Regex Compile(string pattern)       // 失败抛 Std.RegexException

public bool     IsMatch(string input)
public Match    Find(string input)                // 第一个 match；无匹配返回 null
public Match[]  FindAll(string input)             // 所有非重叠 match（可能为空数组）
public string   Replace(string input, string replacement)
public string[] Split(string input)

public int GroupCount()                           // 捕获组个数，不含 group 0
public int GroupIndexOf(string name)              // 命名组 → 1-based 下标；未声明返回 -1
```

| 成员 | 说明 |
|---|---|
| `Compile` | 解析 pattern 并预处理开头的内联 flag。语法错误在这里抛出，不是匹配时 |
| `IsMatch` | 等价 `Find(input) != null` |
| `Find` | 从左到右逐个起始位置尝试；`^` 开头（且未开 `(?m)`）时只试位置 0 |
| `FindAll` | 非重叠扫描；**空匹配也计入**，每遇一个空匹配起点前进一格（`a*` 对 `"bab"` 得 4 个 match） |
| `Replace` | 替换所有非重叠 match；replacement 支持 `$N` 占位符，见下表 |
| `Split` | 按 match 切分。无匹配返回 `[input]`；空输入返回 `[""]`；结果长度 = match 数 + 1 |
| `GroupCount` | `(?:…)` 非捕获组不计入 |

### `Replace` 的 replacement 语法

| 占位符 | 含义 |
|---|---|
| `$0` | 整个 match |
| `$1` … `$9` | 第 N 个捕获组 |
| `$$` | 字面 `$` |
| `$X`（X 非数字非 `$`） | 原样输出 `$X` |
| `$N`（N 超出组数） | 原样输出 `$N` |

只支持单个数字，没有 `${name}` / `${10}` 这类形式。

## Match

一次匹配的结果。由 `Find` / `FindAll` 返回。

```z42
public int    Start()                  // 起始下标（含）
public int    End()                    // 结束下标（不含）
public int    Length()                 // End() - Start()
public string Value()                  // 整个 match 的子串，等价 Group(0)
public string Group(int index)         // 0 = 整个 match；越界抛 RegexException
public string GroupByName(string name) // 名字未在 pattern 中声明则抛 RegexException
public int    GroupCount()             // 不含 group 0
```

- **没参与本次匹配的组返回空字符串** `""`，不是 null。`(a)|(b)` 匹配 `"b"` 时
  `Group(1) == ""`、`Group(2) == "b"`。
- `GroupByName` **区分大小写**：`(?<w>…)` 用 `"W"` 查会抛 `RegexException`。
- 下标是 UTF-16 code unit 下标，与 `string.Length` / `Substring` 一致。

## RegexException

```z42
namespace Std;
public class RegexException : Exception {
    public RegexException(string message)
}
```

由 `Regex.Compile`（非法 pattern）和 `Match.Group` / `Match.GroupByName`
（下标越界 / 名字未声明）抛出。匹配过程本身不抛这个异常。

## 支持的语法子集

下表逐条经实跑核实。

### 字符与字符类

| 语法 | 支持 | 说明 |
|---|---|---|
| 字面字符 | ✅ | |
| `\.` `\\` `\*` `\(` … | ✅ | 转义元字符；`\` 后跟**任何**未列出的字符都当字面量（`\q` = `q`） |
| `\n` `\t` `\r` | ✅ | 类内类外都认 |
| `.` | ✅ | 任意字符，**包括 `\n`**（始终 dotall，没有开关） |
| `[abc]` `[a-z]` | ✅ | |
| `[^abc]` | ✅ | |
| `-` 在末尾（`[a-]`） | ✅ | 字面连字符 |
| `^` 非首位（`[a^]`） | ✅ | 字面 `^` |
| `[]` 空类 | ❌ | 编译抛 `empty character class` |
| `\d` `\D` `\w` `\W` `\s` `\S` | ✅ | **仅在字符类外**。ASCII 定义：`\w` = `[A-Za-z0-9_]`，`\s` = 空格 / `\t` / `\n` / `\r` |
| **类内**的 `\d` `\w` `\s` `\b` | ⚠️ | **不是类**——`[\d]` 等于字面 `d`，`[\w.]` 等于 `[w.]`。见[静默按字面量处理](#静默按字面量处理的写法) |
| `\p{L}` `\p{Greek}` Unicode 属性 | ❌ | 编译报错 |
| `[[:alpha:]]` POSIX 类 | ❌ | 按普通字符类解析，不是 POSIX 语义 |

### 量词

| 语法 | 支持 |
|---|---|
| `?` `*` `+` | ✅ |
| `{n}` `{n,m}` `{n,}` | ✅ |
| `{,m}` | ❌ 编译抛 `expected digit` |
| 惰性 `??` `*?` `+?` `{n,m}?` | ✅ |
| 占有 `?+` `*+` `++` | ❌ 编译抛 `unexpected '+'` |

量词只能跟在一个 atom 后面；`*a` / `^*a` 这类开头量词——前者编译报错，后者把 `*` 作用在
`^` 上（零宽 anchor 的量词无实际效果）。

### 分组与交替

| 语法 | 支持 | 说明 |
|---|---|---|
| `(…)` 捕获组 | ✅ | 1-based，按 `(` 出现顺序编号 |
| `(?:…)` 非捕获组 | ✅ | 不占编号，不计入 `GroupCount()` |
| `(?<name>…)` 命名组 | ✅ | 同时分配数字编号，`Group(N)` 与 `GroupByName(name)` 等价 |
| `\|` 交替 | ✅ | 右结合；多路 `a\|b\|c` 正常 |
| `(?=…)` `(?!…)` lookahead | ❌ | 编译抛 `unsupported '(?…)' construct` |
| `(?<=…)` `(?<!…)` lookbehind | ❌ | 编译抛 `named group name must start with letter or '_'`（报错信息指向命名组，属已知的误导性诊断） |
| `(?>…)` 原子组 | ❌ | 编译抛 `unsupported '(?…)' construct` |
| `(?#…)` 注释 | ❌ | 同上 |
| `\1` `\2` backreference | ⚠️ | **静默**当成字面数字，见下 |

命名组的名字规则：首字符是 ASCII 字母或 `_`，后续可含数字；同一 pattern 内不得重名
（重名、空名、数字开头、缺 `>` 一律编译报错）。

### 锚与零宽断言

| 语法 | 支持 | 说明 |
|---|---|---|
| `^` | ✅ | 默认只匹配输入开头；`(?m)` 下也匹配 `\n` 之后 |
| `$` | ✅ | 默认只匹配输入**末尾**——**不**匹配末尾 `\n` 之前（与 Python / .NET 不同）；`(?m)` 下也匹配任意 `\n` 之前 |
| `\b` `\B` | ✅ | ASCII 词边界，词字符集 `[A-Za-z0-9_]` |
| `\A` `\z` `\Z` `\G` | ⚠️ | **静默**当成字面 `A` / `z` / `Z` / `G` |

`^` / `$` 可以出现在 pattern 中间（`a^b`），此时当作普通零宽断言参与匹配——通常匹配不上。

### 内联 flag

只识别**紧贴 pattern 开头**的一段 `(?…)`，且只认 `i` 和 `m` 两个 flag 字符：

| 写法 | 效果 |
|---|---|
| `(?i)` | 大小写不敏感。ASCII 折叠（`A-Z` ↔ `a-z`），非 ASCII 字符不折叠 |
| `(?m)` | multiline：`^` 也匹配 `\n` 之后，`$` 也匹配 `\n` 之前 |
| `(?im)` / `(?mi)` / `(?ii)` | 组合；顺序无关，重复容忍 |

- `(?i)` 下捕获到的子串保留**输入原本的大小写**。
- 否定字符类在 `(?i)` 下两个大小写都被排除：`(?i)[^A]` 既不匹配 `A` 也不匹配 `a`。
- `(?s)` dotall、`(?x)` 扩展模式、`(?i:…)` 作用域 flag、pattern 中途改 flag（`a(?i)b`）
  一律编译报错。
- `(?)` 不含 flag 字符时也报错。

## 静默按字面量处理的写法

这几类写法**不报错**，但语义与主流正则引擎不同——它们退化成字面字符，导致 pattern
安静地匹配错东西。移植 Python / JavaScript / .NET 的 pattern 时最容易踩：

| 写法 | 期望 | z42 实际 |
|---|---|---|
| `[\d]` `[\w]` `[\s]` | 字符类 | 字面 `d` / `w` / `s` |
| `[\b]` | 退格 0x08 | 字面 `b` |
| `\1` `\2` | backreference | 字面 `1` / `2` |
| `\x41` | 字符 `A` | 字面 `x41` |
| `A` | 字符 `A` | 字面 `u0041` |
| `\A` `\z` `\Z` | 输入始 / 末锚 | 字面 `A` / `z` / `Z` |

典型翻车：`[\w.]+@\w+` 匹配不上 `"foo.bar@baz"`，因为 `[\w.]` 只包含 `w` 和 `.`。
写成 `[A-Za-z0-9_.]+@\w+` 才对。

## 回溯的边界

量词与交替在**同一层**内正常回溯：

```z42
Regex.Compile("\\d+5").Find("1235").Value();   // "1235" —— \d+ 让出末位 5
Regex.Compile("a+ab").Find("aaab").Value();    // "aaab"
Regex.Compile("ab|abc").Find("abc").Value();   // "ab" —— 取最左可行分支
```

但**一旦一个 group 整体匹配成功，引擎就不会再回到组内换另一种匹配法**。捕获组和非捕获组
都一样：

```z42
Regex.Compile("(a|ab)c").Find("abc");      // null（组锁定 "a"，后面 c 对不上 b）
Regex.Compile("(?:a|ab)c").Find("abc");    // null
Regex.Compile("(a*)ab").Find("aaab");      // null（组吞掉 "aaa"，不退让）
Regex.Compile("(Mr|Mrs)\\.X").Find("Mrs.X"); // null
```

绕法：把需要回溯的选择提到组外（`a c|ab c` 形式），或改用两段匹配 + 手工判定。

## 用法

```z42
using Std.IO;
using Std.Regex;

void Main() {
    // 查找 + 捕获
    var re = Regex.Compile("(?<key>\\w+)=(?<val>\\d+)");
    foreach (var m in re.FindAll("a=1, bb=22, c=3")) {
        Console.WriteLine($"{m.GroupByName("key")} -> {m.GroupByName("val")} @{m.Start()}");
    }

    // 判定
    if (Regex.Compile("(?i)^https?://").IsMatch("HTTPS://example.com")) {
        Console.WriteLine("is url");
    }

    // 替换（$N 回填捕获）
    Console.WriteLine(re.Replace("a=1, bb=22", "$2:$1"));   // "1:a, 22:bb"

    // 切分
    foreach (var part in Regex.Compile(",\\s*").Split("a, b,c")) {
        Console.WriteLine(part);
    }

    // 多行日志逐行取头
    var head = Regex.Compile("(?m)^\\w+");
    foreach (var m in head.FindAll("INFO one\nWARN two")) {
        Console.WriteLine(m.Value());   // INFO / WARN
    }
}
```

## 不支持

- **lookaround**：`(?=)` `(?!)` `(?<=)` `(?<!)`——编译报错
- **backreference**：pattern 里的 `\1`（静默退化成字面量）；replacement 里的 `$1` 是支持的
- **Unicode 属性类**：`\p{L}` / `\p{Greek}`；`\w` `\s` `\b` 全是 ASCII 定义
- **字符类内的转义类**：`[\d]` `[\w]` `[\s]`（静默退化成字面量）
- **`(?s)` / `(?x)` / 作用域 flag / pattern 中途改 flag**
- **占有量词**与**原子组**
- **`{,m}` 量词写法**
- **`\x` / `\u` 数值转义**（静默退化成字面量）
- **线性时间保证**：引擎是回溯式的，没有 ReDoS 防护，也没有步数 / 超时上限
- **并发复用同一个 `Regex`**：匹配状态存在实例字段上，多线程共享会互相踩

`Std.Regex` 下的 `RegexNode` / `RegexParser` 虽然是 `public`，但属引擎内部结构，
不构成稳定 API，不要直接使用。
