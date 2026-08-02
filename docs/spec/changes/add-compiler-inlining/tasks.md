# Tasks: 编译期函数内联 + 可独立开关的 OptSet

> 状态：🟢 Phase 1 完成（本 PR #100 = OptSet 门控 + CLI 配置，内联**地基**）；
> **Phase 2（函数内联 pass）拆为后续独立 change**（IR 逐指令寄存器重映射 + 块拼接 + 自举不动点，
> 规模较大，值得专注实现）。 | 类型：feat（compiler，优化门控 + 配置面） | 创建：2026-08-01
> proposal / design / spec 见同目录（含 Phase 2 完整设计，供后续 change 复用）。

## 进度概览
- [x] 阶段 1a: OptSet 门控（逐 pass 门控 + profile 默认；debug=None/-O0）—— 本地 + CI #100 绿
- [x] 阶段 1b: CLI `--opt`/`--no-opt`（toml `[optimize]` **deferred**：走 z42.project stdlib API，
      受两-nightly 纪律，待该 API 随 nightly 发布再消费）
- [ ] 阶段 2: 函数内联 pass（`Inline` 优化）
- [ ] 阶段 3: 验证 + 文档 + 归档

> **阶段 1 实测**（本地）：`z42c build --opt frobnicate` → 报错退出；`--opt inline`/`--no-opt const-fold`
> → 解析接受；debug(-O0) vs release(All) 同程序 zpkg **735B vs 413B 字节不同**（优化确实改 codegen），
> 二者运行输出同为 `5`（语义不变）。codegen golden 64/64 绿（dump 路径保持 Opt.All）。

## 阶段 1: OptSet 门控（commit 1，低风险）
- [ ] 1.1 `OptSet.z42`（NEW）：`Opt` 位常量(ConstFold/CopyProp/Dce/Algebraic/Inline/None/All)+ `Has` + `Resolve(profileIsRelease, tomlBits, cliAdd, cliRemove)`
- [ ] 1.2 `Main.z42`：解析 CLI `--opt <csv>` / `--no-opt <csv>`（未知名报错；`all` 支持）
- [ ] 1.3 ProjectManifest：解析 toml `[optimize]` 逐项 bool
- [ ] 1.4 `Z42cCompiler.z42` / `PackageCompile.z42`：解析 + 透传 optSet 到 IrGen
- [ ] 1.5 `IrGen.Generate(cu, model, optSet)` + `IrDump` 3 处调用（dump 默认 None）
- [ ] 1.6 `IrOptPipeline.Run(irm, optSet)`：逐 pass `if Has(optSet, X)`
- [ ] 1.7 配置解析单测(优先级/加减/未知名报错) + **独立性单测**(每 pass 单独开跑 golden 正确)
- [ ] 1.8 `cargo build` + `xtask test e2e`（debug=None、release=All 现状不变，逐字节一致）

## 阶段 2: 函数内联 pass —— **拆为后续独立 change**（设计已在 design.md D4/D5/D7）
> 规模评估（2026-08-02）：z42 IR **无统一「重映射一条指令的寄存器」操作**，每种指令（~40 种）各有
> 自己的 `TypedReg` 字段 → 内联要逐指令类型克隆+重命名。v1 可裁 curated 指令集（const/arith/cmp/
> logical/copy/fieldget/ret，遇不支持指令跳过该 callee）控规模。这是一次专注实现，故从本 PR 拆出。
> `Inline` 优化位（OptSet）已就位，Phase 2 change 只需实现 `IrInline.z42` + 在 IrOptPipeline 接线。
- [ ] 2.1–2.7 → 后续 change `add-compiler-inlining-pass`（或延用本名的 follow-up）

## 阶段 3: 验证 + 文档 + 归档
- [ ] 3.1 `cargo build` + `xtask test`（e2e / cross-zpkg / stdlib / compiler 自举）全绿
- [ ] 3.2 自举 self-host soak（D7：引入当次多跑一代收敛；pair-gen 兜底）
- [ ] 3.3 内联收益实测（A/B：`--no-opt inline` vs 全开，调用密集 bench）
- [ ] 3.4 `docs/book/src/compiler/optimization-pipeline.md`：OptSet + 独立性约束 + 内联机制/资格/不动点
- [ ] 3.5 构建配置文档：`[optimize]` + `--opt`/`--no-opt`
- [ ] 3.6 归档 + commit + PR

## 备注
- **独立性(design D2)是硬约束**：每个 pass 单独开都必须正确 → 单测逐 pass 单独跑 golden 兜住;未来
  新增优化 pass 一律加一个 OptSet 具名开关 + 独立性单测。
- **自举不动点(D7)**：内联纯优化不改语法/格式;引入当次 soak 破一代自愈,不阻塞发布链。
- v1 保守;跨包内联、单态 vcall 内联、放宽阻断特征 → 后续 spec。
