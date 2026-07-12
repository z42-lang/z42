# Tasks: LinkedList.ToArray + Stream.ReadByte/WriteByte（§2.6 补齐）

> 状态：🟢 已完成 | 创建：2026-07-12 | 完成：2026-07-12 | 类型：feat（最小化模式，additive API）
> 子系统：stdlib（短占，归档即释放）。隔离 git worktree 实施+验证。

**变更说明：** review §2.6：
- `LinkedList<T>.ToArray()`——补齐 ToArray 家族最后一员（head→tail 非破坏快照）。
- `Stream.ReadByte()/WriteByte(byte)`——单字节便捷 API（默认基于 Read/Write，子类可覆写热路径），
  `ReadByte` 返回 0..255 或 -1(EOF)；替调用方手写 `new byte[1]`（BinaryReader）。

**原因：** review §2.6——集合无非破坏枚举 / Stream 无单字节 API。

**文档影响：** 新增对外 API；无行为契约变更；无 book 变更。

- [x] 1.1 `z42.collections/LinkedList.z42`：ToArray（head→next 遍历填 char[]）
- [x] 1.2 `z42.io/Stream.z42`：virtual ReadByte（0..255 / -1）+ WriteByte（默认 1-byte scratch）
- [x] 1.3 回归测试：linkedlist.z42（ToArray + 空 + 非破坏）/ stream_memory.z42（WriteByte/ReadByte
      round-trip + 无符号 200 + EOF -1）
- [x] 1.4 GREEN：worktree 内 `xtask test` 全绿
- [x] 1.5 归档 + 释放 ACTIVE.md stdlib 锁

## doc-check
- [x] 新增对外 API；无行为契约变更；无 book 变更
