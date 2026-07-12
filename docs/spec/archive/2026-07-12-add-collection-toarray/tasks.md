# Tasks: Queue/Stack/List ToArray + List.AddRange（§2.6 补齐）

> 状态：🟢 已完成 | 创建：2026-07-12 | 完成：2026-07-12 | 类型：feat（最小化模式，additive API）
> 子系统：stdlib（短占，归档即释放）。隔离 git worktree 实施+验证。

**变更说明：** review §2.6 指出 `Queue/Stack/LinkedList 无 ToArray()`（不破坏结构无法枚举）、
`List<T> 缺 AddRange/ToArray`。本 change 补：
- `Queue<T>.ToArray()`——FIFO 快照（head→tail），非破坏性（ring buffer 原不可非破坏遍历）。
- `Stack<T>.ToArray()`——top-first 快照（对齐 C# Stack.ToArray），非破坏性。
- `List<T>.ToArray()`——元素快照拷贝；`List<T>.AddRange(T[])`——整数组追加。

（LinkedList.ToArray 留后续；本批聚焦 Queue/Stack/List 三主力。）

**原因：** review §2.6——集合无非破坏枚举/批量追加 API。

**文档影响：** 新增对外 API；无行为契约变更；无 book 变更。

- [x] 1.1 `z42.collections/Queue.z42`：ToArray（FIFO ring 快照）
- [x] 1.2 `z42.collections/Stack.z42`：ToArray（top-first 快照）
- [x] 1.3 `z42.core/Collections/List.z42`：ToArray + AddRange
- [x] 1.4 回归测试：queue.z42（FIFO + 非破坏 + Dequeue 后跟随）/ stack.z42（top-first + 非破坏）/
      新 list_api.z42（ToArray + 空 + AddRange）
- [x] 1.5 GREEN：worktree 内 `xtask test` 全绿
- [x] 1.6 归档 + 释放 ACTIVE.md stdlib 锁

## doc-check
- [x] 新增对外 API；无行为契约变更；README 无强制改（集合类条目不变）；无 book 变更
