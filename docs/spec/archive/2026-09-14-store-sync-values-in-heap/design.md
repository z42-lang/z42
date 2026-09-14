# Design: 同步原语的值存进 GC 堆

## Architecture

```
今天（缺陷）
  z42 Mutex<T> { long _slot } ──slot id──► VmCore.mutexes[id] = Arc<parking_lot::Mutex<Value>>
                                                                       └─ Value：不在任何根里 ✗
  z42 Channel<T> { long _slot } ──────────► VmCore.channels[id] = mpsc 队列<Value>          ✗
  registry 只增不减 → 槽位永不释放

之后
  z42 Mutex<T>   { object _mon; T _value }                         ─┐
  z42 RwLock<T>  { object _mon; T _value; int _readers; int _writersWaiting }
  z42 Channel<T> { object _mon; T[] _buf; int _head; int _count; int _cap; bool _closed; long _sent; long _taken }
                    │                    └─ 普通字段：GC 按对象图追踪，写入走常规写屏障 ✓
                    ▼
  句柄对象（合成 TypeDesc `Std.Threading.MonitorHandle`，无字段）
    NativeData::Monitor(Arc<Monitor>)
                    ▼
  Monitor { state: parking_lot::Mutex<State{ owner: Option<ThreadId>, waiters: u32 }>, cv: Condvar }
    └─ 不含任何 Value；句柄对象被回收后随槽位复用 drop
```

## Decisions

### D1: 让值「随拥有者可达」——字段化，而不是给原生数据加追踪钩子

**问题**：User 选定的方向是「原语持有的值随拥有它的 z42 对象存活，不泄漏」。有两种实现。

**选项**：
- A — **保留原生容器，给 GC 加钩子**：新增 `NativeData::Sync(..)`，`trace_children` 访问其中的 `Value`；
  存入时补写屏障（分代卡表 + 并发标记队列）；`mpsc` 不可遍历，须换自研队列；清扫时释放。
  缺点：要改 GC 的追踪 / 清扫 / 屏障三处核心路径（`gc/arc_heap/generational.rs` 等，另一条在飞的
  GC 分支正在改这些文件），且把「原生容器里的值」变成 GC 需要特判的第二种对象图。
- B — **把值变成 z42 字段，原生层只留同步**：Mutex / RwLock 的值、Channel 的缓冲区都是对象字段；
  原生层提供一个不含值的 Monitor。追踪、写屏障、回收全部复用常规对象模型，**GC 零改动**。
  缺点：Channel / RwLock 的逻辑从 Rust 挪到 z42；每次操作多几次 builtin 调用。

**决定**：选 **B**。它从结构上消灭这类缺陷（原生层没有值可漏），而不是再多维护一个需要被正确追踪的
值容器；#617（spawn 窗口里捕获环境无根）与本缺陷是同一类——原生侧持有 `Value` 却不在根集——B 顺势
确立「原生层不持有 `Value`」这条不变量，写入 `runtime-rust.md`。

### D2: 原生原语只要一个 Monitor

**问题**：Mutex 要互斥，RwLock 要共享 / 独占，Channel 要「空则等 / 满则等」。

**选项**：
- A — 三个原生原语（raw mutex + raw rwlock + condvar）。
- B — 一个 **Monitor**（enter / try_enter / exit / wait），三者都用 z42 在其上实现。

**决定**：选 **B**。RwLock 的读者计数、Channel 的缓冲区管理都只需要「互斥 + 条件等待」；一个原语意味着
只有一处需要把「阻塞 + safepoint + 内部锁顺序」写对（见 D4），也去掉了今天
`mem::forget(guard)` + `force_unlock` + 线程局部 `HELD_*_GUARDS` 这套 unsafe 簿记。
代价是 `RwLock.Read` 从 2 次 builtin 变成 4 次（进出 Monitor 各两次）——用 `06_thread_scaling` 前后对比量出来，
写进验证报告；若退化不可接受再议（不预先优化）。

### D3: 句柄生命周期——挂在对象上，不进 registry

**问题**：今天的槽位 id 登记在 `VmCore` 的 registry 里，从不删除。

**决定**：`__monitor_new` 返回一个合成类型的句柄对象（前例：`weak_handle_type_desc`），原生状态放在
`NativeData::Monitor(Arc<Monitor>)`。对象清扫时只打墓碑，`NativeData` 在该槽位被**复用时**随旧条目 drop
（`gc/region.rs` 的条目覆盖写）。延迟释放对 Monitor 无害：它不含 GC 值，只是几十字节的 Rust 内存；
正在阻塞的线程持有自己克隆出来的 `Arc`，不依赖句柄存活。**不改清扫路径。**

### D4: 阻塞、safepoint 与内部锁的顺序（最容易写错的地方）

Monitor 的「持有」是 z42 层语义（`owner`），内部 `parking_lot::Mutex<State>` 只在 O(1) 临界区里持有。

**铁律**：
1. **阻塞必须处于 `NativeParkGuard` 之内**（否则另一线程发起 GC 时等不到本线程 → 死锁，#598/#600/#606 同判据）。
2. **离开 park（`NativeParkGuard` drop，可能要等 STW 结束）之前必须先释放内部锁。**
   反例：线程 A 在 STW 期间带着内部锁等 GC 结束；线程 B 正在 `exit` 里抢内部锁——B 不在 safepoint 也没 park，
   GC 等 B、B 等 A、A 等 GC ⇒ 死锁。
3. **进入阻塞前先把 `Arc<Monitor>` 从句柄对象里克隆出来，并释放对象借用。** 持着 `GcRef` 借用（条目锁）
   去阻塞，会挡住 GC 对该对象的标记 / 清扫。

```rust
fn enter(ctx, mon: &Monitor) -> Result<()> {
    let me = current_thread_id();
    // 快路径：不 park（park 的进出要拿全局 phase 锁并 notify_all，不能每次加锁都付）
    {
        let mut st = mon.state.lock();          // 只被 O(1) 临界区持有，短暂等待不需要 park
        if st.owner == Some(me) { bail!("Monitor is not reentrant") }
        if st.owner.is_none() { st.owner = Some(me); return Ok(()) }
    }
    // 慢路径：争用
    let _park = NativeParkGuard::enter(ctx);    // 先声明 → 后 drop
    let mut st = mon.state.lock();              // 后声明 → 先 drop（满足铁律 2）
    st.waiters += 1;
    while st.owner.is_some() { mon.cv.wait(&mut st); }
    st.waiters -= 1;
    st.owner = Some(me);
    Ok(())
}                                               // st 先释放，_park 后退出

fn exit(mon) -> Result<()> {
    let mut st = mon.state.lock();
    if st.owner != Some(current_thread_id()) { bail!("Monitor exit by non-owner") }
    st.owner = None;
    if st.waiters > 0 { mon.cv.notify_all(); }  // 无人等待时不发唤醒
    Ok(())
}

fn wait(ctx, mon) -> Result<()> {           // 释放 → 等一次唤醒 → 重新获取
    let me = current_thread_id();
    let _park = NativeParkGuard::enter(ctx);
    let mut st = mon.state.lock();
    if st.owner != Some(me) { bail!("Monitor wait by non-owner") }
    st.owner = None;
    st.waiters += 1;
    mon.cv.notify_all();                        // 让等 enter 的线程有机会进来
    mon.cv.wait(&mut st);                       // 任意一次 exit 都会唤醒——调用方自己循环复查条件
    while st.owner.is_some() { mon.cv.wait(&mut st); }
    st.waiters -= 1;
    st.owner = Some(me);
    Ok(())
}
```

> **实施细化**（2026-09-14）：① 「等进入」与「等信号」拆成两个条件变量——共用一个时两个同时 `wait` 的线程
> 会互相唤醒空转（退回对照实测 100ms 空转 42149 次）；② `enter` 快路径失败后先自旋 40 轮（32 忙等 + 8 yield）再 park
> ——跨线程有界 Channel 从旧实现的 2.15 倍降到 1.60 倍；给 `wait` 加自旋无可测收益，未保留。

`wait` 允许伪唤醒：z42 侧永远写成 `while (!条件) Wait()`。因为「释放 owner」与「开始等待」在同一次内部锁
持有内完成，发送方必须拿到内部锁才能改状态并 `exit`，所以不会丢唤醒。

### D5: z42 侧三个原语的形状

```z42
// Mutex<T>
public void Lock(Func<T, T> body) {
    MonitorNative.Enter(this._mon);
    try { this._value = body(this._value); }      // body 抛异常 → 不赋值，值不变
    finally { MonitorNative.Exit(this._mon); }
}

// RwLock<T>：写者在 body 期间**持有 Monitor**（与 Mutex 同形）；读者只在进出时短暂持有
public void Write(Func<T, T> body) {
    MonitorNative.Enter(this._mon);                // 同线程重入 → Monitor 判定抛异常
    try {
        this._writersWaiting = this._writersWaiting + 1;
        while (this._readers > 0) { MonitorNative.Wait(this._mon); }
        this._writersWaiting = this._writersWaiting - 1;
        this._value = body(this._value);           // body 抛异常 → 不赋值
    } finally { MonitorNative.Exit(this._mon); }
}
public void Read(Action<T> body) {
    MonitorNative.Enter(this._mon);                // 写者 body 进行中 → 阻塞在这里
    while (this._writersWaiting > 0) { MonitorNative.Wait(this._mon); }   // 写者优先
    this._readers = this._readers + 1;
    T snapshot = this._value;
    MonitorNative.Exit(this._mon);
    try { body(snapshot); }                        // 读者 body 不持 Monitor → 多读者并发
    finally {
        MonitorNative.Enter(this._mon);
        this._readers = this._readers - 1;
        MonitorNative.Exit(this._mon);             // 有等待者时唤醒写者
    }
}
// TryWrite：TryEnter 失败 → false；_readers > 0 → Exit 后 false；否则同 Write（不等待）
// TryRead：TryEnter 失败（写者进行中）→ false；_writersWaiting > 0 → Exit 后 false；否则同 Read

// Channel<T>：环形缓冲 _buf，_cap = -1 无界（满则扩容）、>0 有界、0 会合（缓冲区至少 1 格）
// Send:  Enter; 若 _closed → Exit 后抛 ChannelDisconnectedException
//        while (有界 && _count >= _cap) Wait;  入队; long my = ++_sent;
//        若 _cap == 0: while (_taken < my) Wait;   // 会合：等到被取走
//        Exit
// Recv:  Enter; while (_count == 0 && !_closed) Wait;
//        若 _count == 0 → Exit 后抛 ChannelDisconnectedException
//        出队并把该格写回 default(T)（不让已出队元素被缓冲区拖住）; _taken++; Exit
```

「**同线程重入**」的判定只在 Monitor 一处（D4 的 `owner == me`），覆盖：`Lock` 中再 `Lock`、`Write` 中再
`Write`、`Write` 中 `Read`（原先都是永久自死锁）。**不覆盖**的两种与原 `parking_lot` 相同、属读写锁固有语义：
`Read` 中 `Write`（等自己的读者退出）、有写者排队时 `Read` 中再 `Read`（写者优先 ⇒ 等写者）。
机制页写明，不在本变更改动。

> 为什么写者 body 期间持有 Monitor 不会饿死读者：读者 body 不持 Monitor，只在进出计数时短暂持有；
> 写者只在「没有读者」时才进入 body。持 Monitor 执行 body 的代价是同一 RwLock 上的读者进出计数要等写者 body 结束——
> 这正是写锁本来的语义。

### D6: 两阶段落地

旧的 19 个 builtin 在 `z42.core` 的种子 zpkg 里有 `[Native]` 声明，而 builtin id 是**加载时**按名解析的。
当前 VM 删掉它们会不会让冷启动加载旧种子失败，本地无法可靠验证；按 `bootstrap-seed.md` 纪律走两阶段：

1. **本变更**：新增 `__monitor_*`；新 stdlib 源码删除旧 `[Native]` 声明、不再引用旧 builtin；
   旧 builtin、`VmCore.{mutexes,channels,rwlocks}`、`sync.rs` 原样保留（模块头注释标明是种子例外）。
2. **Deferred `store-sync-values-in-heap-remove-legacy`**：本变更进 nightly 后，按
   `stdlib-interop` 程序记下的方法（下载 nightly SDK → `strings` 后 grep）确认种子不再引用，再删。

## Implementation Notes

- **不碰 GC 目录**：`gc/` 下零改动（D1/D3）。与在飞的 GC 分支无文本冲突；语义耦合由合并前 rebase + GREEN 兜底。
- **`sync.rs` 已 590 行**，新代码全部放 `monitor.rs`，不往 `sync.rs` 里加。
- **争用探针**：`profile-contention` feature 下，慢路径（进入 park 之前）计 `lock_contentions`、计时 `lock_wait_us`；
  默认构建零开销。
- **JIT**：builtin 在 interp / JIT 共用派发表，无需分别实现；测试两种模式都跑。
- **句柄取 `Arc`**：`args[0]` 必须是 `Value::Object` 且 `native()` 为 `Monitor`，否则 bail（类型化错误，不 panic）。
- **`default(T)` 清槽**：已确认 z42 泛型支持（`Array.z42:196`）。
- **无界扩容**：新数组容量翻倍，按 `_head` 起按序拷贝，拷完 `_head = 0`。

## Testing Strategy

- **退回对照（必做）**：新增的 `gc_sync_values_rooted.z42` 先在**当前 main 的 VM + stdlib** 上跑，必须失败
  （记录失败输出），修复后通过——证明测试有判别力，不是碰巧绿。
- **Rust 单测**（`monitor_tests.rs`）：spec「Monitor 原生原语」逐条；阻塞让出 safepoint 用
  「一线程阻塞、另一线程 `collect`」形状，带超时判死锁。
- **stdlib 测试**：`z42.threading` 全部现有用例 + 两个新文件，interp 与 `--mode jit` 各跑一遍。
- **debug VM**：三种原语的回归用例在 debug VM 下跑，不得出现 `use-after-finalize`。
- **flake 工作台**：`test stdlib z42.net --no-build` 串行 ≥12 轮，前后对比（本变更只声称修 pool 路径，
  如实记录 threaded 是否仍挂）。
- **性能**：`06_thread_scaling` 前后各跑，记录进验证报告。
- **GREEN**：`xtask test` 全 stage + 全量 `cargo test`（改 runtime 必跑全量，非只 `--lib`）。
