# Tasks: L2 规则表（批 2）

> 状态：🟢 已裁决，IMPL 中 ｜ 前置：[design.md](design.md)
> 裁决（2026-09-26）：D2-0 **一条 PR 全做** ｜ D2-1 **全 Pratt（led tag 派发）** ｜
> D2-4 `bitwise` + `ternary` + L9 补洞 + **`pattern_match`**（死旋钮 13 → **10**）｜
> D2-5 **删掉 `MinimalProfile`** ｜ D2-6 `pattern_match` 边界 = **甲**（5 个引擎入口，保留 `x is T`）
> ⚠️ 一条 PR 全做 ⇒ **每个 task 单独字节对账**，不只对总账（批 1 的代价已实测）。
> 字节基线已取：`origin/main` = `4b0d7a018`，50 个 release zpkg 的 sha256 存
> `scratchpad/baseline-4b0d7a018.txt`（`build all` 输出核过 `25 succeeded, 0 failed` 且无 `error(s)`）。

## T0 —— 开工核查（先做，结论写回本文件）

- [x] `gh pr list` 重查在飞 PR（[[check-inflight-prs-before-starting]]）；**merge 前紧邻再查一次**
- [x] 诊断码：本批预期**不新增**诊断码（复用 E0301）。若需新码，扫全源**不够** ——
      还要 `git show <每个在飞 PR 分支>:DiagnosticCodes.z42`（[[diagnostic-code-uniqueness-program]]）
- [x] 行数余量复核：`ExprParser.z42` 864 / 886（只剩 22 行）
- [x] **实测 L9**（特性门覆盖面洞）：夹具 `break;`（无循环）+ `[syntax] control_flow = false`
      ⇒ 记下今天的实际诊断（是 E0301 还是语义层的「break 不在循环里」）。
      两种都红、**红的理由不同** ⇒ 这是本批要补的行为差
- [x] 取**同基点字节基线**：在 `origin/main`（`4b0d7a018`）全量构建，留 25 包 zpkg 的 sha256
      ⚠️ 换基点后基线作废、必重取（[[verify-conclusion-after-reseeding]]）

## 第一段 —— 表达式规则表【纯收敛，字节必须不变】

### T1 `ParseTable.z42`（新文件，`z42c.syntax`）
- [x] `class ParseRule { int LeftBp; int LedKind; string Feature; }`
- [x] `static class LedKind`（int tag，照 `BinaryTypeTable` 的 `OperandKind`/`ResultKind` 形态）
- [x] `ParseTable.Lookup(int kind) -> ParseRule`（if-else 串）—— 收录 proposal Why #1 那张表的**全部**
      九种形态：后缀链（bp **90**）/ 后缀 switch·with（85）/ Lt 泛型回溯（60）/ is·as（60）/
      `??`（25）/ 三元（20）/ 赋值族（10）/ 二元 11 个（30…80）
- [x] 关系函数（D2-2）：`UnaryBp()` / `AboveAssign()` / `BetweenOrAndXor()` ——
      **从表里算**，不得各自写死同一数值
- [x] 抬头注释写清 **SoT 范围**（批 1 的教训：SoT 范围写不准，「唯一 SoT」本身就是假话）：
      收的是「绑定力 + led 角色 + feature」；**不收** `_isLambdaStart` 的位置约束、
      cast 的 follow 集、`_pendingGt` 回溯三件套 —— 那些是有状态判据，留在原地

### T2 `_parseExpr` 主循环改查表派发
- [x] 九处内联守卫 → 一处 `if (r == null || r.LeftBp < minBp) break;`
- [x] 各分支体**原样搬**（一行不改语义），按 `LedKind` 分派
- [x] 后缀链 bp 90 的等价性断言（见 V2）
- [x] 净增行数 ≤ 22（否则拆 `ExprParserLed.z42`，照 `ExprParserInterp.z42` 先例）
- [x] **单独字节对账**（不与 T3 合并对）

### T3 三处魔数改成关系函数
- [x] `ExprParser:96,99` `_parseExpr(11)` → `AboveAssign()`
- [x] `ExprParser:852` `_parseExpr(10)` → `LeftBp(TokenKind.Eq)`
- [x] `PatternParser:94` `_parseExpr(45)` → `BetweenOrAndXor()`
- [x] `_parsePrefix` 六处 `_parseExpr(85)` → `UnaryBp()`
- [x] **单独字节对账**

## 第二段 —— 语句表 + 顺序表 + 特性门【唯一有行为变更的一段】

### T4 `StmtRules`（落 `Parser.z42` 同层新文件或 `ParseTable.z42` 内）
- [x] `class StmtRule { int StmtKind; string Feature; }` + `StmtRules.Lookup(kind)`
- [x] `ParseStatement` 的门段（`:317-322`）改为**遍历表取 `Feature`** ⇒ 加语句时漏挂门不再可能
- [x] 派发段（`:324-333`）改按 `StmtKind` tag 分派
- [x] 补 L9 的洞（D2-4）：`break`/`continue` 表项 `Feature = "control_flow"`；
      `throw` 表项 `Feature = "exceptions"` —— **新 E0301 发射点**
- [x] `using`/`namespace` 拦截（`:295-305`）与兜底（`:337-339`，**没有 else**）保持原位、原语义

### T5 lookahead 顺序表 + 顺序门（D2-3）
- [x] `static class ProbeKind` + `StmtProbes.Order`（有序 int tag 数组），每项带「为什么在这个位置」
- [x] `_isDeconstructDeclStart` 排在 `_isVarDeclStart` 之后的**隐含理由**首次写下来
      （今天全仓无注释解释，理由散在 `StmtParser.z42:217-219,339-342,347-348`）
- [x] 顺序门：单测拿**两张顺序表**跑同一批夹具（正序 + 故意反序），四条夹具见 design D2-3
- [x] 判别力：反序那遍要验**红的理由**（诊断码/消息），不只退出码

### T6 运算符级特性门变活（D2-4）
- [x] `bitwise` → `|` `^` `&` `<<` `>>` 五条表项
- [x] `ternary` → `?:` 表项
- [x] `pattern_match` → **5 个模式引擎入口**（D2-6 甲；`x is T` / `x is T v` **保留**）：
      `ExprParser:95`(switch 表达式 arm) / `:154`(`@` 绑定) / `:162`(字面量引导 + or-链) /
      `:172,178`(类型引导结构化 + 纯类型名 or-链) / `StmtParser:76`(case) / `:400`(解构声明)
- [x] `pattern_match` 的后果写进文档：关掉后 switch **语句**仍解析（属 `control_flow`），
      但每个 `case` 报 E0301 ⇒ 实际不可用
- [x] 各一条负例门（照 `_syntaxKnobTakesEffect` 的四判据形态）
- [x] 一致性门：遍历两张表，每个非空 `Feature` 必须 `Phase1Profile().Has(name)`；
      **判别力靠注入假名必须红**来验（批 1 刻意没建名字对账门，因为那时判别力为零；
      现在表里有 feature 字段了，这门才有意义）

### T8 删 `MinimalProfile`（D2-5）
- [x] 删 `LanguageFeatures.z42:108-116` + `z42c.core/tests/features.z42:42` 那条单测
- [x] `z42c.core/README.md:16` 去掉它
- [x] `syntax-customization.md:62,311,330` 三处：`:311` 的教学场景改写成「逐项 `false`」，
      `:330` 的「在 `MinimalProfile()` 里置 false」一步删掉
- [x] ⚠️ 它是 `z42c.core` 的**公开静态方法** ⇒ 删公开符号可能是跨成员变更，
      跑 `xtask test bootstrap` 判据（批 1 的结论：**别猜，跑判据**）

### T7 文档
- [x] `syntax-customization.md`：页首状态行（`:3`）/ 现状表（`:6-14`）/ 「21 个特性」（`:62`）/
      「关掉任何开关都不会改变解析行为」（`:64`）/ 一致性检查一节（`:197`）——按 proposal Why #4 逐行订正
- [x] 同页 `:140-146,177-193`：`nud:`/`led:`/`handler:` **函数指针形态在 z42c 里写不出来**（L1/L2）
      ⇒ 改为 int tag + 集中派发的实际形态
- [x] 「死旋钮 13 个」→ **11 个**，三处同步（`LanguageFeatures.z42` 抬头 / `z42-toml.md` /
      `syntax-customization.md`）
- [x] `MinimalProfile` 的事实（D2-5 甲）：profile 不可从 manifest 选择；订正 `:311` 教学场景措辞
- [x] `z42-toml.md` 的 `[syntax]` 节补上新变活的名字（`bitwise` / `ternary`）
- [x] `operators.md`：优先级表改为指向 `ParseTable` 为 SoT（今天那张表是第二份数值拷贝）

## V —— 验收（判别力）

| # | 判据 | 怎么验「红的理由对」 |
|---|---|---|
| V1 | **字节不变**：2a 两个 commit 各自 25 包 zpkg sha256 与基线一致（只允许自身源码被改的包变）| 不为零就逐 commit 二分 |
| V2 | 后缀链 bp 90 等价：断言 `PostfixBp > UnaryBp()` 且 `UnaryBp() >= ` 全部 `LeftBp` 最大值 | 把 90 改成 80 ⇒ 单测必须红 |
| V3 | bp 阶梯不变式：`LeftBp(Pipe) < BetweenOrAndXor() <= LeftBp(Caret)`、`AboveAssign() == LeftBp(Eq)+1` | 改错任一数值 ⇒ 红 |
| V4 | 顺序门：四条夹具 × 正反两张顺序表 | 反序必须红且诊断对 |
| V5 | 特性门：`bitwise=false` ⇒ `a & b` 报 E0301；`ternary=false` ⇒ `a?b:c` 报 E0301；`control_flow=false` ⇒ `break;` 报 E0301 | 阳性对照（不写 `[syntax]`）必须编得过 |
| V6 | 一致性门：表里注入 `Feature = "no_such"` ⇒ 门必须红 | 这是唯一能证明该门不是空门的做法 |
| V7 | 自举不动点 3/3 + golden 全绿 + `build sdk` → `test examples` + `--mode jit` 一轮 | — |
| V8 | `grep -rn "E0301" examples/ docs/learn/`：新发射点可能让记着期望错误的示例变红（[[local-green-misses-examples-gate]]）| — |

## 顺序

T0 → （2a）T1 → T2 → T3 → （2b）T4 → T5 → T6 → T7。
2a 全绿且字节对齐后再开 2b；2b 合并前重跑 2a 的字节判据（main 已动过）。

## 实施结论（2026-09-26，逐项实测）

| # | 结论 |
|---|---|
| 字节 | **每个 task 单独对账，全部只有 `z42c.syntax` 自身变**（其源码被改），其余 24 包字节不变 —— 而那 24 包正是用**含本批改动的新 z42c** 编出来的 ⇒ 发射结果等价 |
| 🔴 陷阱 | `Lt` 的泛型调用尝试**不受 minBp 约束**（`a + f<int>(x)` 以 minBp=71 进循环也要认）⇒ 表把 `Lt` 记成两个角色；若并进统一守卫之后就是行为变更。此前这条只由「代码写在守卫之前」的行文位置表达 |
| 行数 | `ExprParser.z42` 865 → 852（净 -13；删掉已无调用方的 `_infixBp`/`_isAssignOp`，长注释挪进 `ParseTable` 抬头）。硬限 886 |
| L9（实测） | `break`/`continue`/`throw` 确实**没挂门**；已补齐（`break;` + `control_flow=false` 现在报 E0301）|
| 特性门 | 死旋钮 **13 → 10**；新接 `bitwise` / `ternary` / `pattern_match` |
| 门的判别力 | 一致性门：注入假特性名 ⇒ **stdlib 构建照常全绿、只有那道单测红**（这正是它唯一的存在理由）。顺序门：四条夹具各自跑正序 + 故意写反的顺序表 |
| bootstrap | `xtask test bootstrap` 绿 ⇒ 删 `MinimalProfile`（公开静态方法）**不必跨 nightly**（同批 1 的结论：跑判据别猜）|
| 形态 | 设计页画的 `nud:`/`led:`/`handler:` **函数指针在 z42c 里写不出来**（无 delegate）⇒ 落地是 int tag + 集中派发；且表用「常量 + 查询函数」而非返回 rule 对象（主循环是最热路径，返回对象 = 每轮一次分配）|

### 🔴 本批自己制造的一次事故（教训已入 memory）

拿 `?:`（编译器自身大量使用）做「假特性名」实验 ⇒ 产出的编译器连自己都编不了，
毒化产物落进 `artifacts/build/libraries/dist/` ⇒ 此后每次构建都装配它 = **自举死锁**。
`xtask build all` / `--toolchain <种子>` 都救不回来；唯一出路是用种子重建整个 workspace：
`cd src/libraries && ../../.z42/bin/z42c build --workspace --release`。
⇒ 假特性名实验必须选**全仓零出现**的构造（`??` 行）。

### 副产物（真结论）

**`xtask build all` 与 `xtask build stdlib` 对 `z42.core` 产出不同字节**（stash 掉改动跑同一条
命令，`z42.core` 照样变）⇒ 字节对账的基线必须用**同一条命令**取，否则会把命令差异读成
「我的改动动了 stdlib」。
