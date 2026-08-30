# z42.regex — Regular expression parser + matcher

> 落地版本：2026-05-16（add-z42-regex）
> 包路径：`src/libraries/z42.regex/`
> 命名空间：`Std.Regex`（`RegexException` 在 `Std`）

## 职责

正则 pattern 编译 + 匹配 / 搜索 / 替换 / split。子集语法：字面字符 + `.` +
quantifier (`?` `*` `+` `{n,m}`) + 字符类 (`[...]` `\d \w \s`) + group `()`
+ alternation `|` + anchor `^$` + 零宽词边界 `\b` `\B` + inline flag `(?i)` / `(?m)` / `(?im)`。

**对标**：C# `System.Text.RegularExpressions.Regex` + Python `re` +
JavaScript `RegExp` + Java `java.util.regex`（语义子集，行为基本同）。

**引擎**：backtracking NFA。与 Python / Java / JavaScript / .NET 默认引擎
一致；不是 RE2 / Rust 的 linear-time Thompson sim。

## API surface

```z42
class Regex {
    static Regex Compile(string pattern)             // 抛 RegexException
    bool IsMatch(string input)
    Match Find(string input)                          // 第一个 match 或 null
    Match[] FindAll(string input)                     // 所有非重叠 match
    string Replace(string input, string replacement)  // literal repl (no $1)
    string[] Split(string input)
    int GroupCount()                                  // 不含 group 0
}

class Match {
    int    Start()
    int    End()
    int    Length()
    string Value()
    string Group(int index)                           // 0 = entire match
    int    GroupCount()                               // 不含 group 0
}

class RegexException : Exception     // in Std namespace
```

## 设计决策

| Decision | 选项 | 决定 | 理由 |
|----------|------|------|------|
| 1. 引擎 | backtracking / Thompson NFA / DFA | backtracking | 简单实现，~250 LOC；同 Python/Java/JS/C# 默认；ReDoS 风险 documented |
| 2. AST 表达 | discriminated union 类 / sealed inheritance | union 类 | z42 无 ADT；同 TomlValue/JsonValue 模式 |
| 3. Compile vs runtime check | compile-time / lazy | compile-time | fail-fast |
| 4. 字符类存储 | 区间逐对 / bitset 256 | 区间逐对 | N 通常 < 10；O(N) 查找够快 |
| 5. Greedy 实现 | max-then-backoff / lazy | max-then-backoff | greedy 默认；non-greedy 留 Deferred |
| 6. 大小写敏感 | sensitive only / flag | flag via `(?i)` | ASCII fold；add-regex-inline-flags (2026-05-26) |
| 7. Multiline `^$` | 始终首末 / m flag | flag via `(?m)` | `^` 后 `\n` / `$` 前 `\n`；add-regex-word-boundary-multiline (2026-05-30) |
| 8. 异常类 | RegexException 在 `Std` namespace | yes | 同 UriException / TomlException 模式 |
| 9. Namespace | `Std.Text.Regex` / `Std.Regex` | `Std.Regex` | 避免三段 namespace bug（z42.io.binary 经验）；text 包占位类可后续 deprecate |
| 10. Group 0 语义 | 0 = entire / 1 = first | 0 = entire | 同 C#/Python/Java |

## 实现结构

```
src/RegexException.z42  (~10 行)
└── class RegexException : Exception     in Std namespace

src/RegexNode.z42  (~110 行)
└── class RegexNode  — _kind + 各字段 + Charset matching helper

src/RegexParser.z42  (~210 行)
└── class RegexParser  — 递归下降，pattern → RegexNode[]

src/Match.z42  (~45 行)
└── class Match  — Start/End/Length/Value/Group(i)/GroupCount

src/Regex.z42  (~250 行)
├── class Regex  — Compile / IsMatch / Find / FindAll / Replace / Split
└── backtracking engine — _match / _matchQuant + group save/restore
```

## 引擎细节

### 顶层匹配（Find）

`_findFrom(input, pos)` 循环试每个起始位置：

```
for from in 0..input.Length:
    reset group state
    end = _match(seq, 0, from)
    if end >= 0: return Match(from, end, groups)
return null
```

`^` anchored pattern 优化：仅试 `from = 0`，失败立即 return。

### 递归 _match

```
_match(seq, idx, pos) → int    // 新 pos 或 -1
    if idx >= seq.Length: return pos        // 序列消费完
    node = seq[idx]
    switch node._kind:
        LIT, ANY, CHARSET: 匹配单字符，pos+1，递归
        ANCHOR_START: pos == 0
        ANCHOR_END:   pos == input.Length
        ALT: 试 left subseq，成功后递归剩余；失败 restore + 试 right
        QUANT: _matchQuant
        GROUP: 试 child subseq；成功后记录 _gStarts/_gEnds[i]，递归剩余
```

### Quantifier 回溯

Greedy：

```
_matchQuant(q, seq, idx, pos):
    snapshot = save group state
    p = pos
    positions = [pos]
    matched = 0
    while max < 0 or matched < max:
        np = _match(q._childSeq, 0, p)
        if np < 0: break
        if np == p and matched >= min: break    // 零宽防死循环
        matched++
        p = np
        positions.add(p)
    if matched < min: restore; return -1
    for i in positions.length-1 down to min:
        restore snapshot
        replay min..i 次 child match
        rp = _match(seq, idx+1, position[i])
        if rp >= 0: return rp
    return -1
```

注：每次 backtrack 都从 entry 重新 replay min..i 次 child match — 开销 O(i)
per attempt，总 O(positions.length²)。pathological pattern 下指数；正常用例
可接受。优化候选：状态机式增量 backtrack（v1）。

### Group save/restore

ALT / GROUP / QUANT 在 backtrack 时调用 `_snapshotStarts` / `_snapshotEnds`
+ `_restoreStarts` / `_restoreEnds`。每次 snapshot 是 `_groupCount` 大小的
int 数组拷贝 — O(groupCount) per call。groupCount typically < 10。

## 不支持（Deferred）

### regex-future-thompson-nfa

- **来源**：pathological pattern `(a+)+x` over `aaa...aab` 指数时间 ReDoS
- **触发原因**：backtracking 引擎天然问题；切到 Thompson NFA simulation
  （RE2/Rust style）保证 linear time
- **触发条件**：用户场景出现 ReDoS / 接受不信任 pattern（如 web app
  user-defined regex filter）时
- **当前 workaround**：调用方限制 pattern 长度 + input 长度；不要接受不信任输入

### regex-future-backreference

- **来源**：`(\\w+)\\s+\\1`（重复 word）
- **触发原因**：backreference 与 Thompson NFA 不兼容（强制 backtracking）；
  v0 引擎可加但 API 表面增加
- **当前 workaround**：调用方自行做两次匹配 + Group(1) 比较

### ~~regex-future-non-greedy~~ — **✅ 已落地 2026-05-26 (add-regex-non-greedy)**

Shipped: `*?` / `+?` / `??` / `{n,m}?` non-greedy (lazy) quantifiers.
Matcher tries the shortest count first instead of the longest.

Implementation:
- New `bool _greedy` field on `RegexNode` (default true)
- New `RegexNode.QuantLazy(child, count, min, max)` factory mirrors
  `Quant` but sets `_greedy = false`
- `RegexParser.MaybeWrapQuant`: detects trailing `?` after `*` / `+`
  / `?` / `{n,m}` — switches to `QuantLazy`
- `Regex.MatchQuant`: iteration direction parameterised by `_greedy`
  (greedy: posCount-1 → min; lazy: min → posCount)

13 new tests cover: `<.*?>` shortest HTML tag (vs greedy baseline),
`a+?` minimum-one (vs greedy), `\d+?5` lazy-with-suffix, `colou??r`
lazy optional (prefers skip; falls back to match when skip fails),
`a{2,4}?` lazy counted (picks min in range), lazy-then-greedy in
same pattern (`(<.*?>)(\d+)`), HTML tag-name extraction
(`<(\w+?)>`), lazy quantifier on a group.

### regex-future-lookaround

- **来源**：`(?=)` `(?!)` `(?<=)` `(?<!)` 零宽断言
- **触发原因**：高级功能；引擎需要 lookahead 支持
- **当前 workaround**：通常可用 group + post-filter 替代

### ~~regex-future-named-group~~ — **✅ 已落地 2026-05-26 (add-regex-named-groups)**

Shipped: `(?<name>…)` syntax — same matching as `(…)` but the group is
also addressable by name. Numeric index still works alongside.

API:
- `Match.GroupByName(string name) → string` — look up by name; throws
  `RegexException` when the name isn't declared
- `Regex.GroupIndexOf(string name) → int` — name → 1-based index;
  returns `-1` when undeclared (programmatic capability lookup)
- `(?<name>…)` allocates a normal capture index (counts toward
  `GroupCount()`); numeric `m.Group(N)` returns the same string

Name rules (covers all common use cases):
- ASCII letters (`A-Z` / `a-z`) and `_` for the first char
- ASCII letters / digits / `_` for subsequent chars
- Unique per pattern — duplicate names throw at compile time
- Empty name (`(?<>...)`) throws
- Missing closing `>` throws

Implementation:
- `RegexParser`: parallel `string[] GroupNames` + `int[] GroupIndices`
  arrays grown 2× on overflow; `_RegisterNamedGroup` checks uniqueness
- `Regex`: stores the parallel arrays; new public `GroupIndexOf(name)`
  is a linear scan (typical pattern has < 5 named groups so a hash
  would add overhead beyond benefit)
- `Match`: new private `_owner: Regex` back-reference; constructor
  takes an extra arg; `GroupByName` delegates lookup then calls
  existing `Group(int)`

14 new tests cover: basic lookup, dual numeric+name access, canonical
YYYY-MM-DD date parser, mixed named + unnamed, three named groups,
named + quantifier, named + optional, name with `_`/digits, undefined-
name error, duplicate-name compile error, empty-name compile error,
digit-first compile error, missing-`>` compile error, `Regex.GroupIndexOf`.

### ~~regex-future-non-capturing-group~~ — **✅ 已落地 2026-05-26 (add-regex-non-capturing-group)**

Shipped: `(?:…)` syntax — same match semantics as `(…)` but no capture
index allocated; `GroupCount()` and `Group(n)` indices unaffected by
interspersed non-capturing groups.

Implementation:
- `RegexParser.ParseAtom`: when `(` is followed by `?:`, skip the
  capture-index allocation; pass `-1` as the marker `groupIndex`
- `RegexParser`: `(?` followed by any non-`:` char (e.g. `(?=` lookahead,
  `(?<` named group, `(?i)` flags) throws `RegexException` —
  reserves the syntax for clear future error messages on still-deferred
  features
- `Regex.Match` matcher: GROUP node with `_groupIndex == -1` skips the
  `_gStarts` / `_gEnds` slot write, otherwise behaves identically

11 new tests cover: alternation in non-capturing group matches like a
capturing one; `GroupCount()` ignores `(?:)` and only counts `(`;
mixed capturing + non-capturing keeps indices stable; quantifier on
non-capturing (`(?:\d{2}){3}`, `(?:s)?`); nested non-capturing inside
capturing (`((?:abc)+)`); capturing inside non-capturing
(`(?:(\d+)\.(\d+))`); KV-pair separator skip pattern
(`(\w+)(?:=)(\w+)`); error on unsupported `(?=...)` construct.

Two engine limitations surfaced during test authoring (unrelated to
non-capturing — pre-existing, filed independently if not already
tracked):
- Alternation backtracking through a following atom: `(Mr|Mrs|Ms)\.X`
  on `"Mrs.X"` fails because the engine commits to the first matching
  alternative and doesn't backtrack into a later alternative when the
  suffix fails. Tests pick the leftmost-matching alternative.
- Charset `[\w.]+` adjacent to `\w+` in some URL-shape patterns fails
  to match — needs separate investigation; tests avoid this combination.

### regex-future-unicode-classes

- **来源**：`\p{L}` 任意 letter；`\p{Greek}` 等
- **触发原因**：依赖 Unicode database（数 KB 数据 + lookup 算法）
- **前置依赖**：z42.core 落地 Unicode property API
- **当前 workaround**：手写 charset

### ~~regex-future-flags~~ — **✅ partial 已落地 2026-05-26 (add-regex-inline-flags)**

Shipped: leading `(?i)` flag — case-insensitive matching for the
whole pattern. ASCII case folding (A-Z ↔ a-z); non-ASCII unchanged
(matches most regex implementations' default; full Unicode case
folding is the deferred `regex-future-unicode-classes` work).

Implementation:
- `Regex.Compile` strips a leading `(?i)` and records the flag before
  handing the body to `RegexParser` (the parser itself is unchanged)
- `Match` for LIT compares folded chars when flag set
- `Match` for CHARSET tries both the input char and its case-swapped
  version; AND-combines for negated `[^…]` (so `[^A]` excludes both
  `A` and `a`), OR-combines for positive `[…]`
- Captured group strings preserve the ORIGINAL input case (case-folding
  is only for the comparison, not for the output)

14 new tests cover: literal matching with lower / upper / mixed input;
strict case stays default without flag; `[A-Z]` charset accepts both
cases under flag; `[a-z]` likewise; negated `[^A]` excludes both
cases; quantifier + alternation interaction; capture returns original-
case substring; URL-scheme common pattern (`(?i)(https?)`); digits
unaffected by flag; `\w+` escape class unaffected (regression).

Still deferred — `regex-future-flags-extras`:
- `(?s)` dotall — `.` also matches `\n` (currently matches anything;
  the dotall vs not distinction is implementation-dependent here)
- `(?x)` extended — whitespace + comments in pattern stripped
- Mid-pattern flag changes `(?-i)…` or scoped `(?i:…)`

### ~~regex-future-word-boundary-multiline~~ — **✅ 已落地 2026-05-30 (add-regex-word-boundary-multiline)**

Shipped:
- `\b` (WORD_BOUNDARY, RegexNode kind 9) — zero-width assertion that
  succeeds when one side of `pos` is an ASCII word char (`[A-Za-z0-9_]`)
  and the other is not (or absent at input endpoints)
- `\B` (WORD_NON_BOUNDARY, kind 10) — inverse of `\b`
- `(?m)` multiline flag — `^` matches at pos 0 or after `\n`; `$`
  matches at input end or before `\n`
- Combined `(?im)` / `(?mi)` — inline-flag stripper generalised from
  exact `(?i)` to `(?[im]+)`; flag char order + repetition tolerated
  (matches .NET `RegexOptions` semantics)
- `Regex.FindFrom` anchor-start fast-fail disabled under `_multiline`
  to allow `^` after `\n` to be tried at non-zero positions

31 new tests (`tests/regex_word_boundary.z42` + `tests/regex_multiline.z42`)
cover: isolated word match / substring rejection; input endpoints; `\B`
inverse; underscore + digit word membership; `(?i)\b` combination;
default `^`/`$` lock-to-endpoints regression; `(?m)^` after `\n`;
`(?m)$` before `\n`; full-line `(?m)^\w+$` FindAll; `(?im)` order
independence; `(?ii)` repeated tolerance; charset `[\b]` retains
literal `b` (PCRE-vs-z42 documented divergence); real-world log-line
header extraction.

Still deferred:
- Charset-internal `\b` as ASCII BS 0x08 (PCRE behaviour) — v0 keeps
  literal `b`
- Unicode word membership for `\b` / `\B` (waits on
  `regex-future-unicode-classes`)

### ~~regex-future-replace-backreference~~ — **✅ 已落地 2026-05-26 (add-regex-replace-backreference)**

Shipped: `Regex.Replace(input, replacement)` now interprets `$N` /
`$$` placeholders in the replacement string. Closes the deferred —
key/value swaps, date reformatting, capture-wrapping are now one-liners.

| 占位符 | 含义 |
|---|---|
| `$0` | whole match (equivalent to `m.Group(0)`) |
| `$1` … `$9` | capture group N (1-based) |
| `$$` | literal `$` |
| `$X` (X non-digit, non-`$`) | emitted literally as `$X` (matches .NET policy — avoids surprising callers using `$` in plain text) |
| `$N` for out-of-range N | emitted literally |

Implementation: new private `_ExpandReplacement(replacement, m)` on
`Regex`. Fast path skips reconstruction when the replacement string
has no `$`. Replacement parsing is a single linear scan with no
allocations beyond the result string. Backward-compatible: replacement
strings without `$` produce byte-identical output to the prior v0
literal path.

13 new tests cover: `$0` whole-match wrap; `$2=$1` key/value swap
(`name=alice` → `alice=name`); date reformat (`2026-05-26` →
`05/26/2026`); `$$` literal dollar; combined `$$$0`; no-dollar
replacement legacy regression; out-of-range `$5` literal; `$a` (non-
digit) literal; trailing `$` literal; multi-match each uses own
groups (`a=42, b=7` with `$2:$1` → `42:a, 7:b`); wrap capture
brackets; `$1` with no capture groups in pattern → emitted literally.

### regex-future-text-regex-merge

- **来源**：z42.text 早有占位 `Regex` class，本 spec 走独立 z42.regex
- **触发原因**：text 包占位是 placeholder；本 spec 在独立包给完整实现，更
  清晰边界
- **触发条件**：后续 spec 决定 deprecate 旧占位（z42.text.Regex 删除 +
  redirect notice）

## 跨 stdlib 交互

- 依赖 `z42.core`（基础类型 + Exception）
- 与 `z42.text` 互补：z42.text 的 StringBuilder + Substring 与本包结合可
  构造非 regex 字符串管道
- 被未来 `z42.net` / web 处理可能调用（URL 验证、HTTP header parse）

## 实施期发现

1. **`out` 是 z42 保留字**（用作 `out` 参数 modifier？），不能作变量名。
   Replace 内变量 `string out` 改为 `string acc`，parser 内 `out` 改为 `wrap`。
2. **z42 stdlib 不用 `List<T>`**（generic 类型参数 dropping bug）。AST 储存
   走 raw `RegexNode[]` + 显式 count 字段；同 TomlValue / JsonValue 模式。
3. **`_peek()` 越界需显式 `_eof()` 检查**。`{3` 末尾 parser 调用 `_peek()`
   触发 IndexOutOfRange 而非 RegexException。所有 `_peek` 比较前加
   `!this._eof() &&` 守卫。
