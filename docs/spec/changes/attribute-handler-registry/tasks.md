# Tasks: Attribute 与编译期 Handler 体系

> 设计 SoT：[design.md](design.md)。多 PR 阶梯，每 PR 单独分支 + GREEN + 合并（parallel-development）。

## 进度概览

| PR | 内容 | bump | 状态 |
|----|------|:---:|------|
| PR1 | Handler 契约 + 注册表 + DeclId/merge + 4 pass 收敛（纯内部重构） | 否 | ⬜ 进行中 |
| PR2 | Analyzer 契约 + `AnalysisContext` + 诊断/severity + `[lints]` | 否 | ⬜ |
| PR3 | Generator 对外加载（`packages.toml` 依赖 + 反射发现 `[Generator]`） | 否 | ⬜ |
| PR4 | `attribute` 声明 + usage 校验（D1） | 是 | ⬜ |
| PR5 | `deprecated`（D2，持久化版） | 是 | ⬜ |
| PR6 | `extern` 取代 `[Native]`（E1 归位） | 否 | ⬜ |
| PR7 | caller 编译期宏（D3） | 是(param) | ⬜ |
| PR8 | `--fix` 统一分析+修复（build 期 splice） | 否 | ⬜ |
| 后续 | `layout`(E2) / IR 层性能 lint / 用户 `macro` 开放 | 视需 | ⬜ Deferred |

## PR1 · Handler 契约 + 注册表 + 4 pass 收敛（当前）

**目标**：纯内部重构——把现有 4 个 ad-hoc pass 收敛进一个 `HandlerRegistry` + 统一契约，**零新语法、零 bump、
外部可见行为不变** → self-host gen1==gen2 逐字节、5/5 不动点保持。这是全 change 的地基，先把确定性/自举风险压住。

### 阶段 0 · 勘察（先摸清实际源码，再动手）

- [ ] 定位 `AttributeSynth` / `StubEmitter` / `TestIndexBuilder` / `BenchmarkDesugar` 在 z42c 的确切文件与
      pipeline 调用点（`z42c.pipeline` / `z42c.semantics` / `IrGen`）。
- [ ] 摸清 `_isUserAttr` 名字白名单当前的所有消费点。
- [ ] 摸清 attribute 从 AST（`Attr`/`AttributedDecl`）→ `IrAttrRef` → zpkg 的现有数据流。
- [ ] 确认 pipeline 里 pass 的调度方式（顺序、注册点），决定 `HandlerRegistry` 的挂载位置。

### 阶段 1 · HandlerRegistry 脚手架

- [ ] 定义 `Handler` 基契约 + `TriggerSpec`（内部版，先不暴露 stdlib 接口）。
- [ ] `HandlerRegistry`：注解名/命名空间 → 内建 handler 实例；按 handler 稳定 Id 排序（gap1）。
- [ ] 在 pipeline 挂载分派点（parse 后 / bind 后 / IrGen），替换现有"按名字 if-else 分流"。

### 阶段 2 · 4 pass 逐一收敛（行为逐字节不变）

- [ ] `AttributeSynth`（用户 attr→工厂）→ 默认 store-meta 路径（保持不变，只是接入注册表默认分支）。
- [ ] `StubEmitter`（`[Native]`）→ 内建 directive handler（识别从"名字==Native"改成注册表项；**codegen 逻辑一字不改**）。
- [ ] `BenchmarkDesugar` → 内建 Generator handler（包裹逻辑不变）。
- [ ] `TestIndexBuilder`（`[Test]` 系）→ 内建 handler；**TIDX 段退休改到 PR2/独立评估**（PR1 先保持 TIDX 产出
      不变，只把"识别"接进注册表，避免一次动太多触发门禁）。
- [ ] 删除 `_isUserAttr` 名字白名单（全部消费点已改走注册表）。

### 阶段 3 · GREEN + 自举不动点

- [ ] `xtask test` 全 stage gate 绿。
- [ ] self-host gen1==gen2 逐字节；5/5 不动点。
- [ ] 确认零 zbc/zpkg 格式变化（`git diff` zbc-format golden 无漂移）。

> **DeclId/merge 模型**：PR1 只需奠定 `DeclId` 概念（4 pass 收敛用不到 Replace/Augment）；完整 merge/splice
> 实现随 PR3（Generator 对外加载）落地。PR1 把契约接口预留好即可。

## 备注

- 每 PR 合并前并入 main 最新 + 重跑 GREEN（parallel-development §3）。
- 语义耦合自查：`z42-record [add-record-attribute]` / `z42-conv [add-user-conversions]` 两条 in-flight 与本
  change 邻接（record 用 attribute 式声明、转换体系动 semantics）——开工 PR4（`attribute` 声明）前主动知会/对表。
- 带 bump 的 PR（PR4/5/7）走 bootstrap-seed 两阶段纪律，bump 前 `xtask test bootstrap`。
