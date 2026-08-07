# Tasks: 纯函数调用优化（自动推断 + CSE/LICM）

> 状态：🟢 已完成 | 创建：2026-08-06 | 完成：2026-08-06

## 进度概览
- [x] 阶段 1: 纯度推断（IrPureFunctionTable 模块不动点）
- [x] 阶段 2: OptSet 位 + 管线接线
- [x] 阶段 3: CSE + LICM pure-call 分支
- [x] 阶段 4: 测试（codegen / golden / bench）
- [x] 阶段 5: GREEN + 文档 + 归档 + PR

## 阶段 1: 纯度推断
- [x] 1.1 `IrPureFunctionTable.z42`：`PureTable`（bool[] 按 m.Functions 下标 + name→index StrMap，规避 StrMap 无 Remove）+ `Compute(m)` 乐观全纯→单调降级不动点 + `_isFuncPure`（IsPure ∪ 纯 call ∪ readonly-fget ∧ 无 throw 终结）

## 阶段 2: OptSet + 管线接线
- [x] 2.1 `OptSet.z42`：`PureCall=512`、`All=1023`、`ByName` 加 `pure-call`
- [x] 2.2 `IrOptPipeline.Run`：PureCall 开时算 PureTable，传 `_optFunc` → licm/cse 门控加 PureCall + 传表

## 阶段 3: CSE + LICM
- [x] 3.1 `IrOptInfo.CseKey` 加 PureTable 参数 + CallInstr 分支（callee 纯 + 全 args stable → `call|Func|argIds`）；`DstReg` 加 CallInstr
- [x] 3.2 `IrOptPipeline._passCse` 传 PureTable（纯调用无需失效表）
- [x] 3.3 `IrLicm.Run` 收 PureTable + `_isHoistablePureCall`（callee 纯 + args 全循环不变）
- [x] 3.4 `IrDump.z42` 默认 dump optSet 减去 PureCall（防既有 golden 漂移）

## 阶段 4: 测试
- [x] 4.1 `codegen_tests.z42`：纯调用 CSE 消重 / 非纯不消重 / 循环纯调用 LICM 外提 / 循环变参不提
- [x] 4.2 `src/tests/optimization/pure_call_hoist/` 运行时 golden（开/关一致）
- [x] 4.3 `src/libraries/z42.core/bench/pure_call_bench.z42` bench + A/B 数字记 PR

## 阶段 5: 验证 + 文档 + 归档
- [x] 5.1 GREEN：`xtask test` 全 stage（**重点盯自举 gen1==gen2** + golden；重建 worktree xtask 后跑）
- [x] 5.2 `docs/book/src/runtime/optimization-pipeline.md` 加 pure-call pass；`README.md` 功能索引
- [x] 5.3 `docs/features.md` 登记；`docs/roadmap.md` Deferred（跨包/去虚化/分配标量替换/放宽 Div）
- [x] 5.4 归档 + commit + PR

## 备注
- 分支基于 **origin/main（2914ac1f，含 readonly #124 + crossproc-escape + mimalloc）**。
- **无格式 bump**（PureTable 内存分析，不入 zbc）。
- ⚠️ 风险：z42c/stdlib 有纯函数 → PureCall 进 All 改自编译产物（语义不变字节变）→ 盯自举不动点（D7 自愈）+ golden 重生。
- 实施期确认点：IrDump 默认 dump 路径的 optSet 表达式位置；_isFuncPure 对终结子 throw 的判定 API；CseKey 签名膨胀（暂加参数，未来可封 context）。
