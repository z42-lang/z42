# Tasks: 编译期函数内联 + 可独立开关的 OptSet

> 状态：🟡 待确认（6.5 gate） | 类型：lang/ir 边缘（新优化 pass + 配置面）走完整流程 | 创建：2026-08-01
> proposal / design / spec 见同目录。分两阶段两 commit：先 OptSet 门控(低风险)、再内联(触自举不动点)。

## 进度概览
- [ ] 阶段 1: OptSet 门控（config 面 + 逐 pass 门控；debug=None）
- [ ] 阶段 2: 函数内联 pass（`Inline` 优化）
- [ ] 阶段 3: 验证 + 文档 + 归档

## 阶段 1: OptSet 门控（commit 1，低风险）
- [ ] 1.1 `OptSet.z42`（NEW）：`Opt` 位常量(ConstFold/CopyProp/Dce/Algebraic/Inline/None/All)+ `Has` + `Resolve(profileIsRelease, tomlBits, cliAdd, cliRemove)`
- [ ] 1.2 `Main.z42`：解析 CLI `--opt <csv>` / `--no-opt <csv>`（未知名报错；`all` 支持）
- [ ] 1.3 ProjectManifest：解析 toml `[optimize]` 逐项 bool
- [ ] 1.4 `Z42cCompiler.z42` / `PackageCompile.z42`：解析 + 透传 optSet 到 IrGen
- [ ] 1.5 `IrGen.Generate(cu, model, optSet)` + `IrDump` 3 处调用（dump 默认 None）
- [ ] 1.6 `IrOptPipeline.Run(irm, optSet)`：逐 pass `if Has(optSet, X)`
- [ ] 1.7 配置解析单测(优先级/加减/未知名报错) + **独立性单测**(每 pass 单独开跑 golden 正确)
- [ ] 1.8 `cargo build` + `xtask test e2e`（debug=None、release=All 现状不变，逐字节一致）

## 阶段 2: 函数内联 pass（commit 2）
- [ ] 2.1 `IrInline.z42`（NEW）：资格判定(直接调用/同模块/非递归/小或单调用点/无阻断特征)
- [ ] 2.2 展开:寄存器重命名 + reg_types 扩展 + 参数绑定 + 单块/多块拼接 + 稳定序
- [ ] 2.3 预算 + 内联深度上限
- [ ] 2.4 line table 映射 callee 源行
- [ ] 2.5 `IrOptPipeline`：`Has(optSet, Inline)` 时调 `IrInline.Run`（顺序靠前）
- [ ] 2.6 内联单测(小函数/单调用点/递归拒绝/VCall·跨包·异常表·ref-out 跳过/reg_types 保留/**Inline 单独开逐字节一致**)
- [ ] 2.7 golden：内联前后执行逐字节一致(interp+jit)

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
