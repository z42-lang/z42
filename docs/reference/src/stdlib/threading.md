<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.threading/ -->

# z42.threading —— OS 线程与同步原语。

> 包路径 `src/libraries/z42.threading/`；命名空间 `Std.Threading`

真 OS 线程的并发原语：`Thread` 起停、`Channel<T>` 传值、`Mutex<T>` / `RwLock<T>` 护住共享
状态、`Timer` 跑后台周期任务。线程之间共享 GC 堆与静态字段，但 lambda 捕获的是值快照
——跨线程传值只能走静态字段、共享对象、数组单元或 `Channel<T>`。

这是 z42 唯一的并发设施：没有 async / await、没有 Task、没有线程池。每次 `Thread.Start`
都真的开一条 OS 线程，成本按 OS 线程算。

本包随工具链分发、自动可用，工程清单里不需要声明依赖（声明了会报 W0610）。

## 不支持

线程：

- 没有线程池、没有 async / await / Task：`Thread.Start` 就是一条真 OS 线程。
- `Action` 之外的入口形态都没有：worker 不能带参数、不能返回值。要回传结果得走静态字段、
  共享对象或 `Channel<T>`。
- 没有线程名、优先级、IsAlive、Thread.CurrentThread、线程 id 查询（`SlotId()` 只是
  句柄自己的诊断编号，不是「当前线程是谁」）。
- 不能取消、不能中断、不能强杀：`Thread` 上没有 Cancel / Interrupt / Abort。协作式
  取消要自己用共享的 bool[1] 单元或 Channel 实现。
- `Join` 没有超时重载，也没有 TryJoin；`Join` 一旦开始等就只能等到 worker 结束。
- 没有守护线程 / detach 语义：进程随主线程返回而结束，未 Join 的线程被直接终止。
- 没人 `Join` 的 worker 异常被静默吞掉，没有任何全局「未捕获异常」钩子。
- 异常不保真：跨线程只带 Message 字符串，原类型与栈丢失。
- `Thread.Sleep` 是毫秒精度，没有纳秒 / Yield / 自旋提示。

同步：

- 语言没有 lock 语句，也没有面向任意对象的裸锁用户 API——临界区一律经
  `Mutex<T>` / `RwLock<T>` 的回调体。（`using Std.Threading` 还会带进 `ThreadNative` /
  `MonitorNative` 两个名字，它们是 `z42.core` 的 extern 声明层，不是本包的用户 API。）
- 没有 Interlocked / 原子类型 / volatile：一个 long 的原子自增也得用 Mutex<long>。
- 没有信号量、条件变量、屏障、倒计时事件、ManualResetEvent、读写升级锁。
- `Mutex<T>` 没有 TryLock（`RwLock<T>` 有 TryRead / TryWrite，Mutex 没有对应物）。
- 没有带超时的获取：Try* 只有「立刻成功或立刻失败」两态。
- 不可重入：重入抛 LockRecursionException，不是递归计数。
- Lock / Read / Write 都没有返回值：读出来的值必须靠静态字段或数组单元带出回调体。
- `RwLock<T>.Read` 的「只读」不由类型系统保证：T 是引用类型时，读体里改字段会真的改到共享值。

通道：

- 没有带超时的 Recv，也没有 Recv 的取消。要么阻塞到底，要么用 TryRecv + Sleep 轮询。
- 没有 select / 多路复用：无法同时等多个通道。
- 没有 Count / IsClosed / Capacity 查询，也不能 foreach 遍历通道。
- 没有 sender / receiver 分离句柄：谁拿到通道谁都能收发，也就没有「最后一个 sender 析构
  即自动断开」——只有显式 Close() 能让接收方收工。
- 无界通道没有内存上限：生产快于消费就一直扩容。要背压就用有界构造器。
- 会合通道（capacity == 0）上已经阻塞的 Send 不会被 Close() 唤醒：那个值必须被某个
  接收者取走，Send 才会返回，否则该线程一直挂着。关闭会合通道前要先确保在途的值有人接。

定时器：

- 没有 dueTime + period 的组合：StartPeriodic 的首次触发固定在 +interval；想要「立刻
  跑一次再周期跑」得自己先调一次回调。
- 没有 Change / 重启 / 改周期：停了就得重新 Start。
- 没有 state 参数，回调是无参 Action，上下文靠闭包捕获的对象带。
- StopAndJoin 不幂等：第二次调用抛 ThreadException("thread already joined")。
- 回调异常不会上报给调用方，只有一行 Log.Error。

## Channel

`src/libraries/z42.threading/src/Channel.z42:5`

多生产者 / 多消费者 FIFO 队列。三种容量形态（无界 / 有界 / 会合）由构造器决定，之后不能改。

```z42
public class Channel<T> {
    Channel()
    Channel(int capacity)
    void Send(T v)
    T Recv()
    object[] TryRecv()
    void Close()
    bool TrySend(T v)
}
```

### Channel()

`src/libraries/z42.threading/src/Channel.z42:17`

无界构造器（默认）。队列按需扩容，没有上限；要背压用有界构造器。

### Channel(int capacity)

`src/libraries/z42.threading/src/Channel.z42:24`

有界构造器。capacity > 0 有界（Send 在队列满时阻塞直到腾出位置）；capacity == 0 会合 （每个 Send 阻塞直到这个值被另一条线程取走）。

抛 `ArgumentException`：capacity < 0 时。

### void Send(T v)

`src/libraries/z42.threading/src/Channel.z42:47`

按容量形态阻塞地发送 v。通道已关闭、或等待期间被关闭 → 抛 ChannelDisconnectedException("channel closed")。

抛 `ChannelDisconnectedException`：通道已关闭或等待期间被关闭时。

### T Recv()

`src/libraries/z42.threading/src/Channel.z42:73`

阻塞到有值可取。通道已关闭且队列已空时抛 ChannelDisconnectedException("channel disconnected")； 队列里还有值就照常取，关闭不丢数据。

抛 `ChannelDisconnectedException`：通道已关闭且队列已空时。

### object[] TryRecv()

`src/libraries/z42.threading/src/Channel.z42:97`

不阻塞。返回判别式数组：[0, value]（取到值）/ [1]（暂时没值）/ [2]（已关闭且空）。 判别位是 long，用 (long)r[0] 取。

### void Close()

`src/libraries/z42.threading/src/Channel.z42:119`

幂等关闭通道，不可撤销。已入队的值仍可取完；唤醒阻塞中的 Recv 和等位置的 Send。

### bool TrySend(T v)

`src/libraries/z42.threading/src/Channel.z42:130`

不阻塞发送，不抛异常。有界满、通道已关闭返回 false；会合通道只有在已有接收者阻塞在 Recv 上时才返回 true。

## ChannelDisconnectedException

`src/libraries/z42.threading/src/ChannelDisconnectedException.z42:5`

通道断开异常：对已关闭（或等待期间被关闭）的通道 Send，或通道已关闭且队列已空时 Recv。 TrySend / TryRecv 永远不抛这个异常，用返回值表达同样的状态。

```z42
public class ChannelDisconnectedException : Exception {
    ChannelDisconnectedException(string message)
}
```

### ChannelDisconnectedException(string message)

`src/libraries/z42.threading/src/ChannelDisconnectedException.z42:7`


| 参数 | 说明 |
|---|---|
| `message` | 触发消息（"channel closed" / "channel disconnected"）。 |

## Mutex

`src/libraries/z42.threading/src/Mutex.z42:6`

锁包着数据的互斥原语：被保护的值放在 Mutex<T> 内部，只有 Lock 的回调体能碰到它。 没有 Acquire / Release，Lock 自己配对；不可重入，重入抛 LockRecursionException。

```z42
public class Mutex<T> {
    Mutex(T initial)
    void Lock(Func<T,T> body)
}
```

### Mutex(T initial)

`src/libraries/z42.threading/src/Mutex.z42:12`

用 initial 作为初始被保护值。

### void Lock(Func<T,T> body)

`src/libraries/z42.threading/src/Mutex.z42:19`

取锁 → 用当前值调用 body → 把 body 的返回值写回 → 放锁。整段对其他线程的 Lock 是原子的。 没有返回值：想把锁内读到的值带出来，在 body 里写静态字段或 1 元数组单元。

## RwLock

`src/libraries/z42.threading/src/RwLock.z42:8`

多读单写的读写锁，同样是「锁包着数据」的回调形态。读者并发跑，写者独占，写者优先。

不可重入：Write 里套 Write、Write 里套 Read 抛 LockRecursionException；Read 里套 Write、
以及夹在写者后面的 Read 里套 Read 会自死锁——不要在同一个 RwLock 上嵌套获取。

```z42
public class RwLock<T> {
    RwLock(T initial)
    void Read(Action<T> body)
    void Write(Func<T,T> body)
    bool TryRead(Action<T> body)
    bool TryWrite(Func<T,T> body)
}
```

### RwLock(T initial)

`src/libraries/z42.threading/src/RwLock.z42:17`

用 initial 作为初始被保护值。

### void Read(Action<T> body)

`src/libraries/z42.threading/src/RwLock.z42:25`

共享获取：body 拿到当前值的一份快照。body 没有返回值，改不了存储的值。

### void Write(Func<T,T> body)

`src/libraries/z42.threading/src/RwLock.z42:45`

独占获取（挡住所有读者和其他写者）：body 的返回值成为新的存储值。

### bool TryRead(Action<T> body)

`src/libraries/z42.threading/src/RwLock.z42:63`

非阻塞共享获取：拿不到就返回 false 且不调用 body；成功时 body 已跑完、读已释放，返回 true。

### bool TryWrite(Func<T,T> body)

`src/libraries/z42.threading/src/RwLock.z42:90`

非阻塞独占获取：有任何读者或写者占着就返回 false 且不调用 body；成功时返回 true，值已写回。

## Thread

`src/libraries/z42.threading/src/Thread.z42:65`

OS 线程句柄。只能用静态工厂 Start 创建（构造器私有），句柄本身只带一个 slot id。

```z42
public class Thread {
    static Thread Start(Action action)
    void Join()
    long SlotId()
    static void Sleep(long millis)
}
```

### static Thread Start(Action action)

`src/libraries/z42.threading/src/Thread.z42:75`

立刻开一条 OS 线程跑 action 并返回句柄。

| 参数 | 说明 |
|---|---|
| `action` | 只能是无参无返回的 Action——不能带参数、不能返回值。 |

### void Join()

`src/libraries/z42.threading/src/Thread.z42:82`

阻塞直到 worker 结束。

抛 `ThreadException`：worker 抛过异常、或对同一个 Thread 第二次 Join 时。

### long SlotId()

`src/libraries/z42.threading/src/Thread.z42:87`

进程内单调递增的诊断用编号（第一条线程是 1）。不要依赖具体数值。

### static void Sleep(long millis)

`src/libraries/z42.threading/src/Thread.z42:93`

阻塞当前线程 millis 毫秒。毫秒精度；负数按 0 处理。

| 参数 | 说明 |
|---|---|
| `millis` | 要阻塞的毫秒数；负数按 0 处理。 |

## ThreadException

`src/libraries/z42.threading/src/ThreadException.z42:5`

由 Thread.Join()（以及 Timer.StopAndJoin，它内部就是 Join）抛出的跨线程异常。 worker 的原始异常类型与栈不跨线程——只能 catch (ThreadException)，拿到 Message 原文。

```z42
public class ThreadException : Exception {
    ThreadException(string message)
}
```

### ThreadException(string message)

`src/libraries/z42.threading/src/ThreadException.z42:7`


| 参数 | 说明 |
|---|---|
| `message` | 触发消息（worker 异常原文 / "thread panicked" / "thread already joined"）。 |

## Timer

`src/libraries/z42.threading/src/Timer.z42:6`

后台回调调度器，一次性或周期性，跑在自己的 OS 线程上。sealed，只能用两个静态工厂创建。

```z42
public sealed class Timer {
    static Timer StartPeriodic(long intervalMs, Action callback)
    static Timer StartOnce(long delayMs, Action callback)
    void Stop()
    void StopAndJoin()
    bool IsRunning()
}
```

### static Timer StartPeriodic(long intervalMs, Action callback)

`src/libraries/z42.threading/src/Timer.z42:27`

每 intervalMs 毫秒触发一次 callback，首次触发在 +interval（不是立刻）。一次回调跑完才开始下一轮等待。

抛 `ArgumentException`：intervalMs <= 0 时。

### static Timer StartOnce(long delayMs, Action callback)

`src/libraries/z42.threading/src/Timer.z42:41`

delayMs 之后触发一次 callback 然后自动停。delayMs < 0 抛 ArgumentException；0 合法（尽快触发）。

抛 `ArgumentException`：delayMs < 0 时。

### void Stop()

`src/libraries/z42.threading/src/Timer.z42:54`

只发停止信号立刻返回，幂等；正在跑的回调会跑完。

### void StopAndJoin()

`src/libraries/z42.threading/src/Timer.z42:60`

发信号并等后台线程退出。不可重复调用——第二次抛 ThreadException("thread already joined")。

抛 `ThreadException`：第二次调用时。

### bool IsRunning()

`src/libraries/z42.threading/src/Timer.z42:66`

Stop / StopAndJoin 调过之后为 false；StartOnce 触发过之后也是 false。

