# 同步原语：值在堆上，原生层只留 Monitor

> 对齐：2026-09-14 `store-sync-values-in-heap`。
> 代码：`src/runtime/src/corelib/monitor.rs`（原生底座）、
> `src/libraries/z42.threading/src/{Mutex,RwLock,Channel}.z42`（z42 实现）、
> `src/libraries/z42.core/src/Native/ThreadingNative.z42`（`MonitorNative` 声明）。

## 为什么

`Std.Threading.Mutex<T>` / `RwLock<T>` / `Channel<T>` 原先把值存在 **Rust 侧容器**里——
`parking_lot::Mutex<Value>` / `parking_lot::RwLock<Value>` / `std::sync::mpsc` 队列，按槽位 id
登记在 `VmCore.{mutexes,rwlocks,channels}`。GC 的外部根扫描器（`vm_context/construct.rs`）只扫
static 字段、各线程帧寄存器和几个 arena，**看不见这些容器**。

后果是：一个值一旦**只被原语持有**，下一次回收就会被收掉。单线程即可确定性复现——往 `Channel<Box>`
发几个对象、分配一批垃圾、`GC.ForceCollect()`、再收回来：

| 运行方式 | 结果 |
|---|---|
| release（interp / JIT） | 字段读成 `null`，或者**读到另一个对象**（槽位已被复用） |
| debug VM | `panic: GcRef::entry_ref: generation/alive mismatch — use-after-finalize` |

这正是多线程代码里「本该有值处读到 `Null`」（`VCall: expected object, got Null`、`BrCond expects bool, got Null`）
的一类成因：`z42.net` 的 `HttpServer.ServeWithPool` 经 `Channel<TcpClient>` 把连接交给工作线程，
在队列里等待的 `TcpClient` 就处在无根状态。

同类问题此前出现过一次：#617（`Thread.Start` 捕获的环境在 spawn 窗口里没有根）。两者形状相同——
**原生代码持有 `Value`，却不在根集里**。

### 为什么不「存入时 pin 一下」

原生槽位**从不回收**（registry 只增不减）。存入时 `pin_root`、取出时 unpin，确实能保住值，
但被丢弃的 `Mutex` 里的当前值会随槽位**永久泄漏**——把一个 use-after-free 换成一个内存泄漏。
正确的生命周期是：**原语对象可达，值就可达；原语对象不可达，值就不该因它而存活。**

## 不变量

> **原生层不持有任何 GC 值。**

值是 z42 对象的普通字段——`Mutex`/`RwLock` 的 `_value`、`Channel` 的环形缓冲区 `_buf`。
GC 按常规对象图追踪它们、按常规写屏障记录写入、随拥有者一起回收。原生层只提供同步，
而同步只需要一个原语：**Monitor**。这条不变量同时写进了 `../../../agent/rules/runtime-rust.md`。

```
z42 Mutex<T>   { object _mon; T _value }
z42 RwLock<T>  { object _mon; T _value; int _readers; int _writersWaiting }
z42 Channel<T> { object _mon; T[] _buf; int _head; int _count; int _cap; bool _closed; … }
                   │           └─ 普通字段：GC 追踪、写屏障、随对象回收
                   ▼
句柄对象（合成 TypeDesc `Std.Threading.MonitorHandle`，无字段）
  NativeData::Monitor(Arc<Monitor>)
                   ▼
Monitor { state: Mutex<{ owner, entry_waiters, signal_waiters }>, entry_cv, signal_cv }
  └─ 不含任何 Value
```

**句柄生命周期**：`Arc<Monitor>` 挂在句柄对象的 `NativeData` 上，不进任何 registry。对象被清扫时
只打墓碑；`NativeData` 在该槽位被**复用时**随旧条目 drop。延迟释放无害——Monitor 不含 GC 值，
只是几十字节的 Rust 内存；正在阻塞的线程持有自己克隆的 `Arc`，不依赖句柄存活。GC 目录因此零改动。

## Monitor

| 操作 | 语义 |
|---|---|
| `Enter` | 阻塞直到持有。**同线程重入报错**（可 catch），不是死锁 |
| `TryEnter` | 不阻塞；被任何线程（含本线程）持有即返回 false |
| `Exit` | 释放；非持有者调用报错 |
| `Wait` | 原子地「释放 → 等一次 `Exit` 的信号 → 重新持有」。**允许伪唤醒** |

调用方永远写成：

```z42
MonitorNative.Enter(m);
try {
    while (!条件) { MonitorNative.Wait(m); }
    …
} finally { MonitorNative.Exit(m); }
```

「释放持有」与「开始等待」在同一次内部锁持有内完成；想改条件的一方必须先拿到内部锁再 `Exit`，
所以不会丢唤醒。

### 两个条件变量

`entry_cv` 挂「等进入」的线程，`signal_cv` 挂「在 `Wait` 里等信号」的线程：

- `Exit`：唤醒**一个**进入者，**广播**所有等信号者；
- `Wait` 释放持有时：**只唤醒进入者**。

若共用一个条件变量，两个同时在 `Wait` 的线程（比如两个消费者等同一个空队列）会在各自释放持有时
互相唤醒 → 醒来条件仍不满足 → 再 `Wait` → 再唤醒对方……无限空转。单测
`two_waiters_do_not_spin_against_each_other` 专门盯这一点（单条件变量写法下，100ms 安静期内实测空转 4 万余次）。

两个计数（`entry_waiters` / `signal_waiters`）让无人等待时 `Exit` 不发任何唤醒。

### 阻塞、safepoint 与内部锁的顺序

这是整个实现里最容易写错的地方。Monitor 的「持有」是 z42 层语义（`owner` 字段）；内部的
`parking_lot::Mutex<State>` 只在 O(1) 临界区里被持有。三条铁律：

1. **阻塞必须处于 `NativeParkGuard` 之内。** 否则另一线程发起 GC 时要等「全世界停下」，
   而阻塞中的线程永远到不了字节码 safepoint ⇒ 死锁（#598 / #600 / #606 同一判据）。
2. **离开 park 之前必须先释放内部锁。** `NativeParkGuard` 退出时若 GC 正在 STW，要等它结束。
   假如此时还攥着内部锁：

   ```mermaid
   sequenceDiagram
       participant A as 线程 A（刚被唤醒）
       participant GC as GC（STW 中）
       participant B as 线程 B（在 Exit 里）
       A->>A: 持有内部锁，退出 park
       A->>GC: 等 STW 结束
       B->>A: 抢内部锁（未 park、不在 safepoint）
       GC->>B: 等 B 停下
       Note over A,B: A 等 GC，GC 等 B，B 等 A —— 死锁
   ```

   代码里靠**声明顺序**保证：`_park` 先声明（后 drop），内部锁守卫后声明（先 drop）。
3. **阻塞前先把 `Arc<Monitor>` 从句柄对象里克隆出来，并释放对象借用。** 带着对象借用阻塞，
   会挡住 GC 对该对象的处理。

```rust
fn enter(&self, ctx) -> Result<()> {
    { // 快路径：不 park —— park 的进出要拿全局 phase 锁并 notify_all
        let mut st = self.state.lock();
        if st.owner == Some(me) { bail!("not reentrant") }
        if st.owner.is_none() { st.owner = Some(me); return Ok(()) }
    }
    let _park = NativeParkGuard::enter(ctx);   // 先声明 → 后 drop
    let mut st = self.state.lock();            // 后声明 → 先 drop
    st.entry_waiters += 1;
    while st.owner.is_some() { self.entry_cv.wait(&mut st); }
    st.entry_waiters -= 1;
    st.owner = Some(me);
    Ok(())
}
```

快路径失败后**先短暂自旋再 park**（32 次 `spin_loop` + 8 次 `yield_now`，每次重试获取）：Monitor 保护的都是
O(1) 的 z42 片段，持有者通常转眼就释放，而 park 一次要拿全局 phase 锁、`notify_all`，被唤醒还要走条件变量。
`Wait` 不自旋——实测给它加自旋没有可测收益（见下「性能」）。

快路径与自旋都不 park 是安全的：内部锁只被 O(1) 临界区持有；在 park 区里持有它的线程不会在持有期间去等 GC
（它只在释放内部锁之后才退出 park），所以短暂等它的线程很快就能走到下一个 safepoint。

`profile-contention` feature 下，慢路径计入 `lock_contentions` 并计时 `lock_wait_us`，默认构建零开销。

## 三个原语的 z42 实现

**`Mutex<T>`**：`Enter` → `_value = body(_value)` → `finally Exit`。body 抛异常时赋值被跳过、值不变。

**`RwLock<T>`**：

- 读者只在进出 `_readers` 计数时短暂持有 Monitor，body 不持有 ⇒ 多读者并发；
- 写者 **body 期间持有 Monitor**：先 `_writersWaiting++`，`while (_readers > 0) Wait`，再执行 body；
  这期间到来的读者阻塞在 `Enter` 上；
- **写者优先**：有写者在等（`_writersWaiting > 0`）时新读者也 `Wait`，读多的负载饿不死写者；
- `TryRead` / `TryWrite`：`TryEnter` 失败或条件不满足即返回 false，不等待。

**`Channel<T>`**：环形缓冲 `_buf`，`_cap` = -1 无界（满则翻倍扩容）/ > 0 有界 / 0 会合（缓冲区 1 格）。

- `Send`：已关闭 → 抛 `ChannelDisconnectedException`；有界满则 `Wait`（期间被关闭同样抛）；
  入队并记下自己的序号；会合模式下再 `while (_taken < 序号) Wait`，直到被取走才返回；
- `Recv`：`while (空 && 未关闭) Wait`；仍空（已关闭）→ 抛 `ChannelDisconnectedException`；
  出队后把该格写回 `default(T)`，已出队的值不被缓冲区拖住；
- `TrySend`：会合模式只有「已有接收者阻塞在 `Recv` 里」时才成功；
- 每个操作只持有 Monitor 做 O(1) 的事。

### 同线程重入

| 写法 | 结果 |
|---|---|
| `Lock` 里再 `Lock` | 抛异常（原先永久自死锁） |
| `Write` 里再 `Write` / `Read` | 抛异常（原先永久自死锁） |
| `Read` 里 `Write` | **自等**：等自己的读者退出 |
| 有写者排队时 `Read` 里再 `Read` | **自等**：写者优先 ⇒ 等写者 |

后两行是读写锁的固有语义，与原 `parking_lot` 实现一致。**不要嵌套获取同一个 `RwLock`。**

## 性能

同一个 VM 二进制（旧 builtin 仍在），分别配旧 stdlib（nightly SDK）与新 stdlib，交替各跑 5 次取中位数，
每项 30 万次操作，interp：

| 操作 | 旧（值存 Rust 侧） | 新（Monitor + z42） | 比值 |
|---|---|---|---|
| `Mutex.Lock` | 549 ms | 117 ms | 0.21 |
| `RwLock.Read` | 517 ms | 237 ms | 0.46 |
| `RwLock.Write` | 215 ms | 154 ms | 0.72 |
| `Channel` 同线程 Send+Recv | 554 ms | 405 ms | 0.73 |
| `Channel` 跨线程（有界 64，一生产者一消费者） | 338 ms | 540 ms | **1.60** |

锁类原语明显变快（旧实现每次加锁要跨 3 次 builtin 并在线程局部 map 里簿记守卫）。**跨线程高频争用的
Channel 变慢**：旧实现底下是 `std::sync::mpsc`，生产者和消费者几乎不互相阻塞；新实现两者争同一个 Monitor。
`enter` 加自旋前是 2.15 倍，加后 1.60 倍；再给 `Wait` 加自旋没有可测收益（已回退）。按每次操作折算约多 0.7 µs，
对以 I/O 为主的使用方（如 `HttpServer.ServeWithPool`）可以忽略；若将来出现吞吐敏感的场景，方向是给 Channel
单独做无锁快路径，而不是退回把值存进原生容器。

## 测试

- `z42.threading/tests/gc_sync_values_rooted.z42`：三种原语的值在强制回收后完好。**做过退回对照**——
  修复前 interp 下 8 条里 7 条失败（报的正是 `got Null`），JIT 下连测试宿主都被打崩；
  唯一修复前就通过的是「被丢弃的 Mutex 不保留值」，它防的是泄漏回归。
- `z42.threading/tests/sync_semantics_edges.z42`：重入抛异常、关闭后 `Send`、会合、扩容跨回绕保序、
  跨线程 GC 压力、`Close` 放出所有阻塞的接收者。
- `corelib/monitor_tests.rs`：互斥、重入 / 非持有者报错、`wait` 释放并重新持有、两个等待者不空转、
  阻塞在 `enter` / `wait` 里的线程计入 `parked_count`。

⚠️ 写这类用例时，值必须**真的只剩原语一个持有者**：在调用方直接 `new Mutex<Box>(mk(7))`，`mk(7)` 的
临时寄存器会一直留在帧里把对象保活，用例「碰巧绿」。建锁一律放进只返回锁的辅助函数。

## Deferred / Future Work

### store-sync-values-in-heap-remove-legacy：删除旧同步原语 builtin ✅ 已完成（2026-09-26）

19 个旧 builtin（`__mutex_*` 4 / `__channel_*` 7 / `__rwlock_*` 8）、`VmCore.{mutexes,rwlocks,channels}`
三个 registry、`corelib/sync.rs`（596 行）与 `native_decl_tests.rs` 的豁免条目**已全部删除**。

**触发条件是怎么核的**（按本节原先写下的办法）：对下载到本地的 nightly 种子
（`artifacts/build/compiler/bootstrap-check/nightly/`）的 `programs/z42c/*.zpkg` 与 `libs/*.zpkg`
执行 `strings -n 3 | grep -E "__mutex_|__channel_|__rwlock_"` —— **必须先 `strings` 再 grep**，
直接 grep 二进制会假缺席。结果两处皆 **0 引用**（同一份种子里 `__monitor_` 有 5 处，
说明它确实是 2026-09-14 之后的）。

> 📌 **原先记的「为什么没一起删」有一处说法需要更正。** 本节原文写「builtin id 在加载时按名解析」
> 是对的，但 `builtin_table_ext.rs` 的头注同时写着「BuiltinId 就是下标，插在中间会让既有 zbc 里的
> 调用全部错位」—— 两处互相矛盾。读码定论（2026-09-26）：
>
> - zbc 里存的是**名字**：`BuiltinInsn { dst, name, args }`（`zbc_reader/instr_decode.rs`）；
> - `BuiltinId` 由 resolver 在**加载期**经 `builtin_id_of(name)` 填进
>   `Function.resolved.builtin_tokens`，解释器与 JIT 都只把它当**单次运行内的派发令牌**；
> - AOT 不烤它。
>
> ⇒ **槽位可以真删**，append-only 是便于 review / 稳定 id 类测试的**约定**，不是格式约束。
> 该头注已一并更正。真正的判据始终是上面那条：**已发布种子里还有没有 z42 源声明这个名字**。

**顺带**：`runtime/tests/cross_thread_smoke.rs` 里直接调旧 builtin 的两个测试（mutex 自增、channel
生产消费）随之删除 —— 覆盖没有蒸发，它在 z42 那一层：`src/libraries/z42.threading/tests/` 的
17 个单元跑在活路径（`__monitor_*`）上，由 GREEN 的 `stdlib [Test]` 阶段执行。该文件头注声明的
「Send/Sync 端到端证明」由其余 8 个测试继续扛着。
