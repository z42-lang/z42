# Tasks: AST walker 完备性 gate

> 状态：🟢 已完成（2026-09-09）｜ 创建：2026-09-09 ｜ scope: toolchain + docs（无格式 bump）

## 进度概览

| # | 阶段 | 状态 |
|---|---|---|
| 0 | walker 调研分类（7 候选 → 登记 4） | 🟢 已完成 |
| 1 | `xtask_test_walkers.z42` gate + 登记表 | 🟢 已完成 |
| 2 | stage 接线（`_gateStageNames` + `_testAll` + CLI + test-gate.md） | 🟢 已完成 |
| 3 | 修 MethodTypeParamUse 假注释 + 计数漂移 | 🟢 已完成 |
| 4 | 验证（阳性 A/B + 干净绿 + 完整 GREEN） | 🟢 已完成 |
| 5 | Deferred 登记 + 归档 | 🟢 已完成 |

## 阶段 0 —— walker 调研（勿重跑）

- [x] 0.1 密度扫描定位候选 walker（`is <NodeClass>` 高密度文件）。
- [x] 0.2 逐个分类 fallback（LOUD vs SILENT）+ 设计意图（穷举 vs 部分）。结论见 proposal 分类表。
- [x] 0.3 确认全部节点子类都在 `z42c.syntax/src/` 的 4 个 canonical 文件、无外溢。
- [x] 0.4 实测坐实 MethodTypeParamUse 抬头「已有完备性门」为假 + `TypeExpr 6`→5 计数漂移。

## 阶段 1 —— gate 实现

- [x] 1.1 `_wgCollectNodes`：活体枚举 `public sealed class NAME : (Expr|Stmt|Pattern|TypeExpr)`。
- [x] 1.2 `WalkerEntryZ` + `_walkerRegistry()`：4 个 walker（覆盖族 + 白名单）。
- [x] 1.3 `_wgIsMatched`：`is <类名>` 词边界匹配（前后都查，避 `axis IntLitExpr` 假命中）。
- [x] 1.4 `_testWalkers`：对账 + 排序输出 + 硬门（有 gap 即 exit 1）。

## 阶段 2 —— stage 接线

- [x] 2.1 `_gateStageNames()` 加 `"walkers"`（`_stageStart` 未登记即 throw 那道门要求）。
- [x] 2.2 `_testAll` 在 `lines` 之后加 walkers stage（可 `--skip walkers`）。
- [x] 2.3 `xtask_cli_test.z42`：`test walkers` ArgParser + dispatch。
- [x] 2.4 `test-gate.md` gate-stages 区加 `walkers` + 描述段（`_checkGateStageDoc` 锁步门）。

## 阶段 3 —— 修假注释

- [x] 3.1 `MethodTypeParamUse.z42` 抬头 ③ 改指向本门；覆盖基线 `TypeExpr 6`→`5`。

## 阶段 4 —— 验证

- [x] 4.1 干净树绿：67 节点类 × 4 walker = 129 对，0 gap。
- [x] 4.2 阳性 A：删 `is TupleExpr` → `✗ ExprTyper._bindExpr: TupleExpr (Expr)` + exit 1，git checkout 复原。
- [x] 4.3 阳性 B（核心不变量）：注入 `FakeProbeExpr : Expr` → ExprTyper + MethodTypeParamUse 同时报、
      Stmt/Pattern-only 正确忽略；exit 1，git checkout 复原。
- [x] 4.4 完整 GREEN（含 doc-drift 锁步门 + 新 walkers stage）。

## 阶段 5 —— 收尾

- [x] 5.1 Deferred：`analyzer-driver-walk-local-function`（AnalyzerDriver 不递归 LocalFunctionStmt 体）。
- [x] 5.2 归档 changes/→archive/ + 本 tasks 🟢（随 PR 同批）。
