# Proposal: 模式匹配 A2 —— or-模式 / `@` 绑定 / `..=` 范围 / 关系模式

## Why

模式匹配核心 A1（archive-in-flight `add-pattern-matching-core`，#306）已建成三层递归下降引擎
（`PatternParser` / `PatternBinder` / `PatternEmitter` + `Pattern` / `BoundPattern` 节点族），接入
`switch`（语句 + 表达式）与 `is`，覆盖通配 / 常量 / 类型 / **record 位置解构** / 属性 / 裸绑定 / 嵌套 / 守卫。

A2 在**同一引擎**上补齐 Rust 模式匹配的四个常用组合子，让 `switch` 从「一 case 一形态」升级到「富组合」：

| 形态 | 语法 | 现状缺口 |
|------|------|---------|
| **or-模式** | `case 1 \| 2 \| 3:` / `case Circle \| Square:` | 现在多值/多类型要写多个 case，无法合并 |
| **`@` 绑定** | `case p @ Point(0, y):` | 无法「既整体绑定又解构」——要么绑整体、要么解构，不能兼得 |
| **`..=` 闭区间范围** | `case 1 ..= 5:` / `case 'a' ..= 'z':` | 区间匹配只能写守卫 `case n if n >= 1 && n <= 5` |
| **关系模式** | `case > 0:` / `case <= 100:` | 同上，只能靠守卫 |

这四个都是**纯编译期 lowering** 到既有 IR（新增 `Ge`/`Le`/`Gt`/`Lt` 比较 + 已有 `IsInstance`/`Eq`/`BrCond`），
**无 zbc/zpkg 格式 bump、无新 runtime**。它们把「区间 / 多值 / 整体+解构」这类高频意图从守卫样板压成一等模式。

## What Changes

在 A1 引擎上加**四个新模式节点**（syntax `Pattern` + semantics `BoundPattern` 各 4 个）、扩 `PatternParser`
（or-链 + 三个新起始）、`PatternBinder`（4 节点绑定 + 可比较性校验）、`PatternEmitter`（4 节点 lowering）。

| 模式 | 语法 | 语义 / lowering |
|------|------|-----------------|
| or-模式 | `P1 \| P2 \| ...` | 任一子模式匹配即匹配。lowering = 依次尝试，失败落下一 alt，全失败落 fail |
| `@` 绑定 | `name @ P` | 绑 `name` 到整个 subject **且** subject 须匹配 `P`。lowering = bind name=subj + EmitMatch(P) |
| `..=` 范围 | `lo ..= hi` | 闭区间 `subj >= lo && subj <= hi`（含端点）。lo/hi 为常量。lowering = `Ge` + `Le` 短路 |
| 关系 | `> v` / `>= v` / `< v` / `<= v` | `subj <op> v`。v 为常量。lowering = 对应比较指令 + `BrCond` |

**新 token（唯一的词法改动）**：`@`（`At`）、`..=`（`DotDotEq`）。`|` / `>` / `>=` / `<` / `<=` token 已存在。

### 应用位点（关键 scope 决策，详见 design Decision 1）

| 位点 | A2 新增 |
|------|---------|
| `switch` 语句 case | ✅ 全部四种（or / `@` / `..=` / 关系） |
| `switch` 表达式 arm | ✅ 全部四种 |
| `is` 表达式 | ✅ 仅 `..=` / 关系（起始无歧义）；**or / `@` 不入 is**（`\|` 与位或歧义、`@` 与类型引导冲突） |

### 约束（A2 边界，详见 design）

- **or-模式的子模式在 A2 不得引入绑定**（`case Point(x,_) | Circle(x):` 报错）——各 alt 绑定集一致性 + 合流
  寄存器 phi 属独立复杂度，defer 到后续迭代。A2 的 or 覆盖「多常量 / 多类型 / 多区间」纯测试组合（90% 用例）。
- **`..=` / 关系模式仅用于可比较基元**（整数族 / 浮点族 / `char`）——subject 静态类型非可比较基元 → 诊断。
- **`@` 的子模式可含绑定**（`p @ Point(0, y)` 合法，绑 `p` 与 `y`）——`@` 本身不涉合流问题。

## Scope（允许改动的文件）

| 文件路径 | 变更 | 说明 |
|---------|------|------|
| `src/compiler/z42c.syntax/src/TokenKind.z42` | MODIFY | +`At = 152` / `DotDotEq = 153`（末尾追加，值仅需互异、不入 zbc） |
| `src/compiler/z42c.syntax/src/Lexer.z42` | MODIFY | `@` → `At`（单字符）；`..=` → `DotDotEq`（三字符，须在 `..` 前判） |
| `src/compiler/z42c.syntax/src/Pattern.z42` | MODIFY | +`OrPattern` / `AtPattern` / `RangePattern` / `RelationalPattern` |
| `src/compiler/z42c.syntax/src/PatternParser.z42` | MODIFY | `_parsePattern` 拆 or-链 + `_parsePrimaryPattern`；`@` / 关系起始 / `..=` 尾随；常量在 bp>44 解析 |
| `src/compiler/z42c.semantics/src/BoundPattern.z42` | MODIFY | +`BoundOrPattern` / `BoundAtPattern` / `BoundRangePattern` / `BoundRelationalPattern` |
| `src/compiler/z42c.semantics/src/PatternBinder.z42` | MODIFY | 4 节点绑定 + or 无绑定校验 + 可比较基元校验 |
| `src/compiler/z42c.semantics/src/PatternEmitter.z42` | MODIFY | 4 节点 lowering（`Ge`/`Le`/`Gt`/`Lt` + 短路 `BrCond`） |
| `src/tests/pattern-matching/pattern_a2.z42` | NEW | e2e 自检：四形态 × switch-stmt/switch-expr/is 各验；jit 双验 |
| `docs/book/src/language/pattern-matching.md` | MODIFY | 补 A2 四形态语法 + lowering 机制页 |
| `examples/patterns.z42` | MODIFY | 补 A2 示例 |

**Out（后续 change）**：or-模式**带绑定**的合流（各 alt 绑定集一致性 + phi）；解构声明 `Point(x,y) = p`（B）；
穷尽性诊断（C）；`with`（D）；`init`（E）；元组（F）；`is` 中的 or / `@`。

## 自举 / 格式影响

- **无 zbc/zpkg 格式 bump、无新 runtime**：新 token 仅词法内部（不入 zbc）；新模式 lowering 到既有 IR
  （`Ge`/`Le`/`Gt`/`Lt`/`IsInstance`/`Eq`/`FieldGet`/`BrCond` 全部已存在）。
- **两-nightly 纪律满足（单 PR）**：新语法（`@` / `..=` / or / 关系模式）**只在 e2e 测试文件出现**，
  z42c / stdlib / xtask 源**一律不用** → 上一 nightly 的 z42c 仍能编当前源码。改 parser/lexer/codegen 后跑
  `xtask test bootstrap` 验无越界。
- **自举字节不动点**：A1 已确立 z42c 源无 `switch`、`is` 仅 `x is T`/`x is T v`（走未改的 `IsExpr` 老路）；
  A2 不碰这两条路径 → gen1==gen2 天然成立。**唯一风险点**：A2 把常量模式解析 bp 从 `_parseExpr(0)` 抬到
  `_parseExpr(45)`（避免吞 `|`）——但常量模式仅在 `switch`/结构化 `is` 出现，z42c 源无此路径 → 不影响不动点
  （详见 design Decision 2 的风险分析 + 回归验证）。
