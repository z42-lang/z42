# 原生代码与 GC 的协作约定

> 代码：`src/runtime/src/corelib/`、`src/runtime/src/gc/`、`src/runtime/src/vm_context/`
> 相关：[GC 子系统与 safepoint 协议](gc.md) · [增量 major：SATB 屏障](gc-incremental-major.md) · [同步原语](sync-primitives.md)

## 概述

Rust 写的原生层（builtin、corelib、native 扩展）和 GC 共享一个堆，但 GC 看不见 Rust 的局部变量和容器，
也不会等一个卡在系统调用里的线程。本页讲原生代码要守的四个约定：值放在哪才有根、阻塞调用怎么 park、
写堆引用走什么原语、wasm 上怎么计时。每条都对应一类「编译照过、单测全绿、换个时机就坏」的 bug。

## 机制

### 根集之外不持有 `Value`

GC 只看得见帧寄存器、static 字段、几个 arena 和 pinned roots。把 `Value` 放进任何其它 Rust 侧容器
（`Vec` / `HashMap` / mpsc 队列 / `parking_lot` 锁 / 线程闭包里的局部变量……），它就**没有根**：只要那一刻没有
别的 z42 引用，下一次回收就会收掉它，之后读到 `Null` 或别的对象。典型场景：

- `Thread.Start` 捕获的环境，从 spawn 到进入 worker 帧之间只在 Rust 局部变量里；
- `Mutex` / `RwLock` / `Channel` 的值存在 Rust 侧容器里。

**首选：让值成为 z42 对象的字段**，原生层只提供机制（参照 `corelib/monitor.rs`）——追踪、写屏障、随拥有者回收
全都免费。**确实只能暂存**（跨线程移交这类短窗口）时，用 `pin_root` + RAII 守卫 unpin（参照
`corelib/threading.rs` 的 `SpawnedEnvRoot`），并想清楚「谁拥有它、何时释放」，否则就是泄漏。同步原语的完整设计见
[同步原语](sync-primitives.md)。

### 阻塞的 native 调用必须 park

线程卡在系统调用里就到不了字节码 safepoint；GC 要等「全世界停下」⇒ 一条没 park 的阻塞调用会拖住整个进程的 GC。
判据是「可能长时间不返回」，不是「通常很快」：网络读写 / connect / TLS 握手 / DNS（`getaddrinfo` 可以卡满解析超时）/
子进程管道读写（管道满就阻塞）/ `join` / 条件变量等待。三条，缺一不可：

1. **只把阻塞的那几行包进 `NativeParkGuard`**。
2. **park 区间内不分配、不跑 z42 代码**：parked 线程造的对象不是任何根，并发回收会收掉它（debug 构建由
   `debug_assert_not_native_parked` 当场炸）——错误先收成 Rust `String`，出 park 再造结果。collector 会无锁读 parked
   线程的帧栈，park 期间压 / 弹帧就是数据竞争（`debug_assert_frame_change_not_parked`）；确需回调 z42 先
   `NativeUnparkGuard`。
3. **不攥着共享锁阻塞**：排在这把锁上的其它线程是**不 park** 的——只 park 阻塞者本身，GC 照样要等排队者。
   做法：锁下只取出 / 克隆句柄（`Arc`），放锁，再 park 着做 I/O（参照 `corelib/process.rs` 的 `ProcessSlot`、
   `corelib/network/tcp.rs` 的取出-放回）。

park 还是**线程注册协议的前提**：新 `VmContext` 在标记期不注册、等停顿结束，醒来后可能赢得 collector 角色并等所有
已注册线程 park——若有线程卡在没 park 的调用里就永久死锁（loom 模型 B′，`tests/gc_registration_race_loom.rs`）。
测试里主线程 `join` 一个会触发 GC 的 worker 时同理。

新增会阻塞的 builtin 时，照「阻塞线程 `parked_count == 1` → 解除阻塞 → 回到 0」写单测（参照 `process_tests.rs`、
`monitor_tests.rs`），并做一次阴性对照。

### 写堆引用只走带屏障的原语

把引用写进堆对象 / 数组，一律走 [增量 major「屏障放在哪」](gc-incremental-major.md)那张表里的原语——它们在覆盖前
把旧值交给 SATB 删除屏障。绕过它们，major 标记进行中就可能漏标一个仍被使用的对象。

- **批量写同样是覆盖**（切片拷贝、区间搬移）：先对被覆盖区间整体记录旧值，再写。
- **不带屏障的写只有两种场景**：刚分配、还不可达的块（旧值全是 0），以及 GC 自己给死对象断边。新增这类调用点时
  在旁边写清是哪一种。
- **新增一种「在堆里存引用」的布局**时，读旧值 + 记录必须随写入原语一起加，并照 `gc/arc_heap_tests/incremental.rs`
  补一对「开屏障存活 / 关屏障被扫」的测试。
- **不经强引用把已有堆值交给 mutator 的路径**（弱 / 软引用读取、堆遍历）一律过 `admit_resurrected`；GC 在增量周期里
  追踪已有对象时，清扫期跳过未标记条目。原因见 [增量 major「待清扫的死对象」](gc-incremental-major.md)。

### wasm 上不取时钟

`wasm32-unknown-unknown` 没有 `std::time`：`Instant::now()` / `SystemTime::now()` 直接 panic（「time not implemented
on this platform」），整个 VM trap。编译照过、native 测试全绿，只有 nightly 的浏览器 wasm 测试能照出来。典型场景：
time builtins、GC 计时、采样器构造时取起点（→ 每次加载 zbc 都 trap）。

- **关着的探针不取时钟**：计时起点放进 `Option<Instant>`，开关打开才取（参照 `gc/phase_timer.rs`、`gc/sampler.rs`）。
- **始终要跑的计时**（每次 GC、time builtin）：`#[cfg(target_arch = "wasm32")]` 给替代实现（参照
  `gc/arc_heap/observe.rs` 的单调计数、`corelib/bench.rs`）。
- 只在 native 可达的路径（JIT、子进程、信号处理）不受限。
