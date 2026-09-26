# Tasks: L2 规则表（批 2）

> 状态：🔵 DRAFT 待审批 ｜ 前置：[design.md](design.md) ｜ 切分按 D2-0（推荐乙：2a / 2b 两条 PR）

## T0 —— 开工核查（先做，结论写回本文件）

- [ ] `gh pr list` 重查在飞 PR（[[check-inflight-prs-before-starting]]）；**merge 前紧邻再查一次**
- [ ] 诊断码：本批预期**不新增**诊断码（复用 E0301）。若需新码，扫全源**不够** ——
      还要 `git show <每个在飞 PR 分支>:DiagnosticCodes.z42`（[[diagnostic-code-uniqueness-program]]）
- [ ] 行数余量复核：`ExprParser.z42` 864 / 886（只剩 22 行）
- [ ] **实测 L9**（特性门覆盖面洞）：夹具 `break;`（无循环）+ `[syntax] control_flow = false`
      ⇒ 记下今天的实际诊断（是 E0301 还是语义层的「break 不在循环里」）。
      两种都红、**红的理由不同** ⇒ 这是本批要补的行为差
- [ ] 取**同基点字节基线**：在 `origin/main`（`4b0d7a018`）全量构建，留 25 包 zpkg 的 sha256
      ⚠️ 换基点后基线作废、必重取（[[verify-conclusion-after-reseeding]]）

## PR 2a —— 表达式规则表【纯收敛，字节必须不变】

### T1 `ParseTable.z42`（新文件，`z42c.syntax`）
- [ ] `class ParseRule { int LeftBp; int LedKind; string Feature; }`
- [ ] `static class LedKind`（int tag，照 `BinaryTypeTable` 的 `OperandKind`/`ResultKind` 形态）
- [ ] `ParseTable.Lookup(int kind) -> ParseRule`（if-else 串）—— 收录 proposal Why #1 那张表的**全部**
      九种形态：后缀链（bp **90**）/ 后缀 switch·with（85）/ Lt 泛型回溯（60）/ is·as（60）/
      `??`（25）/ 三元（20）/ 赋值族（10）/ 二元 11 个（30…80）
- [ ] 关系函数（D2-2）：`UnaryBp()` / `AboveAssign()` / `BetweenOrAndXor()` ——
      **从表里算**，不得各自写死同一数值
- [ ] 抬头注释写清 **SoT 范围**（批 1 的教训：SoT 范围写不准，「唯一 SoT」本身就是假话）：
      收的是「绑定力 + led 角色 + feature」；**不收** `_isLambdaStart` 的位置约束、
      cast 的 follow 集、`_pendingGt` 回溯三件套 —— 那些是有状态判据，留在原地

### T2 `_parseExpr` 主循环改查表派发
- [ ] 九处内联守卫 → 一处 `if (r == null || r.LeftBp < minBp) break;`
- [ ] 各分支体**原样搬**（一行不改语义），按 `LedKind` 分派
- [ ] 后缀链 bp 90 的等价性断言（见 V2）
- [ ] 净增行数 ≤ 22（否则拆 `ExprParserLed.z42`，照 `ExprParserInterp.z42` 先例）
- [ ] **单独字节对账**（不与 T3 合并对）

### T3 三处魔数改成关系函数
- [ ] `ExprParser:96,99` `_parseExpr(11)` → `AboveAssign()`
- [ ] `ExprParser:852` `_parseExpr(10)` → `LeftBp(TokenKind.Eq)`
- [ ] `PatternParser:94` `_parseExpr(45)` → `BetweenOrAndXor()`
- [ ] `_parsePrefix` 六处 `_parseExpr(85)` → `UnaryBp()`
- [ ] **单独字节对账**

## PR 2b —— 语句表 + 顺序表 + 特性门【唯一有行为变更的一段】

### T4 `StmtRules`（落 `Parser.z42` 同层新文件或 `ParseTable.z42` 内）
- [ ] `class StmtRule { int StmtKind; string Feature; }` + `StmtRules.Lookup(kind)`
- [ ] `ParseStatement` 的门段（`:317-322`）改为**遍历表取 `Feature`** ⇒ 加语句时漏挂门不再可能
- [ ] 派发段（`:324-333`）改按 `StmtKind` tag 分派
- [ ] 补 L9 的洞（D2-4）：`break`/`continue` 表项 `Feature = "control_flow"`；
      `throw` 表项 `Feature = "exceptions"` —— **新 E0301 发射点**
- [ ] `using`/`namespace` 拦截（`:295-305`）与兜底（`:337-339`，**没有 else**）保持原位、原语义

### T5 lookahead 顺序表 + 顺序门（D2-3）
- [ ] `static class ProbeKind` + `StmtProbes.Order`（有序 int tag 数组），每项带「为什么在这个位置」
- [ ] `_isDeconstructDeclStart` 排在 `_isVarDeclStart` 之后的**隐含理由**首次写下来
      （今天全仓无注释解释，理由散在 `StmtParser.z42:217-219,339-342,347-348`）
- [ ] 顺序门：单测拿**两张顺序表**跑同一批夹具（正序 + 故意反序），四条夹具见 design D2-3
- [ ] 判别力：反序那遍要验**红的理由**（诊断码/消息），不只退出码

### T6 运算符级特性门变活（D2-4）
- [ ] `bitwise` → `|` `^` `&` `<<` `>>` 五条表项
- [ ] `ternary` → `?:` 表项
- [ ] 各一条负例门（照 `_syntaxKnobTakesEffect` 的四判据形态）
- [ ] 一致性门：遍历两张表，每个非空 `Feature` 必须 `Phase1Profile().Has(name)`；
      **判别力靠注入假名必须红**来验（批 1 刻意没建名字对账门，因为那时判别力为零；
      现在表里有 feature 字段了，这门才有意义）

### T7 文档
- [ ] `syntax-customization.md`：页首状态行（`:3`）/ 现状表（`:6-14`）/ 「21 个特性」（`:62`）/
      「关掉任何开关都不会改变解析行为」（`:64`）/ 一致性检查一节（`:197`）——按 proposal Why #4 逐行订正
- [ ] 同页 `:140-146,177-193`：`nud:`/`led:`/`handler:` **函数指针形态在 z42c 里写不出来**（L1/L2）
      ⇒ 改为 int tag + 集中派发的实际形态
- [ ] 「死旋钮 13 个」→ **11 个**，三处同步（`LanguageFeatures.z42` 抬头 / `z42-toml.md` /
      `syntax-customization.md`）
- [ ] `MinimalProfile` 的事实（D2-5 甲）：profile 不可从 manifest 选择；订正 `:311` 教学场景措辞
- [ ] `z42-toml.md` 的 `[syntax]` 节补上新变活的名字（`bitwise` / `ternary`）
- [ ] `operators.md`：优先级表改为指向 `ParseTable` 为 SoT（今天那张表是第二份数值拷贝）

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
