# Spec: 同步原语（Channel / Mutex / RwLock）

## ADDED Requirements

### Requirement: 原语持有的值在 GC 中保持可达

原语持有的值（Channel 队列中的元素、Mutex / RwLock 的当前值）只要原语对象可达，就必须可达；
原语对象不可达后，这些值不得因原语而被额外保留。

#### Scenario: Channel 队列中的对象跨越回收
- **WHEN** 向 `Channel<Box>` Send 若干对象，且这些对象除队列外无任何引用；随后大量分配并 `GC.ForceCollect()`
- **THEN** 依次 Recv 得到的每个对象字段完好、顺序与 Send 一致（interp / JIT 均是；debug VM 不触发 `use-after-finalize`）

#### Scenario: Mutex 的当前值跨越回收
- **WHEN** 在辅助函数中 `new Mutex<Box>(mk(7))` 并只返回锁；随后大量分配并 `GC.ForceCollect()`
- **THEN** `Lock` 的 body 收到的对象字段完好

#### Scenario: RwLock 的当前值跨越回收
- **WHEN** 同上，换成 `RwLock<Box>`，分别经 `Read` 与 `Write` 读取
- **THEN** 两条路径读到的对象字段完好

#### Scenario: Write / Lock 存入的新值跨越回收
- **WHEN** `Lock` / `Write` 的 body 返回一个新分配的对象（旧值再无引用），随后回收
- **THEN** 下一次读取得到新对象、字段完好

#### Scenario: 被丢弃的原语不保留其值
- **WHEN** 创建 `Mutex<T>` 持有一个带 `WeakHandle` 观测的对象，随后丢弃锁并回收
- **THEN** 该对象可被回收（`WeakHandle.Upgrade` 返回 null）

#### Scenario: 跨线程生产者-消费者在 GC 压力下
- **WHEN** 生产者线程经 Channel 发送新分配的对象，消费者线程接收并读字段，两侧同时持续分配以触发回收
- **THEN** 全部对象按序到达、字段完好，无死锁

### Requirement: Monitor 原生原语

内部原语，不对用户公开。句柄是不透明对象，随其回收释放原生状态。

#### Scenario: 互斥
- **WHEN** 多个线程对同一 Monitor 反复 enter → 修改共享计数 → exit
- **THEN** 计数结果等于总次数

#### Scenario: 同线程重入
- **WHEN** 已持有 Monitor 的线程再次阻塞式 enter
- **THEN** 抛异常（可 catch），不死锁；原持有状态不变

#### Scenario: 非持有者退出 / 等待
- **WHEN** 未持有 Monitor 的线程调用 exit 或 wait
- **THEN** 抛异常（可 catch）

#### Scenario: try_enter
- **WHEN** Monitor 被其它线程持有时 try_enter；或被本线程持有时 try_enter
- **THEN** 两种情况都立即返回 false，不阻塞

#### Scenario: wait 释放并重新获取
- **WHEN** 持有者 wait；另一线程 enter → 改状态 → exit
- **THEN** 等待者被唤醒且返回时重新持有 Monitor；等待期间其它线程可以 enter

#### Scenario: 阻塞期间让出 GC safepoint
- **WHEN** 一个线程阻塞在 enter 或 wait 上，另一个线程大量分配触发回收
- **THEN** 回收完成、不死锁（与 #598/#600/#606 同一判据）

## MODIFIED Requirements

### Requirement: 同线程重入

**Before:** `parking_lot` 非重入锁 → 同线程在 `Mutex.Lock` 中再 `Lock`、在 `RwLock.Write` 中再 `Write` 或 `Read`，**永久自死锁**。
**After:** 内层调用抛异常（可 catch），锁仍由外层持有，外层 body 结束后正常释放。
（`Read` 中 `Write`、有写者排队时 `Read` 中再 `Read` 仍会自等——读写锁固有语义，与原实现一致，见 design D5。）

### Requirement: 向已关闭的 Channel Send

**Before:** 抛泛型 `Std.Exception`，消息为 `__channel_send: channel N is closed`。
**After:** 抛 `ChannelDisconnectedException`（与关闭后排空再 `Recv` 一致）。`TrySend` 仍返回 false。

### Requirement: 容量 0 的 Channel（会合）

**Before:** 文档声明「每次 Send 阻塞直到被接收」（`mpsc::sync_channel(0)`），无测试。
**After:** 语义不变，补测试：Send 在对应 Recv 取走该元素后才返回。

### Requirement: `SlotId()` 诊断方法

**Before:** `Mutex<T>` / `RwLock<T>` / `Channel<T>` 各有 `public long SlotId()`，返回原生 registry 槽位 id（「仅供诊断」）。
**After:** 删除——槽位不复存在；全仓无调用方。`Thread.SlotId()` 不受影响。

### Requirement: 负容量 Channel

**Before:** `new Channel<T>(-1)` 抛泛型 `Std.Exception`（`__channel_new_bounded: capacity must be >= 0`）。
**After:** 抛 `ArgumentException`。

## UNCHANGED（必须原样成立）

- `z42.threading/tests/` 下全部现有用例：FIFO、`TryRecv` 判别码 0/1/2、关闭后先排空再抛
  `ChannelDisconnectedException`、有界背压、`TrySend` 满 / 已关闭返回 false、body 抛异常时锁被释放且值不变、
  `TryRead` / `TryWrite` 在被占用时返回 false、阻塞期间让出 safepoint。
- `RwLock.Read` 的 body 看到的是快照：body 内对快照的修改不写回。
- `profile-contention` feature 下 `lock_contended` / `lock_wait_us` 仍统计用户锁的争用。
- 公开 API 签名（`Mutex<T>` / `RwLock<T>` / `Channel<T>` 的 public 成员）除 `SlotId()` 外不变。

## Pipeline Steps

- [ ] Lexer / Parser / TypeChecker / IR Codegen —— 不涉及
- [x] VM：新增 builtin + `NativeData` 变体（interp 与 JIT 共用 builtin 派发，无需分别实现）
- [x] stdlib：`z42.core` 原生声明 + `z42.threading` 三个类重写
