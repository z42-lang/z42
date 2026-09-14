# Proposal: 同步原语的值存进 GC 堆（Channel / Mutex / RwLock）

> 变更类型：`vm`（完整流程）｜ 创建：2026-09-14 ｜ 来源：「推进 Null flake」调查

## Why

**`Channel<T>` / `Mutex<T>` / `RwLock<T>` 里存的值不是 GC 根，一次回收就会被收掉。**

`corelib/sync.rs` 把 `Value` 直接存进 Rust 侧容器——`std::sync::mpsc` 队列、
`parking_lot::Mutex<Value>`、`parking_lot::RwLock<Value>`，按槽位 id 登记在 `VmCore.{channels,mutexes,rwlocks}`。
GC 的外部根扫描器（`vm_context/construct.rs:260`）只扫 static 字段 / 各线程帧寄存器 / 三个 arena，
**不扫这三个 registry**，存入时也没有 `pin_root`。于是值一旦**只被原语持有**，就没有任何根。

实测（单线程、确定性，无需并发）：

```z42
var ch = new Channel<Box>(16);
// 循环 Send 8 个 Box（寄存器 b 每轮被覆盖）→ 分配 2 万个垃圾 → GC.ForceCollect() → 再分配 → Recv
```

| 运行方式 | 结果 |
|---|---|
| release interp / JIT | 6 个 `Name` 读成 `null`，1 个**变成了另一个对象**（`junk219968 -2`），只有仍在寄存器里的最后一个完好 |
| debug VM | `panic: GcRef::entry_ref: generation/alive mismatch — use-after-finalize` |
| 对照组（另用 `Box[] keep` 持有） | 全部正确 |

`Mutex<T>` / `RwLock<T>` 同样复现（在辅助函数里建锁、只返回锁本身，排除临时寄存器侥幸存活后，
release 读到 `null`、debug VM 同一个 panic）。

**影响**：任何「经 Channel 把对象交给另一个线程」「把对象放进 Mutex/RwLock 共享」的程序，都会随机读到
被回收的内存——表现为本该有值处读到 `Null`，或静默读到别的对象。`z42.net` 的 `ServeWithPool` 正是经
`Channel<TcpClient>` 派发连接，这是长期阻塞 GREEN 门的多线程 flake 的已证实成因之一。

### 为什么不是「补一个 pin」

原生槽位**从不回收**（registry 只增不减）。若只在存入时 `pin_root`，被丢弃的 `Mutex` 里的当前值会随槽位
**永久泄漏**——把一个 use-after-free 换成一个内存泄漏。User 裁决：一步到位，让原语持有的值的生命周期
**跟随拥有它的 z42 对象**。

## What Changes

**核心不变量：原生层不再持有任何 GC 值。** 值变成 z42 对象的普通字段，由 GC 按常规对象图追踪、
按常规写屏障记录；原生层只提供**不含值**的同步机制。

- 新增一个原生同步原语 **Monitor**（进入 / 尝试进入 / 退出 / 等待），状态里只有「持有者线程 + 等待者计数」。
  它以一个带 `NativeData::Monitor(Arc<Monitor>)` 的句柄对象交给 z42，**随句柄对象回收而释放**，不进任何 registry。
- `Mutex<T>` / `RwLock<T>` / `Channel<T>` 用 z42 在 Monitor 之上重写：值 / 队列缓冲区是对象字段。
  **公开 API 不变。**
- 行为修正（原先是缺陷，见 spec MODIFIED）：
  - 同线程在 `Lock` 中再 `Lock`、`Write` 中再 `Write` 或 `Read`：原先**永久自死锁** → 现在抛异常。
  - 向已关闭的 `Channel` `Send`：原先抛泛型 `Std.Exception("__channel_send: channel N is closed")` →
    现在抛 `ChannelDisconnectedException`（与 `Recv` 一致）。
- **两阶段落地**（`bootstrap-seed.md`「删 runtime builtin = 两 nightly」）：本变更**只新增** `__monitor_*`、
  让新 stdlib 不再引用旧的 19 个 `__mutex_*` / `__rwlock_*` / `__channel_*` builtin；旧 builtin 与三个
  registry **原样保留一个 nightly**（种子例外，不是兼容层），登记 Deferred 由后续变更删除。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/corelib/monitor.rs` | NEW | `Monitor` 原语 + `__monitor_new/enter/try_enter/exit/wait` 五个 builtin + 句柄 TypeDesc |
| `src/runtime/src/corelib/monitor_tests.rs` | NEW | Rust 单测：互斥 / 重入报错 / 非持有者退出报错 / wait 唤醒 / try_enter / 阻塞期间让出 safepoint |
| `src/runtime/src/corelib/mod.rs` | MODIFY | 声明 `mod monitor;` |
| `src/runtime/src/corelib/builtin_table_ext.rs` | MODIFY | 追加登记五个 `__monitor_*` |
| `src/runtime/src/corelib/sync_contention.rs` | MODIFY | 争用探针（`profile-contention` feature）接到 `Monitor::enter` |
| `src/runtime/src/corelib/native_decl_tests.rs` | MODIFY | 19 个旧 builtin 加入 `UNDECLARED_ALLOWLIST`（种子例外，阶段 2 删）——实施中补入 Scope |
| `src/runtime/src/corelib/sync.rs` | MODIFY | 仅模块头注释：标记为「阶段 1 种子例外，待删」，指向 Deferred |
| `src/runtime/src/metadata/types/object.rs` | MODIFY | `NativeData` 新增 `Monitor(Arc<Monitor>)` 变体 |
| `src/runtime/src/corelib/README.md` | MODIFY | 功能索引加 `monitor.rs`；`sync.rs` 标注待删 |
| `src/libraries/z42.core/src/Native/ThreadingNative.z42` | MODIFY | 新增 `MonitorNative`；删除 `MutexNative` / `RwLockNative` / `ChannelNative` 声明 |
| `src/libraries/z42.threading/src/Mutex.z42` | MODIFY | 在 Monitor 上重写，值为字段 |
| `src/libraries/z42.threading/src/RwLock.z42` | MODIFY | 在 Monitor 上重写（读者计数 + 写者优先；写者 body 持 Monitor） |
| `src/libraries/z42.threading/src/Channel.z42` | MODIFY | 在 Monitor 上重写（环形缓冲；无界 / 有界 / 容量 0 会合） |
| `src/libraries/z42.threading/tests/gc_sync_values_rooted.z42` | NEW | 回归：三种原语的值在强制 GC 后仍完好（确定性，单线程） |
| `src/libraries/z42.threading/tests/sync_semantics_edges.z42` | NEW | 重入抛异常 / 关闭后 Send 抛 `ChannelDisconnectedException` / 容量 0 会合 / 无界扩容保序 / 跨线程 GC 压力下的生产者-消费者 |
| `src/libraries/z42.threading/README.md` | MODIFY | 实现说明改为「值在堆上、Monitor 是唯一原生原语」 |
| `docs/book/src/runtime/sync-primitives.md` | NEW | 机制页：不变量、Monitor 状态机、park 与内部锁的顺序、三种原语的 z42 实现、Deferred |
| `docs/book/src/SUMMARY.md` | MODIFY | 挂载新机制页 |
| `docs/roadmap.md` | MODIFY | Deferred Backlog Index 加一行（阶段 2 删旧 builtin） |
| `.claude/rules/runtime-rust.md` | MODIFY | 新增一条：原生层不得在根集之外持有 `Value` |

**只读引用**：

- `src/runtime/src/corelib/threading.rs` — #617 `SpawnedEnvRoot`（同类问题的前例）
- `src/runtime/src/gc/safepoint.rs` — `NativeParkGuard` 协议
- `src/runtime/src/vm_context/construct.rs` — 外部根扫描器覆盖面
- `src/runtime/src/corelib/object.rs` — `weak_handle_type_desc`（合成 TypeDesc 前例）
- `src/libraries/z42.threading/tests/*.z42` — 现有语义用例（必须原样通过）
- `src/libraries/z42.net/src/Http/HttpServer.z42` — `ServeWithPool` 使用方
- `src/tests/perf/scenarios/06_thread_scaling.z42` — 前后性能对比

## Out of Scope

- **删除旧 builtin / registry / `sync.rs`**：阶段 2，等本变更进 nightly 后另起变更（Deferred）。
- **z42.net threaded 用例的 flake**：该路径不经过这三个原语，本变更不声称修复它；调查继续。
- 公开 `Std.Threading.Monitor`（C# `Monitor.Enter/Wait/Pulse`）：本变更里 Monitor 只是内部原语。
- `VmCore.pending_thrown` 不在根扫描里且为核心级共享——调查中顺带发现，另行评估。
- 带超时的等待、公平性保证。

## Open Questions

- [ ] 同线程重入由「自死锁」改为「抛异常」、关闭后 `Send` 改抛 `ChannelDisconnectedException`——两处行为修正是否接受？
- [ ] `RwLock` 用「写者优先」（有写者在等时新读者等待）、写者 body 期间持有 Monitor（D5），是否接受？原 `parking_lot::RwLock` 也偏写者。
