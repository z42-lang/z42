# Tasks: Attribute 与编译期 Handler 体系

> 设计 SoT：[design.md](design.md)。多 PR 阶梯，每 PR 单独分支 + GREEN + 合并（parallel-development）。

## 进度概览

| PR | 内容 | bump | 状态 |
|----|------|:---:|------|
| PR1a | HandlerRegistry AST-phase：AttributeSynth+BenchmarkDesugar 收敛 + 三路 kind 判定（无后缀，byte-identical）+ DeclId 概念 | 否 | ⬜ 进行中 |
| PR1b | HandlerRegistry IR-phase：TestIndexBuilder+StubEmitter 收敛（`[Native]` 不改） | 否 | ⬜ |
| PR2 | 后缀约定（D8）：resolution 剥离 + 强制校验 + 迁移现有 attribute 类 + 反转 `Attribute.z42`/`basic.z42` 头注 | 否 | ⬜ |
| PR3 | Analyzer 契约 + `AnalysisContext` + 诊断/severity + `[lints]` + `#suppress`/`[Suppress]` | 否 | ⬜ |
| PR4 | Generator/ModuleGenerator 对外加载（`packages.toml` 依赖 + 反射发现接口）+ splice/merge（Replace/Augment） | 否 | ⬜ |
| PR5 | `[Deprecated]` directive（D2，持久化 flag+msg，跨包+IDE） | 是 | ⬜ |
| PR6 | caller 编译期宏（D3） | 是(param) | ⬜ |
| PR7 | `--fix` 统一分析+修复（build 期 splice） | 否 | ⬜ |
| 后续 | `[Native]`→`[Extern]` 改名 / `[Layout]`/`[Repr]`(E2) / `OnIrOp` perf lint / 用户 `macro` / 局部变量 attribute | 视需 | ⬜ Deferred |

## PR1a · HandlerRegistry AST-phase + 三路 kind 判定（当前）

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

- [ ] worktree 供种（`.z42`/`xtask`/重建 `xtask.zpkg`，见 [[fresh-worktree-seed-setup]]）。
- [ ] `xtask test` 全 stage gate 绿；self-host gen1==gen2 逐字节；5/5。
- [ ] zbc-format golden 零漂移；`[Setup]/[Teardown]/[Ignore]` 行为逐字节保持。

## 备注

- 每 PR 合并前并入 main 最新 + 重跑 GREEN（parallel-development §3）。
- **语义耦合自查**：并发 worktree `z42-record [add-record-attribute]`（record 用 attribute 式声明）、
  `z42-conv [add-user-conversions]`（动 semantics）与本 change 邻接——PR2（后缀迁移）/ PR3（analyzer 触
  semantics）开工前主动对表/知会（parallel-development §4）。
- **PR2 后缀约定是破坏性迁移**（改现有 attribute 类名 + 反转 `Attribute.z42`/`basic.z42` 已记录的"无后缀"
  决定），**非 byte-identical**；会破一代自举、warm 重建自愈（D7 式）。与纯重构 PR1a/1b 分开，不混。
- 带 bump 的 PR（PR5/6）走 bootstrap-seed 两阶段纪律，bump 前 `xtask test bootstrap`。
