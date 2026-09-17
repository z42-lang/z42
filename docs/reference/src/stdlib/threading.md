# z42.threading —— OS 线程与同步原语

> 对齐：2026-09-17（change `restructure-docs-three-books`）；
> 包路径 `src/libraries/z42.threading/`；命名空间 `Std.Threading`
> （`ThreadException` / `ChannelDisconnectedException` 在 `Std`）

真 OS 线程的并发原语：`Thread` 起停、`Channel<T>` 传值、`Mutex<T>` / `RwLock<T>` 护住共享
状态、`Timer` 跑后台周期任务。线程之间**共享 GC 堆与静态字段**，但 lambda 捕获的是**值快照**
——跨线程传值只能走静态字段、共享对象、数组单元或 `Channel<T>`（见下「线程之间怎么传值」）。

这是 z42 唯一的并发设施：**没有 async / await、没有 Task、没有线程池**。每次 `Thread.Start`
都真的开一条 OS 线程，成本按 OS 线程算。

用之前要在工程清单里声明依赖：

```toml
[dependencies]
"z42.threading" = "0.1.0"
```

## `Thread`

OS 线程句柄。只能用静态工厂 `Start` 创建（构造器私有），句柄本身只带一个 slot id。

```z42
namespace Std.Threading;

public class Thread {
    public static Thread Start(Action action)
    public void Join()
    public long SlotId()
    public static void Sleep(long millis)
}
```

| 成员 | 说明 |
|---|---|
| `Start` | 立刻开一条 OS 线程跑 `action` 并返回句柄。`action` 只能是无参无返回的 `Action`——**不能带参数、不能返回值** |
| `Join` | 阻塞直到 worker 结束。worker 正常结束则返回；worker 抛过异常则在这里抛 `Std.ThreadException`；**第二次 `Join` 抛 `ThreadException("thread already joined")`** |
| `SlotId` | 进程内单调递增的诊断用编号（第一条线程是 `1`）。不要依赖具体数值 |
| `Sleep` | 阻塞**当前**线程 `millis` 毫秒。毫秒精度；负数按 `0` 处理（`Sleep(-100)` 立即返回） |

线程与主线程共享 GC 堆、静态字段、已加载的包与 native 库；异常状态与调用栈是每线程独立的。

两条进程级行为：

- **主线程从 `Main` 返回 = 进程结束**，还没 Join 的线程会被直接终止，不等它们跑完
  （实测：起一条 `Sleep(5000)` 的线程后立刻返回 `Main`，进程 1.4 秒就退了）。要等就 `Join`。
- **没人 `Join` 的 worker 抛出的异常被静默丢弃**——不打印、不影响退出码。异常只有经 `Join`
  才会浮出来。

### 线程之间怎么传值

lambda 捕获值类型（`long` / `bool` / `string` 等局部变量）时拿的是**创建那一刻的快照**，
worker 里对它赋值不会传回外层，外层在 `Start` 之后改它也不影响 worker。详见
[闭包与捕获](../language/closures.md)。可用的通道有四条：

| 手法 | 适用 |
|---|---|
| 静态字段 | 最直白；跨线程读写要自己加锁或用 `Mutex<T>` |
| 共享对象的字段 | 捕获的是引用，字段读写双向可见 |
| 1 元数组单元（`long[1]` / `bool[1]`） | 想把一个值「带出」lambda 时最轻的写法；`Timer` 内部就用它 |
| `Channel<T>` | 需要排队 / 背压 / 结束信号时 |

```z42
long[] cell = new long[1];
var t = Thread.Start(() => { cell[0] = 55; });   // 写数组元素：可见
t.Join();
// cell[0] == 55

long local = 0;
var t2 = Thread.Start(() => { local = 99; });    // 写捕获的局部变量：不可见
t2.Join();
// local 仍是 0
```

## `Channel<T>`

多生产者 / 多消费者 FIFO 队列。三种容量形态由构造器决定，之后不能改。

```z42
namespace Std.Threading;

public class Channel<T> {
    public Channel()                 // 无界
    public Channel(int capacity)     // capacity > 0 有界；capacity == 0 会合

    public void Send(T v)
    public T Recv()
    public object[] TryRecv()
    public bool TrySend(T v)
    public void Close()
}
```

| 形态 | 构造 | `Send` 什么时候阻塞 |
|---|---|---|
| 无界 | `new Channel<long>()` | 从不阻塞，队列按需扩容（**没有上限，慢消费者会把内存吃光**） |
| 有界 | `new Channel<long>(N)` | 队列里已有 N 个值时阻塞，直到有人 `Recv` 腾出位置（背压） |
| 会合 | `new Channel<long>(0)` | 每次 `Send` 都阻塞，直到**这个值**被另一条线程取走 |

`capacity < 0` 抛 `ArgumentException("Channel: capacity must be >= 0, got -1")`。

| 成员 | 说明 |
|---|---|
| `Send` | 按上表阻塞。通道已关闭、或等待期间被关闭 → 抛 `ChannelDisconnectedException("channel closed")` |
| `Recv` | 阻塞到有值可取。**通道已关闭且队列已空**时抛 `ChannelDisconnectedException("channel disconnected")`；队列里还有值就照常取，关闭不丢数据 |
| `TryRecv` | 不阻塞。返回判别式数组：`[0, value]`（长度 2，取到值）/ `[1]`（长度 1，暂时没值）/ `[2]`（长度 1，已关闭且空）。判别位是 `long`，用 `(long)r[0]` 取 |
| `TrySend` | 不阻塞，**不抛异常**。有界满、通道已关闭都返回 `false`；会合通道只有在已经有接收者阻塞在 `Recv` 上时才返回 `true` |
| `Close` | 幂等，可重复调用，**不可撤销**。已入队的值仍可取完；唤醒阻塞中的 `Recv` 与「等位置」的 `Send` |

FIFO 只保证单通道内的入队顺序。多个消费者并发 `Recv` 时，每个值恰好交给一个消费者，
但分配比例不保证（实测 100 个值在两个消费者间分成了 2232 / 2818 的和）。

`TryRecv` 的读法：

```z42
object[] r = c.TryRecv();
long kind = (long)r[0];
if (kind == 0) {
    long v = (long)r[1];      // 取到值
} else if (kind == 1) {
    // 暂时为空，稍后再试
} else {
    // kind == 2：已关闭且取空，不用再等了
}
```

## `Mutex<T>`

**锁包着数据**（Rust 风），不是裸锁：被保护的值放在 `Mutex<T>` 内部，只有回调体里能碰到它。
没有 `Acquire` / `Release`，也不需要 `using`——`Lock` 自己配对。

```z42
namespace Std.Threading;

public class Mutex<T> {
    public Mutex(T initial)
    public void Lock(Func<T, T> body)
}
```

| 成员 | 说明 |
|---|---|
| `Lock` | 取锁 → 用当前值调用 `body` → 把 `body` 的返回值写回 → 放锁。整段对其他线程的 `Lock` 是原子的 |

要点：

- **`body` 抛异常时值不变**（写回被跳过），锁照样释放，异常原样传给 `Lock` 的调用方。
- **不可重入**：同一条线程在 `body` 里再次 `Lock` 同一个实例会抛异常——基类 `Std.Exception`，
  消息 `Monitor is not reentrant: the calling thread already holds it`（不是死锁，也没有专门
  的异常类型可 catch）。外层 `Lock` 的写回不受影响。
- **`Lock` 没有返回值**：想把锁内读到的值带出来，在 `body` 里写静态字段或 1 元数组单元。
- **`T` 是引用类型时锁保护的是那个引用**：`body` 里通过引用改对象字段照样生效，对象本身
  能不能被别处并发访问由你自己保证。

```z42
var counter = new Mutex<long>(0);
counter.Lock((long v) => v + 1);        // 读改写，原子

long[] cell = new long[1];              // 把当前值读出来
counter.Lock((long v) => { cell[0] = v; return v; });
```

## `RwLock<T>`

多读单写，同样是「锁包着数据」的回调形态。读者并发跑（实测 4 条线程可同时在 `Read` 体内），
写者独占，**写者优先**：有写者在排队时新来的读者要等，读多写少不会饿死写者。

```z42
namespace Std.Threading;

public class RwLock<T> {
    public RwLock(T initial)
    public void Read(Action<T> body)
    public void Write(Func<T, T> body)
    public bool TryRead(Action<T> body)
    public bool TryWrite(Func<T, T> body)
}
```

| 成员 | 说明 |
|---|---|
| `Read` | 共享获取，`body` 拿到当前值的一份快照；`body` 没有返回值，**改不了存储的值** |
| `Write` | 独占获取（挡住所有读者和其他写者），`body` 的返回值成为新的存储值 |
| `TryRead` | 拿不到就返回 `false` 且**不调用 `body`**；成功时 `body` 已经跑完、读也已释放，返回 `true`。有别的读者在读不妨碍成功 |
| `TryWrite` | 有任何读者或写者占着就返回 `false` 且不调用 `body`；成功时返回 `true`，值已写回 |

要点：

- **`Write` 的 `body` 抛异常时值不变**，锁释放，异常传给调用方。
- **不可重入**：`Write` 里套 `Write`、`Write` 里套 `Read` → 抛 `Monitor is not reentrant…`
  （同 `Mutex`）；`Read` 里套 `Write`、以及夹在写者后面的 `Read` 里套 `Read` → **自死锁**，
  不抛异常也不返回。不要在同一个 `RwLock` 上嵌套获取。
- **`T` 是引用类型时 `Read` 的快照是引用**：在读体里改对象字段会真的改到共享值，而且此刻
  可能有别的读者在并发读——要么只存不可变值，要么只在 `Write` 里改。
- 读到的值同样只能靠静态字段 / 数组单元带出读体。

```z42
var cfg = new RwLock<long>(0);
cfg.Write((long v) => v + 1);          // 独占更新
long[] cell = new long[1];
cfg.Read((long v) => { cell[0] = v; }); // 并发读

if (!cfg.TryWrite((long v) => v + 1)) {
    // 锁被占着，这次跳过
}
```

## `Timer`

后台回调调度器，一次性或周期性，跑在自己的 OS 线程上。`sealed`，只能用两个静态工厂创建。

```z42
namespace Std.Threading;

public sealed class Timer {
    public static Timer StartPeriodic(long intervalMs, Action callback)
    public static Timer StartOnce(long delayMs, Action callback)
    public void Stop()
    public void StopAndJoin()
    public bool IsRunning()
}
```

| 成员 | 说明 |
|---|---|
| `StartPeriodic` | 每 `intervalMs` 毫秒触发一次，**首次触发在 +interval，不是立刻**。`intervalMs <= 0` 抛 `ArgumentException` |
| `StartOnce` | `delayMs` 之后触发一次然后自动停。`delayMs < 0` 抛 `ArgumentException`；`0` 合法（尽快触发） |
| `Stop` | 只发停止信号立刻返回，幂等；正在跑的回调会跑完 |
| `StopAndJoin` | 发信号并等后台线程退出。**不可重复调用**——第二次抛 `ThreadException("thread already joined")` |
| `IsRunning` | `Stop` / `StopAndJoin` 调过之后为 `false`；`StartOnce` 触发过之后也是 `false` |

要点：

- **回调不重叠**：一次回调跑完才开始下一轮等待，所以实际周期 ≥ `intervalMs` + 回调耗时。
- **回调抛的异常被吞掉**，只经 `Log.Error` 打一行 `Timer.StartPeriodic: callback threw: <消息>`；
  周期定时器继续按下一拍触发（实测 450 ms 内连抛 4 次，一次没漏）。
- **停止响应在 100 ms 粒度内**：即使是 5 秒周期的定时器，`StopAndJoin` 也立刻就返回。
- Timer 线程和普通线程一样，主线程返回时会被直接终止；长期存活的程序记得自己 `Stop`。

```z42
var t = Timer.StartPeriodic(1000, () => { Heartbeat(); });
// ...
t.StopAndJoin();          // 只能调一次

var once = Timer.StartOnce(500, () => { Fire(); });
once.Stop();              // 延迟内调用即取消，回调不会触发
```

## `ThreadException`

```z42
namespace Std;   // 注意：不在 Std.Threading

public class ThreadException : Exception {
    public ThreadException(string message)
    override string ToString()   // "ThreadException: <message>"
}
```

只由 `Thread.Join()`（以及 `Timer.StopAndJoin`，它内部就是 `Join`）抛出：

| 消息 | 触发 |
|---|---|
| worker 抛出的那个异常的 `Message` 原文 | worker 里有未捕获的 z42 异常 |
| `thread panicked` | worker 里发生 Rust 层 panic |
| `thread already joined` | 对同一个 `Thread` 第二次 `Join` |

**worker 的异常类型与栈不跨线程**：worker 里抛 `ArgumentException("boom")`，调用方只能
`catch (ThreadException)`，拿到的是 `Message == "boom"`，`catch (ArgumentException)` 接不住。

## `ChannelDisconnectedException`

```z42
namespace Std;   // 注意：不在 Std.Threading

public class ChannelDisconnectedException : Exception {
    public ChannelDisconnectedException(string message)
    override string ToString()   // "ChannelDisconnectedException: <message>"
}
```

| 消息 | 触发 |
|---|---|
| `channel closed` | 对已关闭的通道 `Send`，或 `Send` 等位置期间通道被关闭 |
| `channel disconnected` | 通道已关闭**且**队列已取空时 `Recv` |

`TrySend` / `TryRecv` 永远不抛这个异常，用返回值表达同样的状态。

## 用法

```z42
using Std;
using Std.IO;
using Std.Threading;

public static class Shared {
    public static Channel<long> Jobs;
    public static Mutex<long> Total;
}

void Main() {
    Shared.Jobs  = new Channel<long>(4);     // 有界：慢消费者会给生产者背压
    Shared.Total = new Mutex<long>(0);

    // 三个 worker 消费到通道关闭为止
    Thread[] workers = new Thread[3];
    int i = 0;
    while (i < 3) {
        workers[i] = Thread.Start(() => {
            bool go = true;
            while (go) {
                try {
                    long job = Shared.Jobs.Recv();
                    Shared.Total.Lock((long v) => v + job);
                } catch (ChannelDisconnectedException e) {
                    go = false;               // 关闭且取空 = 正常收工
                }
            }
        });
        i = i + 1;
    }

    long n = 1;
    while (n <= 100) { Shared.Jobs.Send(n); n = n + 1; }
    Shared.Jobs.Close();                     // 唯一的「没有更多值了」信号

    int j = 0;
    while (j < 3) { workers[j].Join(); j = j + 1; }

    long[] cell = new long[1];
    Shared.Total.Lock((long v) => { cell[0] = v; return v; });
    Console.WriteLine(cell[0]);              // 5050
}
```

不阻塞地轮询一个通道：

```z42
object[] r = Shared.Jobs.TryRecv();
long kind = (long)r[0];
if (kind == 0) {
    Handle((long)r[1]);
} else if (kind == 1) {
    Thread.Sleep(10);                        // 空转让一让；没有「带超时的 Recv」
}
```

## 不支持

线程：

- **没有线程池、没有 `async` / `await` / `Task`**：`Thread.Start` 就是一条真 OS 线程。
- **`Action` 之外的入口形态都没有**：worker 不能带参数、不能返回值。要回传结果得走静态字段、
  共享对象或 `Channel<T>`。
- **没有线程名、优先级、`IsAlive`、`Thread.CurrentThread`、线程 id 查询**（`SlotId()` 只是
  句柄自己的诊断编号，不是「当前线程是谁」）。
- **不能取消、不能中断、不能强杀**：`Thread` 上没有 `Cancel` / `Interrupt` / `Abort`。协作式
  取消要自己用共享的 `bool[1]` 单元或 `Channel` 实现。
- **`Join` 没有超时重载**，也没有 `TryJoin`；`Join` 一旦开始等就只能等到 worker 结束。
- **没有守护线程 / detach 语义**：进程随主线程返回而结束，未 `Join` 的线程被直接终止。
- **没人 `Join` 的 worker 异常被静默吞掉**，没有任何全局「未捕获异常」钩子。
- **异常不保真**：跨线程只带 `Message` 字符串，原类型与栈丢失。
- `Thread.Sleep` 是毫秒精度，没有纳秒 / `Yield` / 自旋提示。

同步：

- **语言没有 `lock` 语句**，也没有面向任意对象的裸锁用户 API——临界区一律经
  `Mutex<T>` / `RwLock<T>` 的回调体。（`using Std.Threading` 还会带进 `ThreadNative` /
  `MonitorNative` 两个名字，它们是 `z42.core` 的 extern 声明层，不是本包的用户 API。）
- **没有 `Interlocked` / 原子类型 / `volatile`**：一个 `long` 的原子自增也得用 `Mutex<long>`。
- **没有信号量、条件变量、屏障、倒计时事件、`ManualResetEvent`、读写升级锁**。
- **`Mutex<T>` 没有 `TryLock`**（`RwLock<T>` 有 `TryRead` / `TryWrite`，`Mutex` 没有对应物）。
- **没有带超时的获取**：`Try*` 只有「立刻成功或立刻失败」两态。
- **不可重入**，且重入抛的是基类 `Exception`，没有可精确 catch 的专用异常类型。
- **`Lock` / `Read` / `Write` 都没有返回值**：读出来的值必须靠静态字段或数组单元带出回调体。
- **`RwLock<T>.Read` 的「只读」不由类型系统保证**：`T` 是引用类型时，读体里改字段会真的改到
  共享值。

通道：

- **没有带超时的 `Recv`**，也没有 `Recv` 的取消。要么阻塞到底，要么用 `TryRecv` + `Sleep` 轮询。
- **没有 `select` / 多路复用**：无法同时等多个通道。
- **没有 `Count` / `IsClosed` / `Capacity` 查询**，也不能 `foreach` 遍历通道。
- **没有 sender / receiver 分离句柄**：谁拿到通道谁都能收发，也就没有「最后一个 sender 析构
  即自动断开」——只有显式 `Close()` 能让接收方收工。
- **无界通道没有内存上限**：生产快于消费就一直扩容。要背压就用有界构造器。
- **会合通道（`capacity == 0`）上已经阻塞的 `Send` 不会被 `Close()` 唤醒**：那个值必须被某个
  接收者取走，`Send` 才会返回，否则该线程一直挂着。关闭会合通道前要先确保在途的值有人接。

定时器：

- **没有 `dueTime` + `period` 的组合**：`StartPeriodic` 的首次触发固定在 +interval；想要「立刻
  跑一次再周期跑」得自己先调一次回调。
- **没有 `Change` / 重启 / 改周期**：停了就得重新 `Start`。
- **没有 state 参数**，回调是无参 `Action`，上下文靠闭包捕获的对象带。
- **`StopAndJoin` 不幂等**：第二次调用抛 `ThreadException("thread already joined")`。
- **回调异常不会上报给调用方**，只有一行 `Log.Error`。
