# Tasks: Mutex/RwLock 回调抛异常时释放锁（消死锁）

> 状态：🟢 已完成 | 创建：2026-07-12 | 完成：2026-07-12 | 类型：fix（最小化模式）
> 子系统：stdlib（短占——converge-z42c-onto-z42-project 的 stdlib 占用为 DRAFT 预留，
> 且其自身阻塞在 compiler 锁；User 授权本高危单项 fix 先占 stdlib，归档即释放）

**变更说明：** `Mutex<T>.Lock` / `RwLock<T>.{Read,Write,TryRead,TryWrite}` 的回调体抛异常时，
后续 `__*_unlock`/`__*_release` 被跳过 → 锁永不释放 → 生产死锁。加 try/finally 保证释放。

**原因：** 运行时用 `mem::forget(guard)` + 显式 unlock 释放锁（`corelib/sync.rs` `HELD_MUTEX_GUARDS`），
无 unwind 期自动释放；Mutex.z42 类头注释「throw 会干净释放」与实现相反（review §2.4，排名 #1）。

**文档影响：** Mutex.z42 类头注释（描述真实释放机制）；无外部 API/行为契约变更（仅修复隐藏死锁），
故不触发 README/book 同步矩阵——释放语义本就是文档承诺的行为，此处使之成真。

- [x] 1.1 `Mutex.z42` `Lock`：body+Store 入 try、Unlock 入 finally；改正类头注释
- [x] 1.2 `RwLock.z42` `Read`/`Write`/`TryRead`/`TryWrite`：release 入 finally
- [x] 1.3 回归测试 `tests/lock_release_on_throw.z42`：5 例（Mutex/RwLock Read/Write/TryRead/TryWrite）
      body 抛异常后再取锁不死锁 + 值未写入 → `z42.threading` 全 13 文件 PASS（含本 5 例）
- [x] 1.4 GREEN：`xtask test` **全绿**（e2e 197/0 + cross-zpkg + stdlib 全库 0 failed（含本 5 例）
      + compiler 自举不动点 7/7 byte-identical + vscode-syntax；C#-free）。注：本地 `.z42` 种子被前序
      会话遗留在半 bump 态（0.29 seed vs 0.31 源）；已用 in-tree 0.31 产物（cargo VM +
      `artifacts/build/libraries/dist` + `artifacts/build/compiler/z42c.driver/dist`）刷新
      `.z42/{bin,libs}` 恢复可运行工具链（`.z42` 为 gitignore 本地 SDK，不入提交）
- [x] 1.5 归档 + 释放 ACTIVE.md stdlib 锁（stdlib 归还排队的 converge-z42c-onto-z42-project）

## doc-check
- [x] 触发矩阵：无新增/删除文件对外入口、无 API/依赖变更 → 仅 `Mutex.z42` 类头注释校正（描述真实
      release 机制）；`docs/design/runtime/concurrency.md:34` 已正确描述机制、无 falsehood，无需改
- [x] 本次触及文档相对链接可解析
