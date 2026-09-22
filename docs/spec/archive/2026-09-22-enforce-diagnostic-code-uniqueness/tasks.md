# Tasks: 诊断码唯一性

> 状态：🟢 已完成（2026-09-22）｜ scope: compiler + stdlib + toolchain + docs（无格式 bump）

## 进度概览

| # | 阶段 | 状态 |
|---|---|---|
| 0 | 撞码普查（42 个字面量码逐个对账，坐实真撞码只有 2 处） | 🟢 已完成 |
| 1 | 登记表补全（10 新常量 + 2 撞码归位 + I0466/I0467 名实归位） | 🟢 已完成 |
| 2 | 撞码归位（发射点 + 测试 + transcript 跟号） | 🟢 已完成 |
| 3 | `xtask test diagcodes` gate（四条规则）+ 四处接线 | 🟢 已完成 |
| 4 | 门的判别力验证（六次注入逐条验红 → 还原即绿） | 🟢 已完成 |
| 5 | 文档（error-codes.md + test-gate.md） | 🟢 已完成 |
| 6 | 完整 GREEN + 归档 | 🟢 已完成 |

## 阶段 0 —— 撞码普查（勿重跑）

- [x] 0.1 扫出 113 个码常量（**无重复值**）+ 42 个字面量码 / 104 个发射站点。
- [x] 0.2 33 个码「既有常量又被字面量发」→ 逐个比对消息文本，确认其中 31 个同义、
      **只有 E0474 / E0477 是真的一码两义**。
- [x] 0.3 `git log -S` 定先后：E0474 先来 = #737 混合访问器；E0477 先来 = #745 funcref。
- [x] 0.4 查实 `z42c.syntax` / `z42c.semantics` 都依赖 `z42c.core` 且已在别处用常量
      ⇒ 字面量惯例无技术必需性，可整体废除。
- [x] 0.5 查实 E0481 / E0482 全仓未被占用；`ForwardSkipped = "I0467"` 全仓零引用（死常量）。

## 阶段 1 —— 登记表补全

- [x] 1.1 补 10 个常量：E0470–E0477 族 8 个 + `ValueTypeNullComparison`(E0481) + `SwitchNotExhaustive`(W0700)。
- [x] 1.2 `AssignTargetNotLvalue` 改值 E0477 → **E0482**（后到者搬家）。
- [x] 1.3 `ForwardSkipped` 改值 I0467 → **I0466**（名实归位；I0467 退役不复用）。
- [x] 1.4 复核：123 个常量、**零重复码值**。

> ⚠️ 踩过一次：用 `git checkout <file>` 还原「注入的假违例」，把**自己在同一文件里的全部改动**
> 一并抹回 HEAD（登记表 10 个新常量全没了，靠 `grep -c` 才发现）。此后注入一律 `cp` 备份还原。

## 阶段 2 —— 撞码归位

- [x] 2.1 `TypeChecker.z42` 值类型 null 比较：`"E0474"` → `"E0481"`。
- [x] 2.2 `AssignTyper.z42` 赋值非左值：`"E0477"` → `"E0482"`（含 2 处注释）。
- [x] 2.3 测试跟号：`value_type_null_tests.z42`(8) / `assign_lvalue_tests.z42`(9)。
- [x] 2.4 文档跟号：`docs/reference/src/language/types.md`。
- [x] 2.5 **transcript 跟号**：`examples/types/structs-records/gaps/run.console` 里实打实印着
      `E0477:` —— `examples` 门会重放实跑输出，漏了必红。（靠 `--include="*.console"` 全仓扫到的。）

## 阶段 3 —— gate

- [x] 3.1 `scripts/test/xtask_test_diagcodes.z42`：四条规则（重复值 / 未登记 / 幽灵常量名 / 字面量清单棘轮）。
- [x] 3.2 `diag-literal-emitters.txt` 基线（50 条 `(码, 文件)` 对）+ `--update` 重生成。
- [x] 3.3 接线四处：CLI 子命令表 + CLI 分派 + `_gateStageNames()` + `_testAll` stage 块。
- [x] 3.4 `docs/internals/src/devinfra/test-gate.md` 机器可读清单（漏改会被 `_checkGateStageDoc` 判红）。

## 阶段 4 —— 判别力验证（六次注入，全部 `cp` 备份还原）

| 注入 | 期望 | 实测 |
|---|---|---|
| 登记表加 `FakeDupInjected = "E0401"` | ① 红 | ✅ exit 1，指名两个常量 |
| 源里发 `"E0999"`（未登记） | ② 红 | ✅ exit 1 |
| 源里引 `DiagnosticCodes.NoSuchCodeName` | ③ 红 | ✅ exit 1 |
| **源里给已登记的 `"E0474"` 加第二个发射点**（回放 #741） | ④ 红 | ✅ exit 1 |
| 基线删一条 | ④ 红（新增方向） | ✅ exit 1 |
| 基线加一条幽灵 | ④ 红（陈旧方向） | ✅ exit 1 |
| 全部还原 | 绿 | ✅ exit 0 |

> ⭐ 第 4 行是本阶段**唯一真正有信息量**的一次：只有 ①②③ 的话，这道门会漏掉两次历史撞码里的
> 一次（#741 直接发已登记码的字面量、根本没碰登记表）。**先验判别力再说门建好了**。

## 阶段 5 —— 文档

- [x] 5.1 `error-codes.md`：撞码两行拆开、新增 E0481/E0482、I0466/I0467 归位与退役登记、
      「这张表怎么来的」从「人工 grep 重建」改写为「登记表是 SoT + 有门守着」、「新增一个码」接上门。
- [x] 5.2 顺带补 **W0701 行**（#749 落地时漏登记，全表里根本没有它）。
- [x] 5.3 `test-gate.md`：stage 流水图 + 各 stage 表 + 机制小节（含「第 ④ 条为什么不能省」的回放表）。

## 阶段 6 —— GREEN

- [x] 6.1 完整 `xtask test` 全绿（冷树：cargo 建 VM → 种子冷启动 → 两轮 stdlib → 全 stage）。
- [x] 6.2 **跨平台**：基线路径按 `lines` 门的做法把 `\` 归一成 `/`（否则 Windows 腿必红）。
- [x] 6.3 **扫描集确定性**：排除 `**/artifacts/`——构建会在 `src/` 下生成 `.z42`
      （`[Forward]` augment、REPL hooks），不排掉扫描集随「树是否构建过」漂移（实测 589 ↔ 591）。
- [x] 6.4 改完门的源码后**重建 `xtask.zpkg` 并重跑**（门禁逻辑编在 zpkg 里，不重建就是在测旧版）。

## 后续（Deferred）

- **`migrate-diag-literals-to-constants`**（晚一个 nightly）：本 PR 的 10 个新常量随 nightly 进
  z42c.core 后，把 97 个字面量发射点切回 `DiagnosticCodes.X`，清空 `diag-literal-emitters.txt`，
  把门的第 ④ 条换成更强的「非 tests 源零字面量发码」。**不能提前**（冷启动 stale-cache）。
- **在飞 #747** 持有一个已被 #752 占用的 **E0478**，本门落地后它会变红并指出该换号。

---

## 阶段 7 —— 规则 ⑤：占号也必须进登记表（后续 PR，2026-09-22）

**起因**：①-④ 只盯「**发射出去的**码」，于是「被占住、但故意零发射点」的号可以完全不进登记表。
实测 main 上有 4 个这样的号，占用只写在 `error-codes.md` 的「保留编号」「已退役的编号空间」两节：
`E0438`（预留）/ `I0467`、`E0901`、`E0902`（退役不复用）。**它们在门眼里就是空位** —— 下一个
扫登记表找空号的人会拿走其中一个，①-④ 全部放行。这与 E0474 / E0477 / E0481 三次撞码是同一形状
（「占号能绕开登记表」），只是换了个绕法。`error-codes.md` 同时自相矛盾：它一边写「`DiagnosticCodes.z42`
**这是唯一能占号的地方**」，一边自己在两节里占了 4 个号。

- [x] 7.1 登记表补 **4 个零发射点占号常量**：`StructSelfReference`(E0438) 就地补在 E0437/E0439 之间；
      新增「已退役的编号」小节放 `RetiredForwardSkipped`(I0467) / `RetiredUnknownNativeName`(E0901) /
      `RetiredNativeArityMismatch`(E0902)。**不加发射点** ⇒ 不触 core→semantics 冷启动 stale-cache
      （规则 ② 只管「发射的必须登记」，反向不管）。
- [x] 7.2 **`E0467` 按裁决放回可用空号池**（不保留）：前缀不同即不同码，`E0466` 与 `I0466` 并存即先例，
      退役的是 `I` 前缀的 0467。文档两处说法（「未使用」vs「不复用」）就此统一。
- [x] 7.3 门加**规则 ⑤**：`error-codes.md` 的码 token ↔ 登记表**双向相等**。左向拦「号被占在散文里」，
      右向拦「新码没进用户查得到的那张表」（「新增一个码」第 3 步此前纯靠自觉）。
- [x] 7.4 **码形态识别放宽到一位小写字母后缀**：`E0908a` / `E0908b` 是登记表里的真常量，长度 6，
      此前谓词只认长度 5 ⇒ 这两个常量整个在门的视野外（不受 ①② 约束，还占着 0908 号段而门看不见）。
- [x] 7.5 判别力注入验证（🔴 一律 `cp` 备份还原，`git checkout <file>` 会连自己的改动一起抹掉）：
      A 删掉 E0438 常量而文档留着 → ⑤ 红；B 登记表加 E0490 而文档不提 → ⑤ 红；
      C 文档里预留 E0491 而无人登记 → ⑤ 红；**D 后来者把退役的 E0901 当空位拿走 → ① 红**
      （D 正是本阶段的收益闭环：补常量之前，拿走 E0901 是全绿的）。四组还原后复跑全绿。
- [x] 7.6 文档：`error-codes.md`（删掉写死的「123 个」——#762 加了 E0483 后就没跟上，写死的数字必然腐坏；
      保留编号小节加「保留 ≠ 只写在这里」；I0467 三处补占号常量名 + 讲清前缀不牵连；第 3 步标注「不是可选的」）
      + `test-gate.md`（四条 → 五条、回放表加一行、⑤ 的由来与后缀放宽的副作用）。

**Deferred 状态更新**：`migrate-diag-literals-to-constants` **仍被卡住** —— 解除条件是 #759 的 10 个
新常量随 nightly 进 z42c.core，实测 `nightly` 的 targetCommitish 是 `18a64904e`(#756)，
`git merge-base --is-ancestor b74bee130 18a64904e` 判 NO。等下一个 nightly。
