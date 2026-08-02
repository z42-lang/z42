# Tasks: 编译期函数内联 + 可独立开关的 OptSet

> 状态：🟢 Phase 1 完成（本 PR #100 = OptSet 门控 + CLI 配置，内联**地基**）；
> **Phase 2（函数内联 pass）拆为后续独立 change**（IR 逐指令寄存器重映射 + 块拼接 + 自举不动点，
> 规模较大，值得专注实现）。 | 类型：feat（compiler，优化门控 + 配置面） | 创建：2026-08-01
> proposal / design / spec 见同目录（含 Phase 2 完整设计，供后续 change 复用）。

## 进度概览
- [x] 阶段 1a: OptSet 门控（逐 pass 门控 + profile 默认；debug=None/-O0）—— 本地 + CI #100 绿
- [x] 阶段 1b: CLI `--opt`/`--no-opt`（toml `[optimize]` **deferred**：走 z42.project stdlib API，
      受两-nightly 纪律，待该 API 随 nightly 发布再消费）
- [x] 阶段 2: 函数内联 pass（`Inline` 优化）—— IrInline.z42 + IrOptPipeline 接线 + 内联单测（2026-08-02）
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

## 阶段 2: 函数内联 pass（IrInline，`Opt.Inline` 位）—— 2026-08-02 实现
- [x] 2.1 `IrInline.z42`（NEW，z42c.semantics）：`Run(m)` 模块级；`_inlineInto` 逐 caller 展开合格调用点
- [x] 2.2 资格 `_eligibleCallee`（D4）：直接 CallInstr / 同模块解析 / 非递归（Name≠）/ 单块+RetTerm /
      无异常表 / 无 varargs / 精确 arity / curated 体 / (instrCount≤24 或单调用点)
- [x] 2.3 curated 集 `_isInlinable` + 逐指令克隆 `_cloneRemap`（const*/copy/算术/比较/位·一元/convert/field_get，
      两者覆盖集**必须一致**）；非 curated → 跳过整个 callee
- [x] 2.4 展开（D5）：offset=caller.MaxReg → callee reg +offset；形参 `copy`；单块 body 克隆；
      Ret 有值→`copy call.Dst`；reg_types 同步扩 `_extendRegTypes`；稳定序（block/instr idx 顺扫）
- [x] 2.5 接线 `IrOptPipeline.Run`：`if Opt.Has(optSet, Opt.Inline) { IrInline.Run(m) }`（逐函数清理 pass 之前）
- [x] 2.6 dump/golden 路径排除内联：`IrDump._buildF` / `BuildModuleD` 用 `Opt.All - Opt.Inline`
      （= Phase 2 前等价输出，既有 golden 逐字节不变）；新增 `DumpFuncOpt`/`DumpModuleOpt` 供内联单测
- [x] 2.7 内联单测（codegen_tests.z42）：内联生效 / 独立性（仅 Opt.Inline）/ 形参绑定 / -O0 保留 call /
      递归拒绝 / vcall 跳过 / 非 curated callee 跳过

## 阶段 3: 验证 + 文档 + 归档
- [x] 3.1 `cargo build` + `xtask test` 全绿：`✅ GREEN — all stages（e2e + stdlib + compiler + vscode-syntax，
      C#-free）`；compiler 20/20 units（含 7 内联单测）、stdlib 25/25 build
- [x] 3.2 自举 self-host 不动点（D7）：`5/5 packages gen1==gen2 (--workspace, C#-free)`——内联确定性稳定序
      使字节不动点成立（稳态无需破代；引入当次已在多轮重建中收敛）
- [x] 3.3 内联收益实测（A/B，`heavy.z42` 调用密集 hot 循环 4M 迭代，interp best-of-3）：
      **OFF 918ms → ON 278ms（~3.3×，−70%）**，输出同为 `13336000000`（语义不变）；zpkg +9%（1028→1123B，
      内联=代码复制换 dispatch 消除，符合准则 1 interp-first）
- [x] 3.4 `docs/book/src/runtime/optimization-pipeline.md`：OptSet 门控 + 独立性约束 + 内联机制/资格(D4)/
      展开(D5)/传导内联/不动点(D7)（页在 runtime/ 非 compiler/，随 jit-lowering-pipeline 立项）
- [ ] 3.5 构建配置文档：`[optimize]`（toml 消费仍 deferred，见备注）+ `--opt`/`--no-opt`（Phase 1 已文档化）
- [ ] 3.6 归档 + commit + PR

## 备注
- **独立性(design D2)是硬约束**：每个 pass 单独开都必须正确 → 单测逐 pass 单独跑 golden 兜住;未来
  新增优化 pass 一律加一个 OptSet 具名开关 + 独立性单测。
- **自举不动点(D7)**：内联纯优化不改语法/格式;引入当次 soak 破一代自愈,不阻塞发布链。
- v1 保守;跨包内联、单态 vcall 内联、放宽阻断特征 → 后续 spec。
