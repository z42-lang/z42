# Proposal: 模式匹配 B —— 解构声明 `Point(x, y) = p;`

## Why

模式匹配核心 A1（#306）+ A2（#308）+ A3（#309）建成三层递归下降引擎（PatternParser →
PatternBinder → PatternEmitter），已服务 `switch`（语句 + 表达式）与 `is` 三个应用位点。设计文档
（`docs/book/src/language/pattern-matching.md`）的 Deferred 段列出「解构声明 `Point(x,y) = p`」为后续独立特性。

解构声明是模式匹配的**第四个应用位点**：把一个 record 直接按位置解构到**新声明的局部变量**，无需
`switch`/`is` 外壳。这是 Rust `let Point{x,y} = p;` / C# `var (x,y) = p;` 的对应物——积类型数据消费的
最简形态。

```z42
[Record] class Point(int X, int Y);
Point p = Point(3, 4);
Point(x, y) = p;          // x ← 3, y ← 4；x/y 在后续语句可见
use(x + y);
```

## What Changes

在既有模式引擎上**加一个不可失败（irrefutable）语句形态**：`<Pattern> = <expr>;`，其中 Pattern 为
record 位置模式。复用 `PatternBinder._bindPositional`（record/arity 校验 + 字段类型解析）与
`PatternEmitter.EmitMatch`（`field_get` 直读 + 递归绑定）——**引擎 100% 复用，只新增 parse 判别 + 一个
AST/Bound 语句节点 + bind/emit 接线**。

| 维度 | 现状 | B（本 change） |
|------|------|----------------|
| 位置模式绑定 | 仅在 switch/is 内 | ✅ 独立声明语句 |
| 绑定作用域 | switch 臂 / is true 分支 | 当前块（后续语句可见，同普通局部声明） |
| 可失败性 | switch/is 有失败分支（落下一 case / 写 false） | **不可失败**：静态限制子模式仅 irrefutable（见下） |

### 不可失败性（irrefutable）——核心设计点

`EmitMatch` 强制要求一个 `failL` 失败标签；引擎无「irrefutable 上下文」概念。解构声明语义上**没有失败
分支**（`Point(x,y) = p` 恒成功——p 已静态是 Point）。两条路线：

- **(A) 静态限制 irrefutable 子模式**（推荐）：解构声明的子模式**只允许** 通配 `_` / 裸绑定 `x` /
  嵌套位置模式 `Point(a, Inner(b))`——**禁止**常量 `0` / or `|` / 范围 `..=` / 关系 `>0` / 类型测试
  `T x`（类型测试对声明的静态类型恒成立可豁免，但初版从简禁止）。含可失败子模式 → 编译错误
  `E0xxx: destructuring declaration pattern must be irrefutable`。`failL` 块理论上不可达，emit 一个
  `unreachable`/panic 兜底。**对齐 Rust `let`**（`let` 只收 irrefutable，可失败要 `let-else`）。
- (B) 允许可失败 + 运行时 panic：`failL` 块发 throw/panic。灵活但引入运行时失败点，且 z42 无现成
  pattern-match-panic 设施。

**推荐 (A)**：更安全、实现更简（纯静态校验，无新运行时）、语义清晰。

### Scope（初版边界）

- **仅 record class 位置模式**（`IsRecord && !IsStruct`）——struct record 位置解构在 A1 即 defer
  （`PatternBinder.z42:158`），B 同样 defer。
- **仅位置形态** `T(p, ...)`；属性形态 `T { F: p }` 的解构声明 defer 到 B2（引擎已支持属性模式，接线
  同理，但初版从简）。
- 子模式限 irrefutable（见上）。嵌套位置模式允许（`Line(Point(x,_), Point(u,_)) = seg;`）。

## 实现落点（Scope 文件）

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/compiler/z42c.syntax/src/Stmt.z42` | MODIFY | 新增 `DeconstructDeclStmt { Pattern Pat; Expr Init }` 节点 |
| `src/compiler/z42c.syntax/src/Parser.z42` | MODIFY | `ParseStatement`（:216）新增解构声明判别分派 |
| `src/compiler/z42c.syntax/src/StmtParser.z42` | MODIFY | 新增 `_isDeconstructDeclStart()`（lookahead `Ident ( ... ) =`）+ `_parseDeconstructDecl()`（复用 `_p._patP._parsePrimaryPattern` 得 PositionalPattern + 消费 `= init ;`） |
| `src/compiler/z42c.semantics/src/BoundStmt.z42` | MODIFY | 新增 `BoundDeconstructDeclStmt { BoundPattern Pat; BoundExpr Init }` |
| `src/compiler/z42c.semantics/src/PatternBinder.z42` | MODIFY | 新增 irrefutability 校验 helper（遍历子模式仅 wildcard/binding/nested-positional） |
| `src/compiler/z42c.semantics/src/StmtBinder.z42` | MODIFY | `_bindStmt` 分派 + `_bindDeconstructDecl`（`_bindExpr(init)` → `_pattern.Bind(pat, init.Type(), env)`，绑进**当前 env**） |
| `src/compiler/z42c.semantics/src/PatternEmitter.z42` | MODIFY | 新增 `EmitIrrefutable`（无 IsInstance / 无失败分支，逐字段 field_get 直读 + 绑定，递归嵌套） |
| `src/compiler/z42c.semantics/src/StmtEmitter.z42` | MODIFY | `_emitStmt` 分派 + emit（`Emit(init)` → `_pat.EmitIrrefutable(subj, pat, contL)` → 续 contL） |
| `src/tests/pattern-matching/pattern_destructure.z42` | NEW | e2e：单层/嵌套/带常量拒绝（负例诊断）；interp+jit 双验 |
| `docs/book/src/language/pattern-matching.md` | MODIFY | 补解构声明语法 + irrefutable 约束 |
| `examples/patterns.z42` | MODIFY | 补解构声明示例（可选） |

## 自举 / 格式影响

- **无 zbc/zpkg 格式 bump、无新 token、无新 runtime**：复用既有 `field_get`/`IsInstance`/`Copy` IR；
  解构声明是既有 `<Pattern>` 文法 + `=` 的组合，无新词法记号。
- **两-nightly 纪律满足**：新语法只在 e2e 测试文件出现，z42c / stdlib / xtask 源一律不用 → 上一 nightly
  的 z42c 仍能编当前源。
- **自举字节不动点**：z42c 源无解构声明用法 → gen1==gen2 天然成立（新语句节点仅在 fresh z42c 编测试时出现）。
- **⚠️ syntax→semantics 跨包新符号**：`DeconstructDeclStmt`（syntax）被 semantics 消费——A1 已修
  `_buildCompilerViaZ42c` retry-on-fail 冷启动环（bootstrap-seed 轴④变体），此路径已收敛；仍须 clean-cold 本地验。

## User 6.5 裁决（已确认）

1. **不可失败性路线 = (A) 静态限制 irrefutable 子模式**（对齐 Rust `let`；不做运行时 panic）。
2. **初版 scope = 仅位置形态 `T(p,...)`**（属性形态 defer）。
3. **类型测试子模式 `T x` 不纳入 irrefutable 白名单**（从简禁止）。

补充实现决策：irrefutable 的类型约束落为**精确类型名匹配**（顶层=init 静态类型，嵌套=父 record 字段
类型），保证 `IsInstance` 恒真、lowering 无 IsInstance / 无失败分支（`PatternEmitter.EmitIrrefutable`）。
