# Tasks: Attribute 与编译期 Handler 体系

> 设计 SoT：[design.md](design.md)。多 PR 阶梯，每 PR 单独分支 + GREEN + 合并（parallel-development）。

## 进度概览

| PR | 内容 | bump | 状态 |
|----|------|:---:|------|
| PR1a | HandlerRegistry AST-phase：AttributeSynth+BenchmarkDesugar 收敛 + 三路 kind 判定（无后缀，byte-identical）+ DeclId 概念 | 否 | ✅ 完成 |
| PR1b | HandlerRegistry IR-phase：TestIndexBuilder+StubEmitter 名字识别收敛 + KindOf 细化三路（`[Native]` 不改，byte-identical） | 否 | ✅ 完成 |
| PR2 | 后缀约定（D8）：resolution 剥离 + 强制校验 + 迁移现有 attribute 类 + 反转 `Attribute.z42`/`basic.z42` 头注 | 否 | ⬜ |
| PR3 | Analyzer 契约 + `AnalysisContext` + 诊断/severity + `[lints]` + `#suppress`/`[Suppress]` | 否 | ⬜ |
| PR4 | Generator/ModuleGenerator 对外加载（`packages.toml` 依赖 + 反射发现接口）+ splice/merge（Replace/Augment） | 否 | ⬜ |
| PR5 | `[Deprecated]` directive（D2，持久化 flag+msg，跨包+IDE） | 是 | ⬜ |
| PR6 | caller 编译期宏（D3） | 是(param) | ⬜ |
| PR7 | `--fix` 统一分析+修复（build 期 splice） | 否 | ⬜ |
| 后续 | `[Native]`→`[Extern]` 改名 / `[Layout]`/`[Repr]`(E2) / `OnIrOp` perf lint / 用户 `macro` / 局部变量 attribute | 视需 | ⬜ Deferred |

## PR1b · HandlerRegistry IR-phase 名字识别收敛 + KindOf 细化三路（当前）

**目标**：把 IR 级两个子构建器（`TestIndexBuilder` / `StubEmitter`）的 attribute **名字识别**上移到
`HandlerRegistry`——emit **逻辑不变**（design §Implementation Notes「逻辑不变，识别改注册表」），
且把 PR1a 的「Directive 一锅端」`KindOf` 细化成真三路。**零新语法、零 bump、byte-identical**。

### 实施

- [x] `HandlerRegistry.KindOf` 细化三路：`Native`→`Directive`；test 非-store-meta 家族
      (`Test/Benchmark/Skip/ShouldThrow/Timeout`)→`Handler`；其余（含 `Setup/Teardown/Ignore`）→`StoreMeta`。
      **store-meta 判定逐字节保持 PR1a**（非-store-meta 集 = directive ∪ 非-SM-test = 那 6 名）。
      返回值目前只被 `AttributeSynth` 以 `== StoreMeta` 消费 → Directive/Handler 区分为文档性、无行为影响。
- [x] 新增注册表查询：`IsNativeDirective(name)`（StubEmitter 用）+ `IsTestHandlerAttr(name)`（8 名触发集，
      TestIndexBuilder 用）。
- [x] `StubEmitter`：两处 `at.Name == "Native"` → `HandlerRegistry.IsNativeDirective(at.Name)`。
- [x] `TestIndexBuilder`：删私有 `_isTestAttrName`（8 名）→ `_hasTestAttr` 改问 `HandlerRegistry.IsTestHandlerAttr`。
- [x] **BenchmarkDesugar 的 `== "Benchmark"` 自触发保留**（applied generator 类名剥后缀即触发，intrinsic；
      PR1a 已把它收进 `RunAst`，其 trigger 不属本 PR 收敛面）。

### 关键设计点（byte-identical 陷阱，PR1a 起沿用）

- **两个 test 名集不同、且必须不同**：`IsTestHandlerAttr` = 全 8 名（TestIndexBuilder 触发集，含
  Setup/Teardown/Ignore）；`KindOf` 的 Handler 子集 = 5 名（非-store-meta）。差集 {Setup/Teardown/Ignore}
  现仍走 store-meta 工厂——对齐两集会破自举字节不动点（PR1a 陷阱）。注册表用两个独立函数显式建模。
- **TestIndexBuilder 现状=handler 形，终态=store-meta**：design 记 TIDX 终态是 store-meta+反射发现、
  TIDX 退休（独立后续变更）；本 PR 标注的是**现状机制**（eager 聚合表 = module-generator 形）→ 归 Handler。

### GREEN（全绿）

- [x] worktree 供种（`.z42`/`xtask`/`xtask.zpkg` 沿用 PR1a 种子，无 bump）。
- [x] `xtask test all` 全 stage gate 绿；self-host gen1==gen2 逐字节 **5/5**；z42c `[Test]` 24 units via TIDX 全过（直接覆盖 TestIndexBuilder 路径）。
- [x] `xtask test incremental` incr==full 逐字节（demo 5/5 + xtask 50/50 whole-dist byte-identical）。
- [x] zbc-format golden 零漂移；Setup/Teardown/Ignore + Native 行为逐字节保持。

## PR1a · HandlerRegistry AST-phase + 三路 kind 判定（已完成）

**目标**：纯内部重构——把 AST 级两个 pass 收敛进 `HandlerRegistry`，用**三路 kind 判定**替换 `_isUserAttr`
名字白名单，**零新语法、零 bump、外部可见行为逐字节不变** → self-host gen1==gen2、5/5 不动点保持。

### 阶段 0 · 勘察（已完成）

- [x] 4 pass 分两层已确认：AST 级 `AttributeSynth.Run(BenchmarkDesugar.Run(raw))`（`IncrementalDriver.z42:52`
      / `IrDump.z42:82`）；IR 级 `TestIndexBuilder`/`StubEmitter` 为 IrGen 子构建器（`IrGen.Generate` 内）。
- [x] `_isUserAttr`（`AttributeSynth.z42:120-124`）唯一消费点 `AttributeSynth.z42:101`。
- [x] attr→IrAttrRef→zpkg 数据流；分流点 = `Attr.FactoryFunc`。
- [x] **字节不动点陷阱**：`_isUserAttr` 黑名单缺 `Setup/Teardown/Ignore`（它们现会被合成工厂）。

### 阶段 1 · HandlerRegistry 脚手架（AST-phase）

- [ ] 定义 `HandlerRegistry`，AST-phase 入口签名保持 `CompilationUnit → CompilationUnit`（与现有两 pass 同构）。
- [ ] 实现三路 kind 判定：directive 注册表（canonical）→ 实现 handler 接口 → else store-meta。
      **PR1a 阶段先只落 store-meta 分支的判定**（directive/handler 分支在 PR1b/PR4 接入），且 store-meta 判定
      **逐字节复刻 `_isUserAttr`**（含 Setup/Teardown/Ignore 走工厂的现状），不得对齐 `_isTestAttrName`。
- [ ] 奠定 `DeclId` 概念（PR1a 用不到 Replace/Augment，仅预留寻址）。

### 阶段 2 · AST 级两 pass 收敛

- [ ] `AttributeSynth`（用户 attr→工厂）→ registry 的 store-meta 默认路径。
- [ ] `BenchmarkDesugar` → registry 编排的内建 Generator（AST-phase 变换，逻辑不变）。
- [ ] 把 `IncrementalDriver.z42:52` + `IrDump.z42:82` 的 `AttributeSynth.Run(BenchmarkDesugar.Run(raw))`
      换成 `HandlerRegistry.RunAst(raw)`（两处必须同步改）。
- [ ] 删 `_isUserAttr`（消费点已改走 registry）。

### 阶段 3 · GREEN + 自举不动点

- [x] worktree 供种（`.z42`/`xtask`/重建 `xtask.zpkg`，见 [[fresh-worktree-seed-setup]]）。
- [x] `xtask test all` 全 stage gate 绿（e2e goldens + stdlib + compiler + vscode-syntax）；self-host gen1==gen2 逐字节 **5/5**。
- [x] `xtask test incremental` incr==full 逐字节（demo + xtask，55/55 files byte-identical）——覆盖 `IncrementalDriver.ParseAllTk` 改动路径。
- [x] zbc-format golden 零漂移；`[Setup]/[Teardown]/[Ignore]` 行为逐字节保持（store-meta 判定复刻 6 名 `_isUserAttr` false-集，未对齐 `_isTestAttrName`）。

### PR1a 实测勘察修正（写码时发现，与阶段 0 DRAFT 出入）

- **挂载点是 4 处，非 DRAFT 说的 2 处**：`AttributeSynth.Run(BenchmarkDesugar.Run(...))` 除
  `IncrementalDriver:52` / `IrDump:82` 外，还有 `IrDump:558`（`BuildModuleD` 跨包路径）+
  `IrDump:667`（`_buildFOpt` 内联单测路径）。四处全切 `HandlerRegistry.RunAst`。
  （`bench_desugar_tests.z42:13` 直调 `BenchmarkDesugar.Run` 保留——单测隔离，函数仍 public。）
- **DeclId 非死代码**：`AttributeSynth._process` 实际 `new DeclId(keyPrefix)` 并用 `did.Key` 构工厂名
  （`did.Key == keyPrefix` → 逐字节一致）。奠定寻址概念且真被执行，不违反 philosophy 反 speculative。
- **`test incremental` 是必跑的额外 gate**：`test all` 不含它，而本 PR 动了增量路径 → 必须单独跑。

## 备注

- 每 PR 合并前并入 main 最新 + 重跑 GREEN（parallel-development §3）。
- **语义耦合自查**：并发 worktree `z42-record [add-record-attribute]`（record 用 attribute 式声明）、
  `z42-conv [add-user-conversions]`（动 semantics）与本 change 邻接——PR2（后缀迁移）/ PR3（analyzer 触
  semantics）开工前主动对表/知会（parallel-development §4）。
- **PR2 后缀约定是破坏性迁移**（改现有 attribute 类名 + 反转 `Attribute.z42`/`basic.z42` 已记录的"无后缀"
  决定），**非 byte-identical**；会破一代自举、warm 重建自愈（D7 式）。与纯重构 PR1a/1b 分开，不混。
- 带 bump 的 PR（PR5/6）走 bootstrap-seed 两阶段纪律，bump 前 `xtask test bootstrap`。
