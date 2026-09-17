# z42 并发与异步设计

> **状态**：L3 前瞻性设计草案（2026-04-30），语言层尚未实现；**runtime
> foundation 已落地**（2026-05-20，详下方）
> **定位**：与 `generics.md` / `static-abstract-interface.md` 同级 — 长期规范，等
> 到 L3 阶段才进入 `docs/spec/changes/` 实施流程
> **参考**：C# / .NET TPL（主蓝本）+ Rust（Send/Sync）+ Kotlin / Swift（结构化并发）
> **核心选型**：染色（async/await 显式）+ 全 async-only 标准库 + 结构化并发强制 +
> `Send`/`Sync` 类型层安全 + 单一 runtime
>
> 阅读对象：使用者视角（语法 / 心智模型 / 减痛体验）。
> 实现原理（VM 调度器、状态机生成、I/O reactor）见 `vm-architecture.md` 与
> `compiler-architecture.md` 的对应章节（L3 阶段补齐）。

---

## Runtime foundation 现状（2026-05-20 落地）

[`add-multithreading-foundation`](../../../spec/archive/2026-05-20-add-multithreading-foundation) spec 已完成 Phase 1+2+3：

- **VmCore / VmContext 类型层划分**：共享 8 字段 (`Arc<VmCore>`) + per-thread 4 字段（VmContext 持 `Arc<VmCore>` 加自己的 Arc<Mutex<>> 字段）
- **GcRef 切到 Arc backing**：`Rc<GcAllocation>` → `Arc<GcAllocation>`，内部 `RefCell<T>` → `parking_lot::Mutex<T>`
- **MagrGC trait 加 Send + Sync 边界**：实施 backend `ArcMagrGC`（前 `RcMagrGC`）满足
- **6 个编译期 Send+Sync assertion**（[src/runtime/src/gc/arc_heap_tests/send_sync.rs](../../../../src/runtime/src/gc/arc_heap_tests/send_sync.rs)）+ 3 个 cross-thread 集成测试（[src/runtime/tests/cross_thread_smoke.rs](../../../../src/runtime/tests/cross_thread_smoke.rs)）钉死不可回归

**Phase 1+2+3 完成后能做什么 / 还不能做什么**：

| 现状 | 描述 |
|------|------|
| ✅ Rust 类型层 Send+Sync | `VmContext` / `VmCore` / `GcRef` / `Value` / `Box<dyn MagrGC>` 全 Send+Sync |
| ✅ 跨线程 read 共享 GC | `Arc<VmCore>` 可跨线程；GcRef 可跨线程 move / clone / borrow |
| ✅ 多 VmContext 共享 GC heap | VmCore `vm_contexts` 注册表 + `Pin<Box<VmContext>>` 地址稳定 + scanner walk registry（add-vmcontext-registry 2026-05-20） |
| ✅ 用户层线程 API | `Std.Threading.Thread.Start(Action)` / `.Join()` + `Std.ThreadException`（add-threading-stdlib 2026-05-20）。底层 `__thread_spawn` / `__thread_join` builtin 接 `VmCore.threads: Mutex<HashMap<u64, JoinHandle>>` slot table。worker 走 `VmContext::new_with_core(Arc::clone(core))` 共享父 VmCore |
| ✅ 同步原语 | `Std.Threading.Mutex<T>` / `Channel<T>` + `Std.ChannelDisconnectedException`（add-sync-primitives 2026-05-20）。Mutex 走 `parking_lot::Mutex<Value>` + thread-local guard parking（`__mutex_lock_acquire` / `__mutex_store` / `__mutex_unlock`，z42 facade 暴露 RAII `Lock(Func<T,T>)`）；Channel 走 `std::sync::mpsc` unbounded MPSC（`__channel_send` / `__channel_recv` / `__channel_try_recv` / `__channel_close`）。GcRef::borrow 同时从 `try_lock+panic` 改为 blocking `lock`（多线程下两 worker 并发 field_get 同一 object 的根因修复）|
| ✅ 并发 GC (safepoint complete) | **Safepoint 协议落地**：interp dispatch loop 在 function entry / backward branch / Call return 三处 `check_safepoint(ctx)`；script-driven `__gc_collect` / `__gc_force_collect` 由 `request_gc_pause` RAII guard 包成 stop-the-world（add-gc-safepoint 2026-05-20）。auto-threshold 路径 `maybe_auto_collect` 现也走 safepoint：trip 时 set `VmCore.needs_auto_collect` AtomicBool flag → 下个 `check_safepoint(ctx)` swap claim → safepoint-wrapped collect（add-gc-safepoint-auto-threshold 2026-05-20）。所有 mutator VmContext park 在 `gc_phase_cv` Condvar 上等 phase 回 Idle。**仍单线程 mark+sweep**（多线程化待 `add-concurrent-gc`）；JIT-mode safepoint 待 `add-gc-safepoint-jit`|
| ❌ `spawn` / `task scope` 语法 | L3 阶段引入（本文档 §3.5） |

**下一步实施 spec**（已在 add-multithreading-foundation tasks.md 列）：
1. ~~`add-vmcontext-registry`~~ ✅ 已落地（2026-05-20）—— VmCore registry + Pin<Box<VmContext>> + scanner walk
2. ~~`add-threading-stdlib`~~ ✅ 已落地（2026-05-20）—— `Std.Threading.Thread.Start` / `.Join` + ThreadException
3. ~~`add-sync-primitives`~~ ✅ 已落地（2026-05-20）—— `Std.Threading.Mutex<T>` / `Channel<T>` + ChannelDisconnectedException + GcRef::borrow blocking 修复
4. ~~`add-gc-safepoint`~~ ✅ 已落地（2026-05-20）—— interp safepoint 协议 + stop-the-world wrapper
5. ~~`add-gc-safepoint-auto-threshold`~~ ✅ 已落地（2026-05-20）—— auto-threshold 路径也走 safepoint，关闭 v0 race window
6. ~~`add-gc-safepoint-jit`~~ ✅ 已落地（2026-05-21）—— JIT translate 在 function entry / backward branch / Call return 等 4 个 site emit `jit_check_safepoint` helper call，与 interp 完全对齐
7. `add-concurrent-gc` — Phase A 性能轨道
8. `add-spawn-syntax` — L3，本文档 §3.5

---

## 1. 设计目标

| # | 目标 | 衡量标准 |
|---|------|---------|
| 1 | **染色但不痛** | 染色带来的"两套 API / 死锁 / 仪式代码"在 z42 上不出现 |
| 2 | **AOT 与 VM 友好** | async 方法编译为状态机，不依赖 fiber 栈分配，AOT 体积可控 |
| 3 | **类型层并发安全** | 跨 task 共享 / 移动的类型必须满足 `Send` / `Sync`，编译期拒绝数据竞争 |
| 4 | **结构化为默认** | 顶层 `spawn` 必须挂在 `task scope { ... }` 下，孤儿任务编译期不可达 |
| 5 | **零分配快路径** | `async 方法` 同步完成时不分配；`Future<T>` 优先栈分配，逃逸再装箱 |
| 6 | **生态不分裂** | 标准库唯一 runtime；不存在 sync/async 双套 API；不存在第三方 runtime 互不兼容 |
| 7 | **新手第一行代码接近同步** | `var s = await? File.Read("a.txt");` —— 三个关键字内即可表达 I/O |

---

## 2. 方案对比与选型

### 2.1 主流并发模型

| 模型 | 代表 | 优 | 劣 |
|------|------|-----|-----|
| **1:1 OS 线程 + 锁** | C/C++ pthread | 控制力最强、零运行时 | 数据竞争靠人；万级并发即崩 |
| **OS 线程 + Send/Sync** | Rust（不含 async） | 编译期消除数据竞争 | 调度仍重，不解决 I/O 并发 |
| **M:N 协程 + channel** | Go | 极轻量、CSP 直观 | 数据竞争编译器不查、抢占点尴尬 |
| **stackless async/await** | C# / Rust / TS | 内存最省、AOT 友好 | 函数染色、生态分裂 |
| **virtual threads** | Java Loom | 同步代码异步执行 | 依赖 JVM、栈快照成本 |
| **结构化 + actor** | Swift / Kotlin / Erlang | 取消传播、生命周期清晰 | 共享状态算法不便 |

### 2.2 z42 选型：染色（stackless async/await）+ 减痛栈

| 决策 | 选择 | 理由 |
|------|------|------|
| **是否染色** | 染色（async/await 显式） | AOT 友好、VM 简单、性能可预测；用减痛设计抵消生态痛点 |
| **runtime 模型** | 全局 work-stealing scheduler + IOCP/io_uring/kqueue | 与 .NET ThreadPool / Tokio 同思路 |
| **调度粒度** | task = 状态机 + 调度槽（非 fiber 栈） | 状态机是普通对象，AOT 友好 |
| **API 颜色** | 标准库 **async-only** | 不再写两套；`File.read` 唯一存在 |
| **结构化并发** | 强制 — `spawn` 必须在 `task scope` 内 | 编译期词法检查根除孤儿 |
| **数据竞争防护** | `Send` / `Sync` marker trait | 跨 task 共享的类型必须满足约束 |
| **取消模型** | 协作式 `CancellationToken` | 与 .NET 一致；无强杀 |
| **Sync 上下文** | **不引入** `SynchronizationContext` | 默认即 `ConfigureAwait(false)`；避免死锁陷阱 |
| **顶层入口** | `async void Main` + 顶层 `await` | 脚本即时可写；无需 `BlockOn` |

### 2.3 为什么不选 fiber / 无染色

| 反对理由 | 说明 |
|----------|------|
| AOT 路径复杂 | fiber 栈管理需 VM 介入，AOT 镜像难以静态化 |
| FFI 边界更难 | C ABI 不识别 fiber 栈，需 stack switch trampoline |
| 抢占点不显式 | 任意函数都可能让步，调试与性能分析困难 |
| 生态价值低 | 与 .NET / Rust / TS 生态符号 / 工具链不一致，用户迁移成本高 |

> 染色的代价是"语法多一个 `async`/`await`"，z42 用第 5 节列出的 16 条减痛把这个
> 代价压到 .NET 现状的约 1/3。

---

## 3. 核心模型与语法

### 3.1 `async` 修饰符与 `await`

```z42
async string Fetch(string url) {
    var resp = await Http.Get(url);
    return resp.Body;
}
```

规则：
- `async` 是函数 / 方法的修饰符，紧贴在返回类型前（与 C# 一致）
- 返回类型 `T` 在调用方观察上等价于 `Future<T>`，编译器自动包装；用户**不写**
  `Future<T>` 作为返回类型（与 C# `async Task<T>` 的强制不同 —— z42 让 `Future`
  完全隐式）
- `await` 只能出现在带 `async` 修饰的函数体内或顶层 `Main`（顶层 `Main` 隐式 `async`）
- 在非 `async` 函数内调用 `async` 方法会得到 `Future<T>`，不会自动 await
- sync 上下文里只能通过 `BlockOn(future)` 阻塞等待，**仅推荐边界使用**（FFI / `Main`
  之外的 sync entry），其他位置触发 lint 警告

### 3.2 `Future<T>` 类型

`Future<T>` 是状态机的统一抽象，**无 `Task<T>` 与 `ValueTask<T>` 的二分**：

- 状态机优先**栈分配**，仅在跨 await / spawn 边界逃逸时升堆
- 同步完成路径零分配（与 .NET `ValueTask` 等价）
- `async T F()` 与 `Future<T> G()` 在类型层等价，可互相替代（前者是后者的语法糖）

### 3.3 错误传播 `await?`

`await` 与 `?`（错误传播）合体为单关键字：

```z42
var user = await? Db.Find(id);
var resp = await? Http.Get(url);
```

语义：先 await，再对 `Result<T>` 做 `?` 提前返回。比 Rust `.await?` 视觉更紧凑。

### 3.4 链式 await 自动串接

中间环节是 async 时，单个 `await` 覆盖整链：

```z42
var body = await Client.Get(url).Header("X-Token", t).Send().Body();
//         ↑ 编译器自动串接所有 async 节点
```

实现：类型推断驱动；链式调用中只要有任一节点返回 `Future<T>`，整链视为
异步链，单个 `await` 在最外层提取最终值。

### 3.5 `task scope { ... }` 与 `spawn`

```z42
async (string, string) ParallelFetch() {
    task scope {
        var a = spawn { Fetch("x"); };
        var b = spawn { Fetch("y"); };
        return (await a, await b);
    }   // 离开 scope 前，所有未完成 task 自动被 cancel + join
}
```

**硬规则**（编译期词法检查）：
- `spawn` 表达式只能出现在 `task scope { ... }` 块的词法作用域内
- 否则报错 **Z0801**（详见第 9 节）
- 检查方式：parser/sema 维护 `scope_stack`，遇到 `spawn` 时栈空 → 错误。
  与 `break` 必须在循环内同一类静态检查，无需类型系统魔法

**scope 三条硬语义**：
1. **join 保证** — scope 块退出前，所有 spawn 子任务必须终止（完成或取消）
2. **异常传播** — 任一子任务抛异常 → 触发兄弟 cancel → 异常在 scope 出口处重抛
3. **取消传播** — scope 被取消 / 提前 return → 所有子任务收到 cancel 信号

**根 scope（`task scope root { ... }`）**：
- 仅在 `Main` 体内合法
- 用于声明"生命周期与进程绑定"的长跑任务（如 metrics / heartbeat）
- 进程退出时所有 root scope 子任务收到 cancel 信号

```z42
async void Main() {
    task scope root {
        spawn { MetricsLoop(); };
        spawn { HttpServer(); };
    }
}
```

**显式 detach（fire-and-forget）**：

```z42
task scope detached {
    spawn { AuditLog.Write(data); };
}
// detached 仍在 scope 退出前 join；要真正后台需挂到 root scope
```

`detached` 关键字让"我故意脱离当前流程"在 review 时一目了然，不再隐藏在
`async void` 之类的裸语法里。

### 3.6 取消令牌 `CancellationToken`

```z42
async void Op(CancellationToken ct) {
    while (!ct.IsCancelled) {
        await? Step(ct);
    }
}
```

- 协作式 — 函数体内显式检查或将 `ct` 传给下游
- `task scope` 自动管理隐式 token；显式参数与隐式 scope token 通过 `ct.Combine(...)`
  组合
- 取消触发 `OperationCancelledException`（不是返回特殊值）

### 3.7 `await foreach` 异步流

```z42
await foreach (var row in conn.Query(sql)) {
    Process(row);
}
```

- 等价于 .NET `IAsyncEnumerable<T>` / `await foreach` —— 语法与 C# 一致
- `Stream<T>` 是一等类型，与 `Iterator<T>` 平行（区别仅在 `Next()` 是否 async）

### 3.8 `await using` 与 `async drop`

```z42
await using (var conn = Db.Open()) {
    // ... 使用 conn ...
}   // 离开块时 await conn.CloseAsync()
```

语法与 C# `await using` 一致；唯一区别是 z42 不要求显式实现 `IAsyncDisposable`，
而是通过 `AsyncDrop` 接口自动识别。

`async drop` 协议（隐式由编译器调用）：

```z42
public class Connection : AsyncDrop {
    public async void Drop() { await this.CloseSocket(); }
}
```

任何实现 `AsyncDrop` 的类型在 `async` 上下文中离开作用域时，编译器自动插入
`await this.Drop()`。在 sync 上下文持有此类对象 → 编译错误（Z0802）。

### 3.9 通信原语

#### Channel\<T\>

```z42
var ch = Channel<int>.Bounded(16);
spawn { await ch.Write(42); };
var v = await ch.Read();
```

- `Bounded(cap)` / `Unbounded()` / `Mpsc()` / `Mpmc()` 通过工厂方法选择
- 关闭：`ch.Close()`；读侧 `await ch.Read()` 返回 `Option<T>`，关闭后为 `None`

#### `select` 多路复用

```z42
select {
    v <- ch1            => Use(v),
    ch2 <- 7            => Log("sent"),
    Timeout(100.Ms)     => break,
    Cancelled(ct)       => return,
}
```

> `select` 是 z42 新增关键字；分支用逗号分隔（match 风格），`<-` 表示 channel
> 操作。对照 Go `select`，z42 的关键差异是分支体走表达式而非语句块，且强制
> 包含通道关闭 / 取消分支可由编译器静态检查（lint）。

#### Lock / Atomic / RwLock

```z42
lock (this.state) {        // C# 同名关键字，Monitor 风格，自动 enter/exit
    this.count += 1;
}

var n = new Atomic<int>(0);
n.FetchAdd(1, Ordering.AcqRel);

var rw = new RwLock<Dictionary<string, int>>(new Dictionary<string, int>());
var r = await rw.Read();
var w = await rw.Write();
```

- `lock` 复用 C# 同名关键字，不暴露裸 `Monitor.Enter` / `Exit`
- `Atomic<T>` 限于内置数值与指针类型；用户类型若需共享，走 `Mutex<T>` 或 `Arc<T>` + `Sync`
- `RwLock` 的 `Read()` / `Write()` 返回 RAII 守卫（实现 `AsyncDrop`），离开作用域自动释放

### 3.10 数据并行

```z42
Parallel.For(0, 1000, i => Process(i));
Parallel.Map(items, x => Compute(x));
```

- 内部走全局 ThreadPool 而非 task scheduler，避开 async runtime 的让步语义
- 适合纯 CPU 工作负载；I/O 密集仍走 task scope + spawn

---

## 4. 类型层并发安全：`Send` / `Sync`

### 4.1 marker trait 定义

```z42
// 内置 marker interface，编译器自动为符合条件的类型实现
public interface Send { }      // 该类型的值可以跨 task 移动
public interface Sync { }      // 该类型的引用可以跨 task 共享（跨线程访问安全）
```

> z42 的 marker trait 通过 `interface` 表达（与 `static-abstract-interface.md`
> 体系一致），不引入独立的 `trait` 关键字。`Send` / `Sync` 是 stdlib 内置接口，
> 编译器对常见结构自动生成 `impl`，用户也可显式声明。

- 大多数值类型自动 `Send + Sync`
- 含内部可变状态（裸指针、`Cell<T>`、未同步的可变字段）的类型默认 `!Send + !Sync`
- 跨 task 边界（`spawn`、`Channel<T>.Write`、`Arc<T>` 共享）要求实参满足约束
- 违反 → 编译错误 Z0803

### 4.2 与现有泛型 / Trait 体系协作

```z42
public Arc<T> Share<T>(Arc<T> x) where T : Sync => x;

public async void Worker<T>(Channel<T> ch) where T : Send {
    while (true) {
        var msg = await ch.Read();
        if (msg is None) break;
        Process(msg.Value);
    }
}
```

`Send` / `Sync` 是 z42 interface 体系（见 `static-abstract-interface.md`）的特殊形式 —
**编译器隐式推导 + 用户可显式声明 `unsafe impl`**（FFI 边界使用）。

### 4.3 与 GC 的关系

z42 始终带 GC（见 roadmap"固定决策"），`Send`/`Sync` 不引入所有权 / 借用模型。
其作用纯粹是**类型层数据竞争检查**，不影响堆管理。GC 处理的是内存生命周期；
`Send`/`Sync` 处理的是访问可见性。

---

## 5. 染色减痛清单（核心设计意图）

下面 16 条是 z42 染色路线区别于 .NET 现状的关键。每条注明缓解的具体痛点。

| # | 减痛措施 | 缓解的痛点 |
|---|----------|-----------|
| 1 | 标准库 async-only，不补 sync 版 | API 双套并存（`Read`/`ReadAsync`） |
| 2 | 命名不带 `Async` 后缀 | 命名噪声 |
| 3 | 高阶函数泛型 over `Future` / `IntoFuture` | LINQ 翻倍（`Where` / `WhereAsync` / `WhereAwait`）|
| 4 | `async trait` 编译器原生支持 | Rust `#[async_trait]` 宏地狱 |
| 5 | `async main` + 顶层 `await` | 脚本入口仪式代码 |
| 6 | `await?` 错误传播合体 | 必须 try/catch 包 await |
| 7 | 链式 `await` 自动串接 | `.await.x().await.y().await` 视觉噪声 |
| 8 | `await foreach` / `await using` / `async drop` 原生 | `IAsyncEnumerable` / `IAsyncDisposable` 后补 |
| 9 | 编译器智能修复建议 | "Cannot await sync function" 空话错误信息 |
| 10 | 单一 `Future<T>`（栈分配优先） | `Task<T>` vs `ValueTask<T>` 二分 |
| 11 | 不引入 `SynchronizationContext` | `.Result` 死锁 + `ConfigureAwait(false)` 噪声 |
| 12 | 异步栈追踪默认拼接 | 跨 await 栈断裂 |
| 13 | `block_on` 仅边界使用，他处 lint | sync 误调 async 死锁 |
| 14 | 单一官方 runtime | tokio / async-std / smol 分裂 |
| 15 | `task scope` 强制，`spawn` 词法约束 | 孤儿 task / `async void` 异常静默吞掉 |
| 16 | `Send`/`Sync` 类型层检查 | 数据竞争靠 `lock` 文化约定 |

新手第一行代码的最终样子：

```z42
// hello.z42 —— 顶层 Main 隐式 async
async void Main() {
    var s = await? File.Read("conf.toml");
    var cfg = Config.Parse(s);
    Console.WriteLine(cfg);
}
```

对比 .NET 同语义代码（≈ 9 行 + 仪式 + Async 后缀），z42 视觉噪声压到约 1/3。

---

## 6. 执行模型

### 6.1 Pipeline

```
async T M(...)  →  AST.AsyncMethod
    ↓ (Lowering pass)
普通 M(...) 返回 Future<T>，body 转为状态机 record + transition table
    ↓ (IR Codegen)
zbc：状态机字段 / case 跳转 / await yield 点 标记
    ↓
Runtime：
  - Interpreter / JIT / AOT 共享同一调度器
  - I/O 操作走 OS 异步原语（IOCP / io_uring / kqueue）
  - work-stealing scheduler，per-thread local queue
```

### 6.2 调度器模型

- **全局唯一**，VM 启动时初始化
- 工作线程数默认 = CPU 核心数；可通过 `RUNTIME_WORKER_THREADS` 环境变量调整
- Task = 状态机 + 调度槽 + 上下文（cancel token、scope handle）
- I/O 完成由 reactor 线程通知调度器恢复对应 task

### 6.3 跨 spawn 边界的捕获规则

```z42
task scope {
    var x = Compute();                 // x: List<int>
    spawn { Use(x); };                 // 编译期检查：x 必须 Send
}
```

跨 `spawn` / async task 边界的闭包捕获遵循通用闭包规范——用户面的捕获语义见
[闭包与捕获语义](../../../reference/src/language/closures.md)。本节只列并发特有的约束（⚠️ `spawn` /
`task` 语法与 `Send` 派生**均未实现**，以下是设计意图）：

- **强制 move 捕获**（与 Rust async 一致）：`spawn` 之后外部不得再用被捕获变量
- **捕获项必须 `Send`**：编译器自动派生闭包 Send 性，违反报 `Z0809`
- **scope 内对 scope 外变量的引用** → 必须 `Sync` 且生命周期覆盖 scope

### 6.4 阻塞操作的处理

CPU 密集 / 阻塞 syscall 不应在 task 上跑（会卡 worker）：

```z42
var result = await SpawnBlocking(() => HeavyCpuWork());
```

`SpawnBlocking` 把闭包扔到独立线程池，task 让出 worker。与 Tokio
`spawn_blocking` 同思路。

---

## 7. 不在本设计内的内容（Out of Scope）

- **Actor 模型**：不作为语言原语，可由库基于 `Channel` + `task scope` 实现
- **绿色线程 / fiber**：与染色路线冲突，不引入
- **`SynchronizationContext`**：明确不引入，UI 线程回流由库显式 dispatcher 处理
- **多 runtime 抽象层**：不允许第三方替换 runtime；标准库 runtime 是唯一实现
- **手动栈管理 / coroutine.yield**：不暴露给用户，状态机由编译器封闭生成
- **类型层取消（cancel-by-type）**：取消仍是协作式 token 传递，不引入"取消即类型错误"

---

## 8. 与其他语言对比

### 8.1 染色相关特性

| 特性 | .NET | Rust | Kotlin | Swift | **z42** |
|------|------|------|--------|-------|---------|
| 染色（async 关键字） | ✅ | ✅ | ✅(suspend) | ✅ | ✅ |
| 标准库 async-only | ❌（双套） | ⚠️（部分） | ❌ | ✅ | ✅ |
| 结构化并发强制 | ❌ | ❌ | ⚠️（库层） | ✅ | ✅（编译期） |
| `Send`/`Sync` 检查 | ❌ | ✅ | ❌ | ✅(部分) | ✅ |
| 单一 runtime | ✅ | ❌ | ✅ | ✅ | ✅ |
| `async trait` 原生 | ✅ | ⚠️（最近稳定） | ✅ | ✅ | ✅ |
| `await foreach` / `await using` | ✅ | ❌ | ✅ | ✅ | ✅ |
| 顶层 await | ❌ | ❌ | ✅ | ✅ | ✅ |
| 错误传播合体（`await?`） | ❌ | ✅ | ❌ | ✅(`try await`) | ✅ |
| 链式自动串接 | ❌ | ❌ | ❌ | ❌ | ✅（z42 独有） |
| 异步栈追踪 | ✅(Core+) | ⚠️ | ✅ | ✅ | ✅ |

### 8.2 z42 独有点

1. **链式 `await` 自动串接** — 单 `await` 覆盖整个链式调用，类型推断驱动
2. **`task scope` 编译期硬强制** — `spawn` 词法作用域检查，比 Swift TaskGroup 更严
3. **统一 `Future<T>`** — 不引入 `Task<T>`/`ValueTask<T>` 二分，栈分配优先
4. **`task scope detached` 关键字** — 显式 fire-and-forget，比 .NET `async void`
   或 Rust `tokio::spawn` 顶层裸调更可见

---

## 9. 错误码段 Z08xx — 并发与异步

> 写入 `error-codes.md` Z08xx 段；本节列出预留诊断码与触发条件。

| Code | Title | 触发场景 |
|------|-------|---------|
| Z0801 | `spawn` outside `task scope` | `spawn` 词法作用域内无 `task scope` |
| Z0802 | `AsyncDrop` value held in sync context | sync 函数持有需要 async drop 的对象 |
| Z0803 | Type does not satisfy `Send` / `Sync` | 跨 task 边界传递不满足约束的类型 |
| Z0804 | `await` outside async context | 非 async 函数体内出现 `await` |
| Z0805 | Cannot call async method from sync context | sync 函数直接调用 async 方法（含修复建议）|
| Z0806 | `block_on` used outside boundary | `block_on` 在非 main / 非 FFI 桥接位置使用（lint，可降级为警告）|
| Z0807 | Mismatched `Future<T>` type | 类型推断中 sync / async 路径不一致 |
| Z0808 | `task scope detached` outside async | `detached` 仅在 async 上下文合法 |
| Z0809 | Capture of non-`Send` value into spawn | spawn 闭包捕获了非 `Send` 变量 |
| Z0810 | Reference to non-`Sync` value across task | scope 内 task 引用了非 `Sync` 的外部变量 |

> 完整诊断信息（措辞 / fix-it 建议）在 L3 实施时与 `DiagnosticCatalog.cs` 同步定稿。

---

## 10. 实施路线图

### 10.1 阶段划分（L3 内的子阶段）

| 阶段 | 范围 | 依赖 |
|------|------|------|
| **L3-A1** | `Future<T>` 类型 + 状态机 lowering + 单线程 interpreter scheduler | L2 完成（Trait / 泛型）|
| **L3-A2** | `await` / `await?` / 顶层 `await` 语法与类型检查 | L3-A1 |
| **L3-A3** | `task scope` / `spawn` / 词法检查 / Z0801 | L3-A2 |
| **L3-A4** | `CancellationToken` + scope 取消传播 | L3-A3 |
| **L3-A5** | `Channel<T>` / `select` / `lock` / `Atomic<T>` 标准库 | L3-A4 |
| **L3-A6** | `Send`/`Sync` marker trait + 推导 + 跨 task 检查 | L3-A4 + Trait 完整 |
| **L3-A7** | 多线程 work-stealing scheduler + I/O reactor | L3-A1 |
| **L3-A8** | `await foreach` / `await using` / `async drop` | L3-A2 + L3-A7 |
| **L3-A9** | 链式 await 自动串接、智能修复建议、异步栈追踪 | L3-A2 + 编译器 lowering 成熟 |
| **L3-A10** | `parallel.for` / `spawn_blocking` / `block_on` 边界 API | L3-A7 |

### 10.2 单元测试要求（每阶段）

每子阶段必须含：
- 至少 1 个端到端 golden test（`source.z42` + `expected_output.txt`）
- 至少 1 个错误诊断 case（覆盖对应的 Z08xx）
- 至少 1 个 `Send`/`Sync` 编译失败 case（A6 起）

### 10.3 与 VM 性能基准的关系

L3-A7 完成后，将引入并发基准：
- 协程上下文切换延迟 < 1μs（对照 Tokio / .NET）
- 1M 并发 task 创建 + join < 1s（Channel 的吞吐量基准）
- I/O echo server p99 latency 比 .NET ASP.NET Core 差 < 30%

未达标 → 暂不进入 L3-A8+ 用户面特性，先优化 runtime。

---

## 11. Open Questions（待 L3 实施时定稿）

- [ ] **链式 `await` 自动串接**的精确推断规则：当链上既有 sync 又有 async 节点时
      如何判定 await 注入点？（候选：从最内层 async 节点起所有外层 await）
- [ ] **`async drop` 的失败语义**：drop 内 await 抛异常时，是否吞掉、记录、还是
      panic？倾向"记录 + 继续 unwind"，但需对照 Swift / Rust async drop RFC
- [ ] **`task scope` 是否允许返回 task handle**：当前设计要求 handle 不得逃逸
      scope；是否需要"detached handle"逃逸到外层 root scope 的能力？
- [ ] **`Channel<T>` 的关闭语义**：`close()` 后未读消息保留还是丢弃？倾向保留直到读完
- [ ] **`select` 是否支持优先级 / 公平性 hint**：当前设计随机选；某些场景需偏置
- [ ] **AOT 模式下状态机大小**：是否对超大 async 方法做切片 / outlining？
- [ ] **与现有 Exception 体系的整合**：取消产生 `OperationCancelledException`，
      但如何与 `task scope` 的"异常聚合"（多个子任务同时抛）协调？候选：聚合为
      `AggregateException`（C# 同名）

---

## 12. 与现有 z42 设计文档的依赖

| 依赖文档 | 关系 |
|---------|------|
| `language-overview.md` | 并发语法将作为 L3 章节追加；`async` / `await` / `task scope` / `spawn` / `select` / `lock` 等关键字加入关键字表 |
| `generics.md` | `Future<T>` / `Channel<T>` / `Arc<T>` / `Mutex<T>` 是泛型类型；`Send`/`Sync` 是 marker trait |
| `static-abstract-interface.md` | `Send`/`Sync` 用 trait 体系表达；`async trait` 与 trait dispatch 协作 |
| `exceptions.md` | `OperationCancelledException` / `AggregateException` 加入标准异常层次 |
| `vm-architecture.md` | scheduler / reactor / 状态机执行机制（L3 实施时新增章节） |
| `compiler-architecture.md` | async 方法 lowering pass / Send-Sync 推导 / scope 词法检查（L3 实施时新增章节）|
| `error-codes.md` | Z08xx 段加入 |
| `ir.md` | 状态机字段编码、await yield 标记、task spawn 指令（L3 实施时定义）|

---

## 13. 修订历史

| 日期 | 版本 | 变更 |
|------|------|------|
| 2026-04-30 | DRAFT v1 | 初稿；L3 前瞻性设计；选型染色 + 减痛栈；待 User 审阅 |
| 2026-04-30 | DRAFT v2 | 全文代码示例由 Rust 风格统一改为 z42 现行 C# 语法（与 `language-overview.md` / `examples/` 对齐）：`async fn name(p: T) -> R` → `async R Name(T p)`；`let` → `var`；`Future<T>` 完全隐式（用户写 `async R F()` 而非 `async Future<R> F()`）；`fn main` → `async void Main`；`for await` → `await foreach`；`async using` → `await using`；`Channel<T>::bounded` → `Channel<T>.Bounded`；闭包 `\|x\| ...` → `x => ...`；`trait Send`/`Sync` → `interface Send`/`Sync`；`Z0805` 标题保留英文 "Cannot call async method from sync context"。设计内容与减痛清单 16 条不变。 |
