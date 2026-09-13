# Tasks: 同步原语阻塞时让出 GC safepoint（第三轮）

> 状态：🟢 已完成 | 完成：2026-09-13
> 变更类型：`fix`（最小化模式）｜接续 #598 / #600

**变更说明：** `corelib/sync.rs` **整个模块零 `NativeParkGuard`**。六个阻塞点全部补上。

| 阻塞点 | 危险度 |
|---|---|
| `channel_recv` 的 `recv()` + 内层 `rx_arc.lock()` | 🔴 **最高** —— `Recv()` 阻塞等生产者是生产者/消费者的**标准写法**；生产者 `Send` 前触发 GC ⇒ 消费者阻塞着到不了 safepoint ⇒ GC 等不到它 ⇒ 生产者也发不出来。与 `Thread.Join`（#600）同构 |
| 有界 `channel_send` 的 `send()` | 队满阻塞等消费者 —— 与 Recv 对称的死锁面 |
| `mutex_lock_acquire` 的 `contended_lock` | 争用即阻塞 |
| `rwlock_read_acquire` / `rwlock_write_acquire` | 同上 |
| `thread_sleep` | **不是永久死锁**，但 GC 停顿被拉长到整个睡眠时长（`Sleep(60000)` 卡 GC 一分钟；poll 循环持续拖累）。其 `_ctx` 原本就是未使用参数 |

**回归用例：** `z42.threading/tests/gc_park_while_blocked.z42` 三条，形状统一为
「一个线程阻塞等另一个，另一个大量分配到触发 GC」：channel recv / thread join / mutex 争用。

**关键：用例做过阴性对照** —— 回退 park 后用同一份用例跑，**90s 挂住不结束**；有 park 则全过。
不是「碰巧绿」的测试。（这类死锁只在「阻塞 + 并发 GC」同时成立时现形，用例必须真的分配到
触发 GC 才有意义 —— 每轮拼新字符串 + 建数组，循环量级跨过 nursery。）

- [x] 1.1 sync.rs 六个阻塞点（mutex / rwlock ×2 / channel recv / channel send）
- [x] 1.2 threading.rs 的 `thread_sleep`
- [x] 1.3 回归用例 + 阴性对照验证
- [x] 1.4 GREEN：`xtask test` 全 13 stage 绿；`cargo test --lib` 1251 + 21

## 三轮下来的完整清单（#598 / #600 / 本轮）

socket connect·accept·read·write / udp send·recv / connect_timeout / `Thread.Join` /
`child.wait` + 读线程 join ×2 / `stdin().read_line` / mutex / rwlock ×2 /
channel recv·send / `Thread.Sleep` —— **共 17 处**。

**仍未覆盖（有意）**：普通文件 I/O —— 本地盘读通常不久，全包会给热路径加 park/unpark 开销。
