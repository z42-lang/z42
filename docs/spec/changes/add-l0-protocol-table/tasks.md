# Tasks: L0 协议表（批 1，C 档）

> 前置：[proposal.md](proposal.md) ｜ [design.md](design.md)（D-new-1 = **形状 乙**）
> **交付形态：一条 PR 全做**（2026-09-25 User 裁）。理由：不留「表建了没人用 / 名字收敛了特性门还是死的」中间态。
> 代价自认：review 里「纯收敛」与「行为变更」混在一起 ⇒ **T1/T2 必须各自单独做一次字节对账**
> （见 V1），不能只在 PR 末尾对一次总账。

## 已核实的落位事实（2026-09-25，本 worktree）

| # | 事实 | 位置 |
|---|---|---|
| F1 | `z42c.core` 在 `src/libraries/z42c.core/`（**不在** `src/compiler/`），6 个文件，`namespace Z42.Core` | `src/libraries/z42c.core/src/` |
| F2 | 生产代码里 `new Parser(...)` **只有 2 处**，都在 `IncrementalDriver.z42`（`:56` 批量、`:428` 单文件） | `src/compiler/z42c.driver/src/IncrementalDriver.z42` |
| F3 | `z42c.driver` 同时依赖 `z42c.core` 与 `z42.project` ⇒ **中性 name/value → LanguageFeatures 的那次转换落在 driver** | `z42c.driver.z42.toml:11-21` |
| F4 | `LanguageFeatures` 调用方仍只有它自己的单测（`z42c.core/tests/features.z42`） | 全仓 grep |
| F5 | `ProjectManifest` 的 `Optimize*`/`Lint*` 都是「公字段 + ManifestLoader 构造后填」 | `ProjectManifest.z42:35,46,79-82` |
| F6 | `CompileInput.OptSet` 是「driver 设 → pipeline 读」的既有通道；`cin.OptSet = optSet` 在 `Main.z42:444` | `PackageCompile.z42:46-48,93,259` |
| F7 | `MemberParser` 已 `using Z42.Core` ⇒ 引用 core 常量零新依赖 | `MemberParser.z42:5` |

## T1 —— `ProtocolNames`（`z42c.core`）+ foreach 侧改引用【纯收敛，字节必须不变】

- [ ] 新增 `src/libraries/z42c.core/src/ProtocolNames.z42`：static class + 串常量。抬头写明
      **它是 SoT、以及为什么名字在 core（L1/L2）**，并写「加名字前先确认真有消费方」（照
      `LanguageFeatures` 抬头那条的口径 —— 那张表当年就是宣告了 6 个不存在的东西）。
- [ ] 收哪些：foreach 枚举器面（`GetEnumerator` / `MoveNext` / `Current` / `Dispose`）、
      索引面（`get_Item` / `Count` / `Length`）、访问器前缀（`get_`）。
      **不收**纯类型名（D7）。
- [ ] `ForeachProtocol.z42` / `StmtBinder.z42` 全部字面量改引用。
- [ ] 🔴 `get_` 前缀在 `CountMember` 里是**拼接**（`"get_" + srcName`）——常量要给的是前缀本身，
      别顺手改成「存一串完整的 `get_Count`/`get_Length`」，那会把两档逻辑复制成两份。

## T2 —— `ForeachProtocol.Resolve` + `ForeachPlan`【收敛，字节必须不变】

- [ ] 新增 `ForeachPlan`（哪条路径 + 元素类型来源 + 计数成员串 + `Disposable` 标志）。
- [ ] `ForeachProtocol.Resolve(Z42Type)`：把 `StmtBinder._bindForeach` 的路径选择原样搬进来，
      **顺序在函数体里一眼可见**（数组 → 索引面 → 枚举器）。
- [ ] `StmtBinder` 改为「问一次 `Resolve`，按 plan 合成 AST」；E0401 那条「needs an integer
      indexer plus a `Count`/`Length`, or a `GetEnumerator()`」的消息与 span 不变。
- [ ] 判据函数（`IsIntIndexer` / `CountMember` / `CountOf` / `EnumeratorDisposable`）**原地不动，
      注释不搬**（D-new-1 的核心取舍就是保住这些事故注释）。
- [ ] 🔴 `Resolve` 不得二次绑定 —— `EnumeratorDisposable` 的签名约定是「调用方已取好成员面」，
      搬家时别让它变成「自己再取一次」（那会在接口静态类型上取到另一张表）。
- [ ] 🔴 **元素类型推导（`var` 那一支）留在 `StmtBinder`，不进 `Resolve`。** 它今天只处理
      `Z42ClassType`（`Z42InstantiatedType` 如 `List<int>` 落不进去、`feElem` 停在 Unknown），
      **比协议成员面窄**。挪进 `Resolve` 顺手「修正」这个窄口 = 行为变更 = 字节变，
      本批必须字节不变 ⇒ 原样留在原地，另起一条记账。
- [ ] 🔴 **判据问的是「哪张继承面」，这一句不能在搬家中丢**（design L7 的实测）：类沿 `BaseName`
      链、接口交 `InterfaceClosure.FindMethod`，判据不足一律保守。`Resolve` 只排顺序，
      「问哪张面 + 拿不准往哪边倒」必须留在判据函数里。

## T3 —— event 降糖改引用常量【收敛，字节必须不变】

- [ ] `MemberParser.z42` 的 stdlib 具体名全部改引用 `ProtocolNames`：
      `IDisposable` ×2、`Disposable`·`From`、`InvalidOperationException`、`Subscribe`/`Unsubscribe`、
      `DelegateOps`·`ReferenceEquals`、`add_`/`remove_` 前缀、`MulticastAction`/`MulticastFunc`/
      `MulticastPredicate`、`Action`/`Func`/`Predicate`。
- [ ] ⚠️ 旧稿漏掉、这次一起搬的两处：`:20-22` handler 委托名映射（`MulticastFunc`→`Func`）、
      `:239` 语法层替用户合成 `new MulticastXxx<>()`。
- [ ] 🔴 `"\"single-cast event already bound\""` 存的是**带引号的原始 token 文本**
      （`StringLitExpr` 约定）。要么连引号一起存，要么在使用点加回 —— 弄错会让合成的 AST 变形，
      而这形状的测试很可能只验「有没有抛异常」、不验消息，**门不会红**。
- [ ] AST 合成动作**留在语法层**（D-new-3）：本批消掉的是「语法层**知道**类叫什么」，不是搬阶段。

## T4 —— `[syntax]` 特性门接线【唯一有行为变更的一段】

- [ ] `ProjectManifest`：加 `SyntaxNames: string[]` / `SyntaxValues: bool[]` / `SyntaxCount: int`
      公字段 + 构造函数置空，照 F5 的形态（**不撑大已 20 参的构造函数**）。
- [ ] `ManifestLoader`：解析 `[syntax]` 段填上述字段。只搬运，不解释。
- [ ] `Main._build`（driver）：**唯一一次** resolve —— 从 profile 起底 + 按 manifest 覆盖，
      产 `LanguageFeatures`；未知特性名**报错、不静默忽略**（同 `Opt.ByName` 口径）。
      🔴 workspace 每个成员有自己的 manifest ⇒ resolve 必须落在 manifest 在手的那一层，
      **不在 CLI 层预先算**。
- [ ] `CompileInput` 携带 → `IncrementalDriver` 的 **2 处** `new Parser(...)`（F2）都要传。
      ⚠️ 只挂一处 = 另一条路径静默无门（`parallel-pass-sequences-miss-new-hook`）。
- [ ] `Parser` 持 `LanguageFeatures`（可空 = 全开，保住 F4 里那些测试与 REPL 等直接 new 的调用方）
      + 一个 `_requireFeature(name, span)` 发 **E0301**。
- [ ] 挂到**至少一个真语法构造**上（`control_flow` 是最直的：关掉 ⇒ `foreach`/`while` 报 E0301）。
- [ ] 🔴 **旋钮效果进 cache key**：`depsId` 折进 syntax 集，否则「全量生效、增量被忽略」
      （`[optimize]` 实测踩过这颗雷）。
- [ ] ⚠️ **可能跨一个 nightly**：给 `ProjectManifest` 加公字段并被 `z42c.driver` 读 = 新跨成员符号。
      判据 = `xtask test bootstrap`。**先跑判据再决定拆不拆**，不预先假设要拆（#819 那轮没踩到）。
      真要拆：support（manifest 加字段，无消费者）→ 晚一个 nightly → use（driver 读）；
      这会让「一条 PR 全做」退化成两条，那时回来找 User 确认。

## T5 —— 文档

- [ ] `docs/internals/src/compiler/syntax-customization.md`：
      - 订正「实施路径」的**三处自相矛盾**（正文「分三批」/ 表列 4 批 / 后文「批 1–3 落地后」）；
      - 写明 `[syntax]` 的**粒度是整个语法构造、裁不到协议内的某一步**，以及将来要裁需要怎么做
        （给 `Resolve` 的步骤挂 feature 名）。
- [ ] `docs/reference/`：`z42.toml` 的 `[syntax]` 段（字段、未知名报错、可用特性名清单）。
- [ ] `docs/reference/src/.../error-codes.md`：E0301 删掉「⚠️ 零发射点」标记，补发射条件。
- [ ] `docs/internals/`：`ProtocolNames` 是 SoT + 「判定在 semantics、名字在 core」的分层理由
      （L1/L2），以及 `ForeachPlan`/`Resolve` 这条链的顺序语义。
- [ ] `LanguageFeatures` 抬头那段「本表当前零调用方 / 关不掉任何东西」**必须改**——接线后它就成假话。

## V —— 验收（判别力）

- [ ] **V1 字节对账（分两次）**：T1+T2+T3 落完各自 / 合并后，`xtask` 自举 3/3 不动点 + golden 全绿
      + 产物字节不变。收敛名字这件事字节一变就不是收敛。
- [ ] **V2 收敛有效**：改 `ProtocolNames` 里一个名字 ⇒ 所有客户一起变（若只有一处变，说明还有第二份）。
      具体做法：临时把 `GetEnumerator` 改成一个不存在的名字，**期望 foreach 整体失效**；
      只有一两个测试红 = 收敛没做全。
- [ ] **V3 特性门双向**：`control_flow = false` ⇒ 用 `foreach` **必须**报 E0301（负例）；
      不写 `[syntax]` ⇒ 一切照旧（正对照）。只写「开着能解析」等于没测门。
- [ ] **V4 未知特性名**：`[syntax] no_such_thing = false` ⇒ 报错退出，不静默。
- [ ] **V5 增量**：改 `[syntax]` ⇒ 缓存必须失效（取**产物字节**判据，不取日志文字）。
      🔴 判别力来自夹具 —— 夹具里必须真有会被该特性门挡住的语法（`[optimize]` 那轮第一版门
      复用了一个没有可内联调用的夹具，翻旋钮什么都不变，门零判别力）。
- [ ] **V6 语法层不再出现 stdlib 具体类名**：可加 grep 门；**判别力靠注入假名验证**
      （注入后门必须红，否则是假门）。
- [ ] **V7 foreach 三路径回归**：数组 / `String` / `List<T>`（索引面）/ `Dictionary`（枚举器面）
      / `foreach_dispose_optional`（条件步）全绿。
- [ ] **V8 examples 门**：改了编译器可见行为 ⇒ `build sdk` → `test examples`
      （`local-green-misses-examples-gate`：只跑 `build compiler` 测的是旧编译器）。

## 顺序

T1 → T2 → T3（三次收敛，每次对账）→ T4（行为变更）→ T5（文档）→ V 全量。
T4 的 bootstrap 判据**尽早跑**（加完 `ProjectManifest` 字段就跑一次），别等到最后才发现要跨 nightly。
