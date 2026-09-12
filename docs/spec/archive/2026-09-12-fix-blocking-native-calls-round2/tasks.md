# Tasks: 其余阻塞 native 调用让出 GC safepoint（第二轮）

> 状态：🟢 已完成 | 完成：2026-09-12
> 变更类型：`fix`（最小化模式）｜**接续 #598**（那轮只覆盖了网络七处）

**变更说明：** #598 修掉了 socket 的七个阻塞点，但同一条判据下还有三类 native 调用漏网。
判据只有一句：**任何可能长时间不返回的 native 调用都必须包 `NativeParkGuard`** ——
否则线程在阻塞期间到不了 safepoint，另一线程发起 GC 就会永远等下去。

| 位置 | 阻塞点 | 为什么危险 |
|---|---|---|
| `corelib/threading.rs` | `handle.join()` | ⚠️ **最危险的一个** —— `t.Join()` 是 z42 多线程的**主干写法**。被 join 的线程只要在结束前触发 GC，join 方卡在 `handle.join()`（不在安全点）⇒ GC 等不到它 ⇒ 被 join 的线程也结束不了。教科书式死锁，且在用户代码主路径上 |
| `corelib/process.rs` | `child.wait()`（经 `wait_with_optional_timeout`）+ 两个读线程 `join()`；`handle_wait` 的 `wait` + 两个 `join` | 子进程跑多久就阻塞多久。**xtask / z42b 一直在 spawn 子进程并等待**，是这条最密集的使用者 |
| `corelib/io.rs` | `stdin().read_line` | 等用户输入，可以无限久 |

**实现要点：** `NativeParkGuard` 的作用域必须**紧**——parked 期间不得 mutate 根或分配，
所以 park 一律在回到 VM（构造返回值 / 调 `ok_result`）**之前**结束：
`process.rs` 的两处用块作用域把「wait + 读线程 join」整段圈住再取值，`handle_wait` 显式 `drop(_park)`。

**为什么这类 bug 长期隐形：** 只在「阻塞方 + 另一线程恰好触发 GC」同时成立时才现形，
而触发 GC 需要足够的堆压力。小测试跑不出来 —— #598 那个网络死锁正是在
「转发让 z42b in-process 编译完整个包（堆很大）」之后才第一次暴露。

- [x] 1.1 `threading.rs` 的 `Thread.Join`
- [x] 1.2 `process.rs` 的 `process_run` / `process_handle_wait`（wait + 读线程 join 全段）
- [x] 1.3 `io.rs` 的 `stdin().read_line`
- [x] 1.4 GREEN：`xtask test` 全 13 stage 绿；`cargo test --lib` 1251 + 21；
      `z42.threading` 13 个单元全过

## 仍未覆盖（有意留下）

普通文件 I/O（`File.ReadAllBytes` 等）未包 —— 本地盘读通常不阻塞很久，全包会给热路径加
park/unpark 开销。若将来出现网络文件系统场景，按同一判据补。
