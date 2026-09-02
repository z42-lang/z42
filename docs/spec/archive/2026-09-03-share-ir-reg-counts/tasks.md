# Tasks: 寄存器读/写计数单一实现（share-ir-reg-counts）

> 状态：🟢 已完成 | 创建：2026-09-03 | 完成：2026-09-03 | 类型：refactor（compiler）
**变更说明：** 新增 `z42c.semantics/src/IrRegCounts.z42`（`Defs` / `DefsCap` / `Reads`），删除 IrOptPipeline /
IrDeadBranch / IrEscapeAnalysis / IrLoopAllocReuse 四处逐字相同的 `_computeDefs`（+ IrOptPipeline 的 `_computeReads`），
全部改调 IrRegCounts。纯搬移，语义不变。
**原因：** 三面评审 C-2——同一段计数循环在四个文件平行维护；后续若给 pass 间加「变更标志 → 复用计数」
需要一个落点。
**文档影响：** `src/compiler/z42c.semantics/README.md` 核心文件表；`docs/book/src/runtime/optimization-pipeline.md` 读/写计数段。

- [x] 1.1 IrRegCounts.z42 + 四处替换
- [x] 1.2 文档同步
- [x] 1.3 验证：`xtask test`（含自举不动点）+ 产物字节对比（预期与 main 完全一致，除路径）
- [x] 1.4 归档

## 验证记录（2026-09-03）
- `xtask test` ✅ GREEN 14:46（不动点 3/3）。
- 产物字节对比（本分支 vs 同基线 ea983dfb 仅改 runtime 的 wt-vcall）：stdlib 25 包中 23 包逐字节相同，
  `z42c.core` / `z42c.syntax` 仅调试信息源码路径长度差；`z42c.driver` / `z42c.pipeline` 相同。
