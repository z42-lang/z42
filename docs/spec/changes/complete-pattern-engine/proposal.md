# Proposal: 模式引擎补齐 —— 属性解构声明 + `is` 放开 or/@ + struct record 解构

> **规范先行 DRAFT**（lang 变更）。待 User 6.5 裁决后进入 IMPL。

## Why

模式匹配引擎 A1（#306）+ A2（#308）+ A3（#309）+ B 解构声明（#311）+ C 穷尽性（#312）+
D `with`（#313）已全部合入 main，建成三层递归下降引擎（PatternParser → PatternBinder →
PatternEmitter），服务 `switch`（语句 + 表达式）、`is`、解构声明四个应用位点。

`docs/book/src/language/pattern-matching.md` 的 **Deferred 段**列出若干后续独立特性。本 change 一次性
补齐其中**三个「引擎已基本就位、只差最后一段接线」的缺口**（均**无格式 bump、无新 token、无新关键字**）：

| # | Deferred 项（book L297-302） | 现状缺口 |
|---|------------------------------|----------|
| 1 | 解构声明的属性形态 `Point { X: x } = p` | 引擎已有 PropertyPattern，但 `CheckIrrefutable`/`EmitIrrefutable` + parser lookahead 只认位置形态 |
| 2 | `is` 中的 or / `@` | binder 已位点无关支持 or/@，纯 parser 层把 `is` 限死走 `_parsePrimaryPattern`（不接 or-链/@） |
| 3 | struct record 位置解构 | A1 起 `_bindPositional` 硬 defer `IsStruct`；emit 侧字段读未按 struct blob 布局分派 |

**为何捆一个 change**：三者共享 `PatternParser`/`PatternBinder`/`PatternEmitter` 同一批文件，且特性 3
把 PatternEmitter 的字段读改成「按 subject 是否 blob-struct 分派」这一改动**同时惠及**特性 1 的属性字段读——
拆成三个 PR 会像 B/C/D 那样陷入「同文件 + 同 doc 段」的连环 rebase 返工。捆为一个「补齐模式引擎剩余缺口」
逻辑单元，避开该坑。**元组模式 / 泛型 record 解构 / `with` struct·`..base` / init 访问器 / sealed 穷尽性**
仍各自 defer（元组要引新类型系统构造、可能格式 bump，另开专项）。

---

## 特性 1 — 属性形态解构声明 `{X: x, Y: y} = p`

B（#311）已实现**位置形态**解构声明 `Point(x, y) = p`（不可失败 irrefutable）。本特性把它扩到**属性形态**：

```z42
[Record] class Point(int X, int Y);
Point p = Point(3, 4);
{ X: x, Y: y } = p;        // x ← 3, y ← 4（省类型，类型 = p 的静态类型）
{ X: x } = p;              // 部分字段：只绑 x ← X（属性形态天然允许省字段）
Point { X: x } = p;        // 带类型标注（须 = p 静态类型，恒真无失败分支）
Line { Start: Point(a, _) } = seg;   // 嵌套位置子模式
```

### 设计要点

- **引擎已就位**：`PatternParser._parsePropertyBody`（PatternParser.z42:126-149）已完整解析 `{F:p}` 与
  `T{F:p}`；`PatternBinder._bindProperty`（:200-226）已实现（可选类型 + 逐字段 `Fields.ContainsKey` 校验 +
  递归 Bind）。**缺口只在解构声明这条不可失败路径的两处**：
  1. `CheckIrrefutable`（PatternBinder.z42:38-51）当前只识别 Wildcard/Binding/**Positional**，PropertyPattern
     落兜底报错 "must be irrefutable" → 新增 `BoundPropertyPattern` 分支。
  2. `EmitIrrefutable`（PatternEmitter.z42:154-177）当前只处理 Binding/Positional → 新增 PropertyPattern
     分支（逐字段直读 + 递归，无 IsInstance/无失败分支）。
- **parser lookahead 扩展**：`_isDeconstructDeclStart`（StmtParser.z42:337-352）当前硬编码 `T(...)=`。新增两支：
  `{...}=`（首 token `LBrace`，配平 `{}` 后见 `=`）和 `T{...}=`（`_skipTypeOffset` 后跟 `LBrace`）。
  **⚠️ 歧义消解**：`{ ... } = ...` 语句起始与**块语句** `{ ... }` 撞——用「配平 `}` 后必须紧跟 `=`」判别
  （块语句后不会跟 `=`），同 B 的 `T(...)=` lookahead 思路。
- **irrefutable 语义**（对齐 B 与 Rust `let`）：
  - **子模式白名单**：字段值子模式只允许 irrefutable（通配/裸绑定/嵌套位置/嵌套属性），禁常量/or/范围/关系。
  - **类型标注 `T{...}` 须精确 = init 静态类型**（`IsInstance` 恒真 → 无失败分支），同 B 位置形态的精确类型约束；
    省类型时类型 = init 静态类型。
  - **部分字段合法**：属性形态可只列部分字段（`{X:x}` 不绑 Y）——不影响 irrefutable（未列字段不解构、不失败）。

---

## 特性 2 — `is` 表达式放开 or `|` 和 `@`

A2 把 or/`@` 限制为 **switch-only**，`is` 内不支持。经复核，这个限制**可安全放开**：

```z42
if (p is Circle(r) | Square(r)) { use(r); }   // or-模式带绑定（A3 引擎已支持）
if (p is c @ Circle(_)) { use(c); }            // @ 绑定整体
```

### 为何 A2 当初限制、为何现可放开（事实校正）

A2 设计注释（ExprParser.z42:119-120 / Pattern.z42:79）称「`is` 内 `|` 保持位或、`@` 与类型引导冲突」。
**复核后两条顾虑都不成立**：

- **or `|`**：`is` 走 `_parsePrimaryPattern`（不接 or-链），故 `x is A | B` 现解析为 `(x is A) | B`。但
  `(x is A)` 是 **bool**，z42 的 `|` **只对整数操作数**（`bool | X` 恒报 "bitwise op requires integral
  operands"）→ **现状下 `is ... | ...` 是恒定的类型错误，没有任何合法程序走这条解析**。故让 `is` 的模式解析
  改走 `_parsePattern`（or-链）**不会回归任何合法程序**。已 grep 坐实 z42c/stdlib/xtask 源无单管道 `is`
  用法（`x is T || ...` 是 `||` 逻辑或、独立 token，不受影响）→ **自举字节不动点安全**。
- **`@`**：`x is c @ Circle(_)` 的 `c` 与类型名的「冲突」用**一 token 前瞻**消解——`is` 后见 `Identifier`
  且**下一 token 是 `@`** → At-模式（类型名后永不合法跟 `@`）；否则走原类型引导（`x is Circle p`）。无歧义。

### 设计要点

- `ExprParser.z42:122` is 结构化分支：`_parsePrimaryPattern()` → `_parsePattern()`（接 or-链）；类型引导路
  （L127-131）同理让 `x is A(..) | B(..)` 可接 or-链。
- `_isPatternLead`/前瞻加 `Identifier @` 分支（仅 is 路），进 At-模式。
- **binder/emitter 零改**：`Bind` 对 OrPattern/AtPattern 位点无关（PatternBinder.z42:27-28），`is` 走
  `TypeOpTyper._bindIsPatternExpr`（:84-88）直接 `Bind`；emit 走 `EmitMatch`（A3 已支持 or-带绑定的 phi-free
  合流）。**or 带绑定在 `is` 真分支可见**（同 switch 臂），引擎天然支持。

---

## 特性 3 — struct record 模式解构（去 `!IsStruct` 限制）

A1 起位置模式硬 defer struct record（`PatternBinder.z42:176`，诊断码 **TypeMismatch**，非 E0402；message
"positional pattern on struct record is not yet supported (A1)"）。本特性放开：

```z42
[Record] struct Point(int X, int Y);
switch (p) {
    case Point(0, y): ...      // struct record 位置模式
    case Point(x, y): ...
}
Point(x, y) = p;               // struct 解构声明
```

### 根因 / 难点（Explore 坐实）

A1 defer 的**真实根因**在 emit 侧：`PatternEmitter` 无条件发 `FieldGetInstr(fReg, subj, name)`（按名读堆对象
字段，PatternEmitter.z42:166/199），**对 blob-struct 是错的**——struct 字段无 auto-property getter，值语义按
**字节偏移 + TypeTag** 读（`AccessEmitter.z42:149-162` 的 `_emitBlobFieldGet`/`StructFieldGetPrim` +
`StructLayout` 偏移）。误用 `FieldGetInstr` 会「运行时按 struct tag 解码基元 → 崩」。

**另有潜伏 bug**：`_bindProperty`（PatternBinder.z42:200-226）**无** `IsStruct` 检查 → 属性模式在 binder
已放行 struct，但 emit 同样走错 `FieldGetInstr`。本特性一并修（binder 放行 + emit 正确分派）。

### 设计要点

- **binder**：删 `_bindPositional` 的 `if (ct.IsStruct)` defer（PatternBinder.z42:176-179）。
- **emitter**：`_emitFieldSeq`（:188-206）+ `EmitIrrefutable`（:161-174）字段读按 **subject 是否 blob-struct**
  分派——struct 走 blob 偏移读（参照 `AccessEmitter.z42:149-162`，需 `_ctx.Gen.Layouts` 的 `StructLayout`
  偏移 + `Tag.FromName`），非 struct 保持 `FieldGetInstr`。此改**同时修好特性 1 属性字段读对 struct 的正确性**。
- **值语义副本**：struct 字段读出若为内联 struct（嵌套 struct record）需拷进新局部 blob（避别名/悬垂，同
  AccessEmitter 内联 struct 处理 :152-162）。初版可**先限字段为基元 + 引用类型**，嵌套 struct 字段解构 defer。
- **⚠️ jit 双验必做**：struct blob 读 + 分支 + 合流寄存器，是 record 程序「as_cast+field_get jit 误编」同类
  高风险区 → interp 全过≠jit 过，`--mode jit` 双跑（record-value-semantics 血泪教训）。
- **仍 defer**：泛型 record 解构（另项）；本特性只放开非泛型 struct record。

---

## 自举 / 格式影响（三特性统一）

- **无 zbc/zpkg 格式 bump、无新 token、无新关键字、无新 runtime builtin**：全部复用既有 IR
  （`FieldGetInstr`/`StructFieldGetPrim`/`IsInstance`/`Copy`/比较/`BrCond`）与既有词法记号（`{`/`}`/`|`/`@`/`=`）。
- **两-nightly 纪律满足**：三特性的新语法/新语义**只在 e2e 测试文件出现**，z42c/stdlib/xtask 源一律不用 →
  上一 nightly 的 z42c 仍能编当前源（特性 2 已 grep 坐实源无 `is ... |`/`is ... @`）。
- **自举字节不动点**：z42c 源无本批新用法 → gen1==gen2 天然成立。特性 2 的 parser 改动虽触及 `is` 解析路，但
  仅在「`is` 后遇 `|`/`@`」时行为改变，而该组合在旧源恒为类型错误、不出现 → 现有 `is` 用法（`x is T`/`x is T v`/
  `x is T(..)`/`x is T{..}`/`x is >0`/`x is 1..=2`）逐字节不变。
- **⚠️ syntax→semantics 跨包新符号**：本批**无新 AST 节点跨包**（PropertyPattern/OrPattern/AtPattern 均 A1/A2
  已有），故不触发 bootstrap-seed 轴④冷启动环；仍 clean-cold 本地验（`rm -rf artifacts/build/compiler`）。

---

## 实现落点总表

| 文件 | 特性 | 变更 |
|------|------|------|
| `z42c.syntax/StmtParser.z42` | 1 | `_isDeconstructDeclStart`（:337）加 `{...}=`/`T{...}=` 两支 + `_parseDeconstructDecl` 分派属性模式 |
| `z42c.syntax/ExprParser.z42` | 2 | is 分支（:122/:130）`_parsePrimaryPattern`→`_parsePattern`；加 `Ident @` 前瞻 |
| `z42c.syntax/PatternParser.z42` | 2 | `_isPatternLead`/前瞻支持 is 路的 `@`（At-模式） |
| `z42c.semantics/PatternBinder.z42` | 1,3 | `CheckIrrefutable`（:38）加 PropertyPattern 分支；`_bindPositional`（:176）删 IsStruct defer |
| `z42c.semantics/PatternEmitter.z42` | 1,3 | `EmitIrrefutable`（:154）加 PropertyPattern；`_emitFieldSeq`/`EmitIrrefutable` 字段读按 blob-struct 分派 |
| `src/tests/pattern-matching/pattern_gaps.z42`（或分 3 文件） | 1,2,3 | e2e：属性解构声明（含部分字段/嵌套/负例）、is or/@（含带绑定）、struct record 解构（switch+decl，**jit 双验**） |
| `docs/book/src/language/pattern-matching.md` | 1,2,3 | 补三特性文档，Deferred 段移除已实现项 |
| `examples/patterns.z42` | 1,2,3 | 补示例（可选） |

---

## User 6.5 裁决（已确认 2026-08-28，全部按推荐）

1. **特性 1 类型标注约束 = 精确匹配**：`T{...}=p` 要求 `T` 精确 = p 静态类型（保 irrefutable 无失败分支，同 B 位置形态）。
2. **特性 2 = 放开 or 带绑定进 `is`**：`if (p is Circle(r) | Square(r))` 的 `r` 在真分支可见，与 switch 臂完全对等。
3. **特性 3 嵌套 struct 字段 = defer**：初版限 struct record 字段为基元 + 引用类型；嵌套 struct record 字段解构另项 defer。
4. **特性 3 覆盖位点 = 全放开**：struct record 在 switch/is/解构声明 + 位置/属性模式全支持（emit 一处分派天然全覆盖 +
   顺带堵 `_bindProperty` 潜伏 bug）。
5. **测试粒度 = 三个独立 e2e 文件**：`pattern_prop_destructure.z42` / `pattern_is_oral.z42` / `pattern_struct_record.z42`。
