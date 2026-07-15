# Tasks: MemoryStream.Seek i32 溢出静默截断修复

> 状态：🟢 已完成 | 创建：2026-07-12 | 完成：2026-07-12 | 类型：fix（最小化模式）
> 子系统：stdlib（短占，归档即释放）。隔离 git worktree（0.32 工具链，镜像自主树）实施+验证。

**变更说明：** `MemoryStream.Seek` 把 `long newPos` 直接 `(int)newPos` 存入 `_position`——
目标位置超 int.MaxValue 时静默回绕成负数、损坏流状态（负检查只挡 newPos<0，挡不住超大正值）。
加 `newPos > 2147483647L`（int.MaxValue，无 Int32.MaxValue 常量）检查，越界抛 ArgumentException。

**原因：** review §2.1——`(int)newPos` 超 i32 回绕无检查。

**文档影响：** 无对外 API 变化；行为「超 i32 静默截断→抛 ArgumentException」。z42.io README 无需改；
      无 book 变更。

- [x] 1.1 `MemoryStream.z42` Seek：`newPos > 2147483647L` 越界抛 ArgumentException
- [x] 1.2 回归测试 `stream_memory.z42`：Seek(3e9) 抛异常 + position 不变
- [x] 1.3 GREEN：worktree 内 `xtask test` 全绿
- [x] 1.4 归档 + 释放 ACTIVE.md stdlib 锁

## doc-check
- [x] 触发矩阵：无对外 API/依赖/入口变化；行为「越界抛异常」。README 无需改；无 book 变更
