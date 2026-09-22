# Proposal: 诊断码唯一性 —— 撞码归位 + 登记表成唯一咽口 + 一道会变红的门

> change: `enforce-diagnostic-code-uniqueness` ｜ scope: `compiler` + `stdlib` + `toolchain` + `docs` ｜ 无格式 bump
> 来源: [[primitive-silent-defects-program]] 推进过程中发现（开工前核基准时撞见）

## Why

**诊断码是用户可见契约**：用户拿到 `E0477` 会去 `docs/reference/src/appendix/error-codes.md` 查它
是什么意思。一码两义 ⇒ 查表得到的是**另一个诊断的解释**——比「查不到」更坏，因为它看起来是个答案。

开工时实测，main 上**已有两处一码两义**：

| 码 | 先来（保号） | 后到（撞上来） |
|---|---|---|
| **E0474** | 属性**混合** auto 与带体访问器（`MemberParser.z42:308`，#737 `87378dd0f`） | 值类型与 `null` 比较恒假/恒真（`TypeChecker.z42:657`，#741 `a4aed8247`） |
| **E0477** | 取重载自由函数引用、无重载匹配目标委托（`ExprTyper.Funcref.z42:39`，#745 `037b9bf9c`） | 赋值目标不是左值（`AssignTyper.z42:224`，#749 `76fb9c11b`） |

两处的形状完全一样，且**都不是粗心**——是机制缺陷：

1. **发码点绕开了登记表**。`DiagnosticCodes.z42` 有 113 个码常量，但语义层 / 语法层同时存在一条
   「用字面量直接发码」的惯例——42 个码、**104 个发射站点**走这条路。查实：这两层
   （`z42c.semantics` / `z42c.syntax`）**本来就依赖 `z42c.core`、也确实在别处用着
   `DiagnosticCodes.X`** ⇒ 字面量是**惯例，不是技术必需**。
2. **抢号不会产生任何冲突信号**。两个并行 PR 各自在自己的文件里写下 `"E0478"`，git 眼里是两处
   互不相干的新增 ⇒ **欢快地合并**。#752（已合）与 #747（在飞）此刻就各自持有一个 E0478。
3. **没有任何门盯着**。错误码全表靠人工 `grep` 重建（`error-codes.md` 第 14 行自述如此），
   E0474 那次撞码是**事后**被人手工标了个「⚠️（撞码）」了事；E0477 这次连标都没标——
   码表说它是「赋值非左值」，参考文档说它是「funcref 无匹配」，**两边互相矛盾且都不完整**。

正是本仓反复撞到的那条：**没有会变红的东西盯着的约定，迟早会烂**（[[audit-silent-gates-program]]）。

## What Changes

### ① 两处撞码按「先来后到」归位

先占号者保号，后到者搬家。两条都是 2026-09-22 当天合入、尚未进任何 nightly 之外的长期契约：

| 诊断 | 原 | 新 |
|---|---|---|
| 值类型与 `null` 比较 | E0474（撞） | **E0481** `ValueTypeNullComparison` |
| 赋值目标不是左值 | E0477（撞） | **E0482** `AssignTargetNotLvalue`（常量改值） |

### ② 登记表补全 —— 每个发射出去的码都必须在表里登记

> 🔴 **开工时校正过一次方案前提**：原计划是「禁止字面量发码、104 个站点全改常量引用」。
> 查源码坐实**那条惯例有技术理由**——`DeclBinder.z42:555` /`OverloadBinder.z42:186` 写明
> 「不引用**新** `DiagnosticCodes` 常量 → 避 core→semantics 冷启动 stale-cache」，
> `GeneratorDriver.z42:529` 则是走完两阶段的先例（PR-N 发字面量 → 常量随 nightly 载入后 PR-N+1 切回）。
> 这正是 [bootstrap-seed.md](../../../agent/rules/bootstrap-seed.md) 的**分阶段引入纪律**：
> **新常量与它的引用不能同 PR**。故「全改常量」今天落不了地，要跨一个 nightly（见 Deferred）。

好在 ① 的**真正价值是让登记表成为唯一咽口**，这不需要废除字面量就能拿到。改为补全登记表：
**发射得出去的码，必须在 `DiagnosticCodes.z42` 里登记**（发射点本身仍可用字面量）。

9 个「只有字面量、没有常量」的码补上常量：

| 码 | 新常量名 |
|---|---|
| E0470 | `RefArgNotAddressable` |
| E0471 | `ObsoleteParamModifier` |
| E0472 | `RefArgModifierMissing` |
| E0473 | `RefArgModifierUnexpected` |
| E0474 | `MixedPropertyAccessors` |
| E0475 | `NullableToNonNullableValue` |
| E0476 | `NullableValueType` |
| E0477 | `FuncRefNoMatchingOverload` |
| W0700 | `SwitchNotExhaustive` |

外加一处顺带的名实归位：常量 `ForwardSkipped` 登记的是 **I0467**，而真正的发射点发的是
**I0466**（`error-codes.md` 早已标注「I0467 ⚠️ 零发射点（跳过实际发的是 I0466）」）。
把常量改值到 I0466，**I0467 登记为 ❌ 已退役**（编号不复用）。

> **额外收益（这条才是长期价值）**：登记表成为**唯一咽口**后，两个并行 PR 抢同一个号会在
> `DiagnosticCodes.z42` 上产生**货真价实的 git 文本冲突** —— 正是 #752/#747 那次**缺席**的信号。
> 把「静默合并」变成「合并前必须有人做决定」。

### ③ 新 GREEN gate stage `xtask test diagcodes`

仿 `xtask test walkers` 的**活体对账**（硬门、无基线文件、无白名单）。三条规则：

1. **登记表内无重复码值** —— 同一个码值出现在两个常量上 = 红。
2. **每个发射出去的码都必须在登记表里登记** —— 扫 `src/**` 非 `tests/` 的 `.z42` 源（剥注释后），
   发射点的字面量码 / `DiagnosticCodes.X` 引用，只要在登记表里找不到 = 红。
3. **引用的常量名必须存在** —— `DiagnosticCodes.Typo` 这类笔误 = 红（防造出幽灵码）。

**①+② 合起来对历史两次撞码都有判别力**（这是设计判据，不是事后解释）：

| 历史事件 | 哪条规则会拦住 |
|---|---|
| #737 / #745 发**未登记**的字面量 E0474 / E0477 | ② 红 → 逼它们进登记表 |
| #741 / #749 给**已登记**的 E0474 / E0477 再挂一个含义 | 进表即撞值 → ① 红 |
| #752 / #747 并行各抢一个 E0478 | 两边都得改登记表 → **git 文本冲突** |

判别力按 [[fake-gate-lets-compiler-bug-into-main]] 的要求，**实施时注入假撞码验证它真会红再还原**。

### ④ 文档

- `docs/reference/src/appendix/error-codes.md`：两处撞码行拆开归位、加 E0481/E0482 两行、
  I0466/I0467 行修正；把「这张表怎么来的」从「人工 grep 重建」改写为「登记表是 SoT + 有门守着」。
- `docs/internals/src/devinfra/test-gate.md`：新 stage 进机器可读清单（有门对账，漏了会红）。

## 非目标

- **不动**其余 111 个码的号与含义（本 change 行为零变化，除两处撞码归位）。
- **不做**「⚠️ 已定义未接线」那三成码的清理——那是独立议题，且需要逐条判断该接线还是该退役。
- **不改** `#747`（在飞 PR，同样持有一个 E0478）。本门落地后它会**变红**，届时按门的提示换号——
  这正是门该起的作用。

## 风险

- 本 change **行为零变化**，除两处撞码归位（E0474→E0481 / E0477→E0482 的发射点与测试跟号）。
- 新常量加在 `z42c.core` 但**本 PR 内零引用**（发射点仍用字面量）⇒ 不触碰冷启动 stale-cache。
- 门是纯文本扫描（~1s），host-independent，与 `test walkers` 同族。

## Deferred

- **`migrate-diag-literals-to-constants`**（晚一个 nightly）：本 PR 新增的 10 个常量随 nightly
  载入 z42c.core 后，把 104 个字面量发射点切回常量引用（`GeneratorDriver` E0449 的既有手法），
  之后可把门加强到第 ④ 条「非 tests 源零字面量发码」。**不能提前**——同 PR 引用新常量会撞
  冷启动 stale-cache。
