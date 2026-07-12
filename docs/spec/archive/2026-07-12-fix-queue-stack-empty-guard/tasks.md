# Tasks: Queue/Stack 空容器 Dequeue/Pop/Peek 加空检查（消状态损坏）

> 状态：🟢 已完成 | 创建：2026-07-12 | 完成：2026-07-12 | 类型：fix（最小化模式）
> 子系统：stdlib（短占——converge-z42c-onto-z42-project 的 stdlib 占用为 DRAFT 预留、
> 自身阻塞在 compiler 锁；User「Continuity」授权续做 review 批次 A 高危 fix，归档即释放）

**变更说明：** `Queue<T>.{Dequeue,Peek}` / `Stack<T>.{Pop,Peek}` 对空容器无检查——
Dequeue/Pop 读陈旧槽 + count 减到 -1（静默状态损坏），Peek 读越界槽。加空检查抛异常。

**原因：** review §2.1 排名 #2。同包 PriorityQueue/LinkedList 已抛异常 → 同包不一致 + 数据损坏。
本 fix 对齐同包既有风格（`throw new Exception("<Class>.<Method>: ... is empty")`）。
§2.7 的「空容器统一 InvalidOperationException」是独立的跨包约定变更，不在本 fix 范围。

**文档影响：** 无对外 API 新增/删除、无依赖变化；仅行为从「静默损坏」→「抛异常」（与同包一致）。
collections README 功能索引无需改（Queue/Stack 条目不变）。无 book 机制变更。

- [x] 1.1 `Queue.z42`：`using Std;` + `Dequeue`/`Peek` 空检查抛 `Exception`
- [x] 1.2 `Stack.z42`：`using Std;` + `Pop`/`Peek` 空检查抛 `Exception`
- [x] 1.3 回归测试：`tests/queue.z42` +3 例、`tests/stack.z42` +3 例
      （空 Dequeue/Pop/Peek 抛异常 + count 不变负 + drain-then-empty 越界抛）
- [x] 1.4 GREEN：`xtask test` **全绿**（e2e 197/0 + stdlib 全 273 文件/22 库 0 failed（含本 6 例）
      + 自举不动点 7/7 + vscode-syntax；C#-free）。注：首轮被并发会话共享 `artifacts/` 污染
      （197/197 假失败，expected==actual），等安静窗口重跑得干净全绿（User 裁决）
- [x] 1.5 归档 + 释放 ACTIVE.md stdlib 锁（归还排队的 converge-z42c-onto-z42-project）

## doc-check
- [x] 触发矩阵：无对外 API/依赖/入口变化；仅行为「静默损坏→抛异常」（与同包 PriorityQueue/
      LinkedList 一致）。collections README 功能索引/核心文件表无需改（Queue/Stack 条目不变）；
      无 book 机制变更（空检查非新机制）
- [x] 本次触及文档相对链接可解析
