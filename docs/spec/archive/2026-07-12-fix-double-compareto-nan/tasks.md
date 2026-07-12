# Tasks: Double.CompareTo(NaN) 全序（NaN 排最前，等于自身）

> 状态：🟢 已完成 | 创建：2026-07-12 | 完成：2026-07-12 | 类型：fix（最小化模式）
> 子系统：stdlib（短占，归档即释放）。隔离 git worktree 实施+验证（主树被并发 params 迁移污染）。

**变更说明：** `Double.CompareTo` 用朴素 `<`/`>`——对 NaN 两者皆 false → 返 0（等于一切），
破坏全序，含 NaN 的 `List<double>.Sort()` 未定义。改镜像 C# 语义：NaN 排最前、等于自身
（`x != x` 检测 NaN）。

**原因：** review §2.2——NaN 破坏 CompareTo 全序。

**文档影响：** 无对外 API 变化；行为「NaN CompareTo 返 0→NaN 排序有定义」。无 README/book 变更。

- [x] 1.1 `Double.z42` CompareTo：NaN 分支（both NaN→0 / this NaN→-1 / other NaN→1），正常路径不变
- [x] 1.2 回归测试 `op_edge_cases.z42`：NaN.CompareTo 全序（NaN==NaN / NaN<1 / 1>NaN）+ 正常序不变
      （NaN 经 `double.Parse("nan")` 构造）
- [x] 1.3 GREEN：worktree 内 `xtask test` 全绿
- [x] 1.4 归档 + 释放 ACTIVE.md stdlib 锁

## doc-check
- [x] 无对外 API/依赖/入口变化；README 无需改；无 book 变更
