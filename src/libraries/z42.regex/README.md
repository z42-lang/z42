# z42.regex

## 职责
正则表达式 parser + 匹配 / 搜索 / 替换 / split。常见正则语法的子集，接口参照 Python `re` / JavaScript `RegExp`。

**引擎**：backtracking NFA（同 Python/Java/JS）。简单、覆盖 90% 用例；
pathological pattern（`(a+)+x` 类）下可能指数时间 — 详 `docs/reference/src/stdlib/regex.md`。

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/Regex.z42`          | `Std.Regex.Regex` main class + backtracking engine |
| `src/RegexParser.z42`    | pattern string → `RegexNode[]` AST，递归下降 |
| `src/RegexNode.z42`      | AST 节点（_kind + fields；同 TomlValue/JsonValue 模式） |
| `src/Match.z42`          | `Std.Regex.Match` — Start/End/Length/Value/Group(i)/GroupByName/GroupCount |
| `src/RegexException.z42` | `Std.RegexException`（compile-time errors） |

## 入口点

```z42
using Std.Regex;

// Compile + 匹配测试
Regex r = Regex.Compile("\\d{3}-\\d{4}");
bool ok = r.IsMatch("call 555-1234 today");           // true

// 找第一个 match
Match m = r.Find("call 555-1234 today");
if (m != null) {
    m.Start();   // 5
    m.End();     // 13
    m.Value();   // "555-1234"
}

// 找所有 match
Match[] all = r.FindAll("a1 b22 c333");
// all.Length == 3, ["1", "22", "333"]

// Replace
string out = Regex.Compile("\\d+").Replace("a1 b22 c333", "N");
// "aN bN cN"

// Split
string[] parts = Regex.Compile("\\s+").Split("hello   world  z42");
// ["hello", "world", "z42"]

// Capturing groups
Regex kv = Regex.Compile("(\\w+)=(\\w+)");
Match m2 = kv.Find("name=alice");
m2.Group(0);   // "name=alice" (entire match)
m2.Group(1);   // "name"
m2.Group(2);   // "alice"
```

## 支持的语法

| 语法 | 含义 |
|------|------|
| 字面字符 / `\.` `\*` 等 | 字面 / 转义 metachar；`\n` `\t` `\r` 控制字符 |
| `.` | 任意单字符（含换行，无 dotall 开关） |
| `^` / `$` | 输入首 / 末锚点；`(?m)` 下也匹配 `\n` 前后 |
| `\b` `\B` | ASCII 词边界 |
| `\d` `\D` `\w` `\W` `\s` `\S` | 预定义类（ASCII，仅字符类外） |
| `[abc]` `[a-z]` `[^abc]` | 字符类（正 / 负 / 区间） |
| `?` `*` `+` `{n}` `{n,}` `{n,m}` | 贪婪量词；加 `?` 为惰性（`*?` 等） |
| `(...)` / `(?:...)` / `(?<name>...)` | 捕获 / 非捕获 / 命名组（`GroupByName` / `GroupIndexOf`） |
| `\|` | alternation |
| `(?i)` `(?m)` | 仅 pattern 开头的内联 flag |
| Replace 中 `$0`–`$9` / `$$` | 捕获组引用 |

## 不支持

详见 `docs/reference/src/stdlib/regex.md`「不支持」节与「静默按字面量处理的写法」：

- lookahead / lookbehind、原子组、占有量词
- backreference `\1`（静默退化为字面量）
- 字符类内的 `\d` `\w` `\s`（退化为字面字母）
- Unicode property classes `\p{L}`
- `(?s)` / `(?x)` / 作用域 flag；`\x41` / `\u0041` 数值转义
- group 内部的选择不回溯：`(a|ab)c` 匹配不上 `"abc"`

## 如何测试验证

```bash
xtask test stdlib z42.regex    # 本库全部 [Test]
```

## 依赖关系
依赖 `z42.core`（基础类型 + Exception）。

## 性能特征

- 编译：O(N) where N = pattern 长度
- 匹配：典型 O(N·M) where N = pattern, M = input 长度
- pathological：`(a+)+x` 对 input `aaaa...aab` 是指数时间（ReDoS 风险）
  → 无步数 / 超时上限，勿对不受信任的 pattern 或超长输入使用

## 实现说明

- AST 用 raw `RegexNode[]` + count 字段（z42 stdlib 不用 `List<T>`，generic
  type param dropping 限制；同 TomlValue / JsonValue 模式）
- Concat 隐式：序列即 `RegexNode[]`；ALT / QUANT / GROUP 的 child 是子序列
- Group capture 用 `_gStarts[i]` / `_gEnds[i]` 数组，回溯时 snapshot + restore
- Quantifier 默认 greedy：先匹配最多次，再 backtrack 一格一格短；`?` 后缀为 lazy
