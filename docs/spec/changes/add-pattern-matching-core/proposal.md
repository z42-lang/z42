# Proposal: Rust 风格模式匹配核心（A1 —— 结构化模式引擎）

## Why

`[Record]` 值语义已完成（archive `2026-08-26-add-record-value-semantics`，#300）。record 是**积类型
数据载体**，而消费积类型的天然方式是**模式匹配**（Rust struct/enum ↔ `match` 是同一硬币两面）。当前 z42 的
`switch` **只支持常量模式**（`case 1:` → `Eq(subject, 1)`），`x is T v` 是唯一的富模式入口。要让 record
成为头号公民、把 record 的剩余 follow-up（解构 / `with` / `init`）纳入一套统一的语义，第一步是引入一个
**结构化模式引擎**：让 `switch` 与 `is` 支持通配、类型、**record 位置解构**、属性、嵌套、绑定与守卫。

核心洞察：record 的主构造器已定位置字段（声明序，`Z42ClassType.OwnFieldNames`），因此 `Point(a, b)`
位置模式可以**内建**绑定 `a←X, b←Y`——**无需用户写 `Deconstruct` 方法、无需 `out` 参数**（比 C# 简，
更贴近 Rust）。

本 change 是模式匹配程序的**第一个 change（A1）**：只做结构化模式核心。or-模式 / `@` / 范围 / 关系模式（A2）、
解构声明（B）、穷尽性诊断（C）、`with`/`init`（D/E）各自独立 change 后续推进。

## What Changes

引入一套统一的**模式文法**，接入 `switch`（语句版 + 表达式版）与 `is` 表达式。A1 覆盖以下模式形态：

| 模式 | 语法 | 语义 |
|------|------|------|
| 通配 | `_` | 匹配任意，不绑定 |
| 常量 | `1` / `"s"` / `'c'` / `true` / `null` / `Color.Red` | 值相等（`Eq`）。**byte-identical 保持现状** |
| 类型 | `Point` / `Point p` | `IsInstance` 测试（+ 可选绑定，已存在于 `is`） |
| **位置（record）** | `Point(x, y)` / `Point(0, y)` | `IsInstance` + 按**主构造器声明序**读字段绑定/递归匹配 |
| 属性 | `Point { X: 0, Y: y }` / `{ X: 0 }` | `IsInstance`（类型可省）+ 按字段名读取匹配 |
| 裸绑定 | `x` | 解析命中类型名→类型模式；否则→新绑定 |
| 嵌套 | `Line(Point(x, _), _)` | 上述任意组合递归 |
| 守卫 | `case P if cond:` / `P if cond => e` | 匹配成功后再评估布尔守卫 |

- **`switch` 语句**：`case <pattern> [if <guard>]:` —— 富模式 + 守卫。
- **`switch` 表达式**：`subject switch { <pattern> [if <guard>] => <expr>, ... }`。
- **`is` 表达式**：`x is <pattern>` —— 由「类型 + 绑定」扩到完整模式（`x is Point(a, b)`，绑定在 true 分支可见）。
- **绑定作用域**：模式绑定的变量在对应 arm body / 守卫 / `is` 为真分支进入 `TypeEnv`；各 arm 独立作用域。
- **无新关键字**（扩 `switch`；`is` 已有；守卫复用 `if`）、**无 zbc/zpkg 格式 bump**、**无新 runtime**
  （纯编译期 lowering 到既有 IR：`IsInstance` / `Eq` / `FieldGet` / `BrCond` / 关系比较）。

**Out（后续 change）**：or-模式 `|` / `@` 绑定 / `..=` 范围 / 关系 `> 0`（A2）；解构声明 `Point(x,y) = p`（B）；
穷尽性诊断（C）；`with`（D）；`init`-only（E）；元组（F）；**泛型 record 的位置/属性模式**（A1 只支持非泛型
record 位置解构，泛型解构 defer）；**struct record 的位置解构**（实施期确认 defer——struct 为字节 blob，位置
解构需按 StructLayout 偏移读取，A1 位置模式限 record class 引用类型；非 record class 用位置模式报 E0402）。

## Scope（允许改动的文件）

| 文件路径 | 变更 | 说明 |
|---------|------|------|
| `src/compiler/z42c.syntax/src/Pattern.z42` | NEW | `Pattern` AST 层级：Wildcard / Constant / Type / Positional / Property / Binding（+ 嵌套子模式数组） |
| `src/compiler/z42c.syntax/src/PatternParser.z42` | NEW | `_parsePattern`：字面量/`_`/名字路径决策（`(`→位置、`{`→属性、后跟 ident→类型绑定、点分名→常量、单名→裸绑定） |
| `src/compiler/z42c.semantics/src/BoundPattern.z42` | NEW | `BoundPattern` 层级：绑定后的模式树（携 resolved 类型、字段索引、绑定名/寄存器占位） |
| `src/compiler/z42c.semantics/src/PatternBinder.z42` | NEW | 模式 binder：类型解析、裸名歧义消解（类型名 vs 绑定）、位置模式要求 `IsRecord` + arity 校验、字段递归、绑定注册进 `TypeEnv` |
| `src/compiler/z42c.semantics/src/PatternEmitter.z42` | NEW | 模式 lowering：递归下降 emit「test（`IsInstance`/`Eq`/字段读比较）+ bind（字段→寄存器/局部）」，短路 `BrCond`；**ConstantPattern 路径 byte-identical 现状** |
| `src/compiler/z42c.syntax/src/Stmt.z42` | MODIFY | `SwitchCase`：`pattern` `Expr`→`Pattern`；加 `Expr guard`（可空） |
| `src/compiler/z42c.syntax/src/Ast.z42` | MODIFY | `SwitchArm`：同上；`IsExpr`：由 `(type,bind)` 扩为持 `Pattern` |
| `src/compiler/z42c.syntax/src/StmtParser.z42` | MODIFY | `_parseSwitch`：`case` 后走 `_parsePattern` + 可选 `if` 守卫（`:141`附近） |
| `src/compiler/z42c.syntax/src/ExprParser.z42` | MODIFY | switch-expr arm 走 `_parsePattern` + 守卫；`is` 走 `_parsePattern` |
| `src/compiler/z42c.semantics/src/BoundStmt.z42` | MODIFY | `BoundSwitchCase`：`pattern` `BoundExpr`→`BoundPattern`；加 `BoundExpr Guard` |
| `src/compiler/z42c.semantics/src/BoundExprOp.z42` | MODIFY | `BoundSwitchArm`：同上；`BoundIsExpr`：改持 `BoundPattern` |
| `src/compiler/z42c.semantics/src/StmtBinder.z42` | MODIFY | `_bindSwitchStmt`（`:66`）：经 `PatternBinder` bind 模式 + 守卫，绑定入 arm scope |
| `src/compiler/z42c.semantics/src/ExprTyper.z42` | MODIFY | `_bindSwitchExpr`（`:190`）：同上 |
| `src/compiler/z42c.semantics/src/TypeOpTyper.z42` | MODIFY | `_bindIsExpr`（`:70`）：扩为 bind 完整模式，绑定入 true 分支 scope |
| `src/compiler/z42c.semantics/src/StmtEmitter.z42` | MODIFY | `_emitSwitch`（`:211`）：改用 `PatternEmitter` + 守卫分支 |
| `src/compiler/z42c.semantics/src/OperatorEmitter.z42` | MODIFY | `_emitSwitchExpr`（`:141`）：同上 |
| `src/compiler/z42c.semantics/src/TypeOpEmitter.z42` | MODIFY | `_emitIs`：扩为完整模式 lowering（复用 `PatternEmitter`） |
| `src/tests/pattern-matching/pattern_core.z42` | NEW | e2e 自检（`Assert`，空 stdout 范式）：通配/常量/类型/位置/属性/嵌套/守卫/绑定作用域，switch-stmt + switch-expr + `is` 三位点 |
| `docs/book/src/language/pattern-matching.md` | NEW | 机制页：文法、裸名歧义规则、record 位置解构原理、lowering 数据流（含 mermaid/伪代码） |
| `src/compiler/z42c.syntax/README.md` | MODIFY | 功能索引 + `Pattern.z42`/`PatternParser.z42` |
| `src/compiler/z42c.semantics/README.md` | MODIFY | 功能索引 + `BoundPattern.z42`/`PatternBinder.z42`/`PatternEmitter.z42` |

**只读引用**（理解必读，不改）：`OperatorEmitter._emitSwitchExpr`（常量链范式）、`StmtEmitter._emitSwitch`、
`TypeOpEmitter._emitIs`（`IsInstance` 范式）、`FunctionEmitter.EmitSynthEqualsResult`（record 程序的
`IsInstance`+**直读字段** lowering 范式，jit 安全）、`Z42ClassType.OwnFieldNames/OwnFieldVis/IsRecord`。

## Out of Scope

- A2/B/C/D/E/F 的一切（见 What Changes 的 Out）。
- 泛型 record 的位置/属性解构（A1 只非泛型 record）。
- 任何 zbc/zpkg 格式 bump、任何 runtime/VM 改动。
- god-class 拆分等既有债（本 change 只加新文件 + 最小接线）。

## Open Questions（→ 阶段 6.5 与 User 敲定）

1. **A1 精确 scope**：属性模式（`{F:p}`）是否 A1 就要，还是收窄到「只位置模式」、属性模式挪 A1.5？
2. **`is` 是否 A1 扩**：`x is Point(a,b)` 一并做，还是本 change 只扩 `switch`、`is` 留类型+绑定现状？
3. **守卫关键字**：`if`（Rust 式，推荐）已定；确认不用 `when`。
4. **裸名歧义规则**：单名解析命中类型名→类型模式、否则→新绑定（Rust 式解析期消解）——确认接受。
5. **穷尽性**：A1 不做穷尽性检查（C 迭代），`switch` 无 `default` 且未覆盖时**运行期落空 = 现状行为**（不报错、不抛）——确认接受。
