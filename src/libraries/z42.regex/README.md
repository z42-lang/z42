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
公开 API 的完整签名见 [reference 包页](../../../docs/reference/src/stdlib/regex.md)（由源码 doc 注释生成）。

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

转义后的 ASCII 字母/数字只接受上表已经实现的形式；字符类内仅接受 `\n` / `\t` / `\r`。
其它保留转义在 Compile 时抛 `RegexException`，而标点、反斜杠与非 ASCII 字符可按字面转义。
直接写字面字母/数字即可匹配它们；实际控制字符也可作为字面量。replacement 的 `$N` 规则独立于 pattern 转义。

## 不支持

详见 `docs/reference/src/stdlib/regex.md`「不支持」节：

- lookahead / lookbehind、原子组、占有量词
- backreference `\1`（Compile 抛 RegexException）
- 字符类内的 `\d` `\w` `\s` `\b` `\B`（Compile 抛 RegexException）
- Unicode property classes `\p{L}`
- `(?s)` / `(?x)` / 作用域 flag；`\x41` / `\u0041` 数值转义
- 不保证线性匹配时间；组内选择支持回溯，例如 `(a|ab)c` 可以匹配 `"abc"`

## 如何测试验证

```bash
xtask test stdlib z42.regex    # 本库全部 [Test]
```

## 关联文档

- 用法与规则（参考手册）：[z42.regex](../../../docs/reference/src/stdlib/regex.md)
- 设计 / 机制（实现内幕）：[标准库三层架构](../../../docs/internals/src/stdlib/architecture.md) · [包划分与 interop 归属](../../../docs/internals/src/stdlib/organization.md)

## 依赖关系
依赖 `z42.core`（基础类型 + Exception）。

## 性能特征

- 编译：O(N) where N = pattern 长度
- 匹配：典型 O(N·M) where N = pattern, M = input 长度
- pathological：`(a+)+x` 对 input `aaaa...aab` 是指数时间（ReDoS 风险）
  → 默认无上限；`Regex.Compile(pattern, maxSteps)` 可设置正数引擎步数预算。
- 每次 Find/IsMatch 或整次 FindAll/Replace/Split 共享一次预算；耗尽抛 `RegexMatchLimitException`，不返回部分结果，下次调用重置预算。
- 预算计数引擎调度、重复原子尝试和捕获恢复；不是编译、输入物化、时间或内存上限。

## 实现说明

- AST 编译为指令图，匹配使用显式选择栈和捕获变更日志
- Concat 隐式：序列即 `RegexNode[]`；ALT / QUANT / GROUP 的 child 是子序列
- Group capture 使用 slots 和变更日志，回溯时恢复到选择点
- Quantifier 默认 greedy：先匹配最多次，再 backtrack 一格一格短；`?` 后缀为 lazy
