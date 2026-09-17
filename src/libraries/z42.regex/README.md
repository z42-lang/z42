# z42.regex

## 职责
正则表达式 parser + 匹配 / 搜索 / 替换 / split。RFC 5234 + POSIX BRE/ERE
子集。对标 C# `System.Text.RegularExpressions.Regex` + Python `re` +
JavaScript `RegExp`。

**引擎**：backtracking NFA（同 Python/Java/JS/C#）。简单、覆盖 90% 用例；
pathological pattern（`(a+)+x` 类）下可能指数时间 — 详 design doc Deferred。

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/Regex.z42`          | `Std.Regex.Regex` main class + backtracking engine |
| `src/RegexParser.z42`    | pattern string → `RegexNode[]` AST，递归下降 |
| `src/RegexNode.z42`      | AST 节点（_kind + fields；同 TomlValue/JsonValue 模式） |
| `src/Match.z42`          | `Std.Regex.Match` — Start/End/Length/Value/Group(i)/GroupCount |
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

## 支持的语法（v0）

| 语法 | 含义 |
|------|------|
| 字面字符 | `a`, `1`, `_` 等 |
| `.` | 任意单字符（v0：包括换行） |
| `^` / `$` | 字符串首 / 末锚点 |
| `\\` `\.` `\*` `\(` `\[` 等 | 转义 metachar |
| `\n` `\t` `\r` | 控制字符 |
| `\d` `\D` | 数字 / 非数字 |
| `\w` `\W` | word char `[A-Za-z0-9_]` / 非 |
| `\s` `\S` | 空白 / 非空白 |
| `[abc]` `[a-z]` `[^abc]` | 字符类（正 / 负 / 区间） |
| `?` `*` `+` | quantifier（greedy） |
| `{n}` `{n,}` `{n,m}` | 计数 quantifier |
| `(...)` | capturing group（按 `(` 顺序 1-based） |
| `\|` | alternation |

## 不支持（详 `docs/reference/src/stdlib/regex.md`「不支持」节）

- backreference `\1`, `\2`
- non-greedy `*?` `+?` `??`
- lookahead / lookbehind `(?=)` `(?!)` `(?<=)` `(?<!)`
- named group `(?<name>...)`
- non-capturing group `(?:...)`
- Unicode property classes `\p{L}`
- flags（i / m / s — 大小写不敏感、多行 ^$、`.` 匹配换行）
- Replace 中的 `$1` 反向引用
- atomic group / possessive quantifier

## 依赖关系
依赖 `z42.core`（基础类型 + Exception）+ `z42.collections`（manifest 依赖，
实际未直接使用 List<T>；通过 z42.core 间接可用）。

## 性能特征

- 编译：O(N) where N = pattern 长度
- 匹配：典型 O(N·M) where N = pattern, M = input 长度
- pathological：`(a+)+x` 对 input `aaaa...aab` 是指数时间（ReDoS 风险）
  → v1 升级到 Thompson NFA simulation 可消除（详 design doc Deferred）

## 实现说明

- AST 用 raw `RegexNode[]` + count 字段（z42 stdlib 不用 `List<T>`，generic
  type param dropping 限制；同 TomlValue / JsonValue 模式）
- Concat 隐式：序列即 `RegexNode[]`；ALT / QUANT / GROUP 的 child 是子序列
- Group capture 用 `_gStarts[i]` / `_gEnds[i]` 数组，回溯时 snapshot + restore
- Quantifier 是 greedy：先匹配最多次，再 backtrack 一格一格短
