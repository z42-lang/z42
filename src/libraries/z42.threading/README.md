# z42.threading — 线程库

## 职责

OS 线程级并发原语 — 用户层 `Thread.Start(Action) / Join()` API。底层通过
`__thread_spawn` / `__thread_join` builtin 接入 runtime 的 `VmCore.threads`
slot table。

`Mutex<T>` / `RwLock<T>` / `Channel<T>` **用 z42 写在 `MonitorNative` 之上**，值 / 队列缓冲区是对象字段——
GC 可见、随对象回收。原生层只提供不含值的 Monitor（store-sync-values-in-heap, 2026-09-14；此前值存在
Rust 侧容器里，GC 看不见，只被原语持有的值会被回收）。机制见
[docs/book/src/runtime/sync-primitives.md](../../../docs/book/src/runtime/sync-primitives.md)。

## src/ 核心文件

| 文件 | 类型 | 说明 |
|------|------|------|
| `Thread.z42` | `Std.Threading.Thread` | OS 线程句柄；`Start(Action)` 工厂 + `Join()` 同步等待 + 静态 `Sleep(long millis)` 阻塞当前线程 |
| `ThreadException.z42` | `Std.ThreadException` | 跨线程异常封装（worker `throw` 或 Rust panic 经由 Join 透传） |
| `Mutex.z42` | `Std.Threading.Mutex<T>` | 排他互斥；RAII callback `Lock(Func<T,T>)`；同线程重入抛异常（add-sync-primitives 2026-05-20） |
| `RwLock.z42` | `Std.Threading.RwLock<T>` | 多读单写 lock；`Read(Action<T>)` 多 reader 并发 + `Write(Func<T,T>)` 单 writer 排他、写者优先（add-sync-primitives-rwlock 2026-05-20） |
| `Channel.z42` | `Std.Threading.Channel<T>` | 多生产者多消费者 FIFO：unbounded (`new Channel<T>()`)、bounded with back-pressure (`new Channel<T>(N)`) 或会合 (`new Channel<T>(0)`)；`Send` / `Recv` / `TryRecv` / `TrySend` / `Close`（add-sync-primitives 2026-05-20 + bounded 扩展 2026-05-20） |
| `ChannelDisconnectedException.z42` | `Std.ChannelDisconnectedException` | 所有 sender 关闭且队列空时 `Recv()` 抛出 |

## 入口点

```z42
using Std.Threading;

// Spawn / Join
var t = Thread.Start(() => {
    Console.WriteLine("hello from worker");
});
t.Join();

// Mutex — RAII callback；锁内 (long v) => v + 1 读改写自动 unlock
var counter = new Mutex<long>(0);
counter.Lock((long v) => v + 1);

// Channel — 跨线程 FIFO 队列
var c = new Channel<long>();
Thread.Start(() => { c.Send(42); c.Close(); });
long v = c.Recv();
```

## 依赖关系

仅依赖 `z42.core`（异常基类 + delegate `Action` / `Func`）。
