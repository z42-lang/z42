# 委托与事件（delegate / event）

> 对齐：2026-09-17

z42 把"可调用值"拆成**两类互不混淆的类型**：

- **单播** —— `delegate` 关键字声明的类型（`Action` / `Func` / `Predicate` 及用户自定义），
  一个实例最多持 **1 个** handler；
- **多播** —— stdlib 的 `MulticastAction<T>` / `MulticastFunc<TArg, TResult>` /
  `MulticastPredicate<T>` 三个 sealed class，一次注册 **N 个** handler。

`event` 关键字**不决定**单播还是多播——**字段类型决定**。它只做访问控制糖：把外部的
`+=` / `-=` 改写成对合成访问器 `add_X` / `remove_X` 的调用。

与 C# 的最大差别：C# 的 `Action<T>` 本身就是多播的（`MulticastDelegate`），z42 的
`Action<T>` 永远只有一个 handler；要多播就写出 `MulticastAction<T>`。类型 100% 诚实。

## 1. 设计目标

| # | 目标 | 衡量标准 |
|---|------|---------|
| 1 | C# 用户零迁移成本 | 表层语法（`delegate` / `event` / `+=` / `-=`）与 C# 视觉一致 |
| 2 | 消除 C# delegate/event 的长期痛点 | 见 [§9 改进项映射](#9-改进项映射c--z42) |
| 3 | API 表面极简 | 1 个 `event` 关键字；0 个订阅策略 attribute |
| 4 | 关键字 / 修饰符不堆叠 | 不会出现 `[Weak] [Once] event ...` 这种链式修饰符 |
| 5 | 默认路径零开销 | 裸 `+= h` 订阅不分配包装对象 |
| 6 | 未来扩展不动语法 | 新订阅策略 = 加一个 `ISubscription` 实现类 |
| 7 | 单播 / 多播视觉与心智统一 | 都用 `event TYPE NAME` + `+=` / `-=`；类型决定语义 |

## 2. 单播：`delegate` 类型

### 2.1 stdlib 类型

```z42
namespace Std;

public delegate void Action();
public delegate void Action<T>(T arg);
public delegate void Action<T1, T2>(T1 a, T2 b);
public delegate void Action<T1, T2, T3>(T1 a, T2 b, T3 c);
public delegate void Action<T1, T2, T3, T4>(T1 a, T2 b, T3 c, T4 d);

public delegate R Func<R>();
public delegate R Func<T, R>(T arg);
public delegate R Func<T1, T2, R>(T1 a, T2 b);
public delegate R Func<T1, T2, T3, R>(T1 a, T2 b, T3 c);
public delegate R Func<T1, T2, T3, T4, R>(T1 a, T2 b, T3 c, T4 d);

public delegate bool Predicate<T>(T arg);
```

**arity 覆盖 0–4**（`Predicate` 仅 1 元）。C# 的 `Action<T1..T16>` / `Func<T1..T16, R>`
在 z42 **不存在**——实测 examples / tests 里 5+ 元 callable 为 0 个，超出 4 元请自己写
一个具名 `delegate`。

### 2.2 语义

| 项 | 行为 |
|----|------|
| 持有量 | 一个实例持 0 或 1 个 handler（target + method）|
| 调用 | `f(arg)` 等价 `f.Invoke(arg)` |
| null | 调用 null delegate 抛 `NullReferenceException`（C# 一致）；用 `?.Invoke()` 短路 |
| 方法组转换 | `Action<int> a = SomeMethod;` / `obj.Method` 编译期合成 delegate 值 |
| Lambda 转换 | `Func<int,int> f = x => x*2;` |
| `+=` / `-=` | 单播类型上这两个操作符**只在 `event` 字段上**有意义（见 §5）|

### 2.3 嵌套 delegate

delegate 可以声明在 class 内部，与 C# 一致：

```z42
public class Btn {
    public delegate void OnClick(int x);
    public delegate int  Compute(int a, int b);
}
```

**外部引用用 dotted-path**：

```z42
public class Listener { public Btn.OnClick handler; }

Btn.OnClick MakePrinter() { return (int x) => Console.WriteLine(x); }
void Run(Btn.OnClick h, int v) { h(v); }
```

> **限制**：嵌套 delegate 在类内部**不能**用裸名（`OnClick handler;`）引用——嵌套类型
> 被展平成 `Outer+Inner`，裸名解析不到。类内外一律写 `Btn.OnClick`。

### 2.4 delegate 值的引用相等

`Std.DelegateOps.ReferenceEquals(a, b)` 判两个 delegate 值是否指向同一 target + method。
`MulticastAction.Unsubscribe(handler)` 与单播 event 的 `-=` 都走它。

## 3. 多播：三个 Multicast 类

### 3.1 API 表面

```z42
namespace Std;

public sealed class MulticastAction<T> {
    public int Count();                                              // 方法，不是属性
    public IDisposable Subscribe(Action<T> handler);                 // 裸 handler → fast path
    public IDisposable SubscribeAdvanced(ISubscription<Action<T>> wrapper);
    public void Unsubscribe(Action<T> handler);
    public void Invoke(T arg, bool continueOnException = false);
}

public sealed class MulticastFunc<TArg, TResult> {
    public int Count();
    public IDisposable Subscribe(Func<TArg, TResult> handler);
    public IDisposable SubscribeAdvanced(ISubscription<Func<TArg, TResult>> wrapper);
    public void Unsubscribe(Func<TArg, TResult> handler);
    public TResult[] Invoke(TArg arg, bool continueOnException = false);
}

public sealed class MulticastPredicate<T> {
    public int Count();
    public IDisposable Subscribe(Predicate<T> handler);
    public IDisposable SubscribeAdvanced(ISubscription<Predicate<T>> wrapper);
    public void Unsubscribe(Predicate<T> handler);
    public bool[] Invoke(T arg, bool continueOnException = false);
    public bool All(T arg);   // && 折叠（短路：遇 false 立即返回）
    public bool Any(T arg);   // || 折叠（短路：遇 true 立即返回）
}
```

**要点（和 C# 直觉不同的地方）**：

- `Count` 是**方法** `Count()`，不是属性；
- 裸 handler 走 `Subscribe`，`ISubscription` 包装走 **`SubscribeAdvanced`**（两个不同的
  方法名，不是重载）；
- `MulticastFunc` 的泛型参数叫 **`TArg` / `TResult`**（不是 `T` / `R`）；
- `Subscribe` / `SubscribeAdvanced` 返回 `IDisposable` 订阅 token，`Dispose()` 即退订。

### 3.2 调用语义

| 类型 | `Invoke` 返回 | `continueOnException=false`（默认） | `continueOnException=true` |
|---------|------|--------------------------------|------------------------|
| `MulticastAction<T>` | `void` | 首抛即停；抛原异常 | 全跑完；有失败则抛 `MulticastException` |
| `MulticastFunc<TArg,TResult>` | `TResult[]` | 首抛即停；前面的值丢失；抛原异常 | 全跑完；失败位置 = `default(TResult)`；有失败则抛 `MulticastException<TResult>` |
| `MulticastPredicate<T>` | `bool[]` | 同上 | 同上，失败位置 = `false`，抛 `MulticastException<bool>` |

**调用顺序 spec 化为 FIFO**：注册顺序 = 调用顺序，且 strong 通道全部先于 advanced 通道。
C# 把顺序定义为 "unspecified"，z42 钉死。

### 3.3 多线程安全

多播列表用 **COW snapshot**：`Invoke` 触发时拷贝当前订阅数组作为快照；触发期间的
`Subscribe` / `Unsubscribe` / `Dispose` **影响下次触发，不影响本次**。杜绝"边遍历边修改"竞态。

## 4. 订阅策略：`ISubscription` 包装

### 4.1 接口

```z42
namespace Std;

public interface ISubscription<TD> {
    TD Get();          // 取出被包装的 handler；仅当 IsAlive() 为 true 时有意义
    bool IsAlive();    // 是否仍活跃；invoke loop 的短路检查
    void OnInvoked();  // 每次 invoke 后回调；包装类用它更新自身状态
}
```

> `Get` / `IsAlive` 都是**方法**（不是属性），且 **`TD` 没有类型约束**——z42 没有统一的
> `Delegate` 基类，用错类型（如 `ISubscription<int>`）在包装类构造期 fail-fast。

### 4.2 内置包装类

| 类 | 语义 |
|---|---|
| `StrongRef<TD>` | 强引用直通；`IsAlive()` 恒 true，`OnInvoked()` 是 nop |
| `OnceRef<TD>` | 一次性：`OnInvoked()` 第一次后 `IsAlive()` 转 false |
| `WeakRef<TD>` | 弱引用 receiver；receiver 被 GC 后 `Get()` 返回 null、`IsAlive()` 转 false |
| `CompositeRef<TD>` | 组合：`int Modes` 位标志，`Once` 与 `Weak` 可叠加 |

```z42
namespace Std;

// mode 标志位（z42 暂无 [Flags] enum，用 int 常量 + 位运算）
public class ModeFlags {
    public static int None = 0;
    public static int Once = 1;
    public static int Weak = 2;
}

public sealed class CompositeRef<TD> : ISubscription<TD> {
    public int Modes;
    public CompositeRef(TD h, int modes);
    public CompositeRef<TD> WithMode(int additional);   // 累加 flag，复用同一对象
}
```

用法：

```z42
bus.SubscribeAdvanced(new OnceRef<Action<int>>(handler));
bus.SubscribeAdvanced(new WeakRef<Action<int>>(listener.OnEvent));
bus.SubscribeAdvanced(new CompositeRef<Action<int>>(handler, ModeFlags.Once | ModeFlags.Weak));
```

> **没有 `.AsWeak()` / `.AsOnce()` 链式扩展方法**——全仓零实现。构造包装类要显式
> `new`，或在已有 `CompositeRef` 上 `WithMode(...)` 累加。在 generic interface 上挂
> `impl` 扩展方法当前不可用，这是该写法缺席的直接原因。

**`WeakRef` / `CompositeRef(Weak)` 的退化**：包装类构造时把 handler 拆成
"弱引用 receiver + 函数名"。没有 receiver 的 handler（静态方法、无捕获 lambda）
**退化为 strong**，`IsAlive()` 恒 true。

**`WithMode` 的坑**：构造后追加 `Weak` **不会**重新拆解 handler。要 weak 就在构造器里
一次给全 mode。

### 4.3 自定义策略

实现 `ISubscription<TD>` 即可接入 `SubscribeAdvanced`——throttle / debounce / 调度到指定
线程等策略都是"再写一个类"，**不动 `event` 关键字、不加 attribute**。

## 5. `event` 关键字

### 5.1 角色

`event` **不假设 cardinality**，它的职责只有两条：

1. 把外部的 `+=` / `-=` 改写成 `add_X` / `remove_X` 调用；
2. 为 interface 中声明的 event 自动合成 `add_X` / `remove_X` 契约，class 侧写同名 event
   字段即满足契约。

cardinality 100% 由字段类型决定。

### 5.2 类型 → 行为分派

| 字段类型 | cardinality | `target.X += h` | `target.X -= h` | 类内触发 |
|---------|:----------:|----------------|----------------|---------|
| `event Action<T>` | 单播 | X 为 null → 设置；已设 → 抛 `InvalidOperationException` | 引用相等 → 清空；否则 no-op | `X?.Invoke(args)` |
| `event Func<TArg,TResult>` | 单播 | 同上 | 同上 | `X?.Invoke(args)` |
| `event Predicate<T>` | 单播 | 同上 | 同上 | `X?.Invoke(args)` |
| `event MulticastAction<T>` | 多播 | `Subscribe(h)` | `Unsubscribe(h)` | `X.Invoke(args)`，空链 = no-op |
| `event MulticastFunc<TArg,TResult>` | 多播 | 同上 | 同上 | `TResult[] r = X.Invoke(args)` |
| `event MulticastPredicate<T>` | 多播 | 同上 | 同上 | `bool[] b = X.Invoke(args)` |

**多播 event 字段自动初始化**：`public event MulticastAction<T> Bar;` 无初始化器时编译器
补 `= new MulticastAction<T>()`，所以永远不用写 `Bar?.Invoke(...)`。
**单播 event 字段不自动初始化**，默认 null。

**`+=` 只接受裸 handler**：合成的 `add_X` 只有**一个**重载，形参类型是对应的单播 delegate
（`MulticastAction<T>` → `Action<T>`，`MulticastFunc` → `Func`，`MulticastPredicate` →
`Predicate`）。`ISubscription` 包装**不能**经 `+=` 传入——走字段自身的
`bus.Clicked.SubscribeAdvanced(wrapper)`。

### 5.3 Desugar 规则

```z42
// === 单播事件 ===
public event Action<T> Foo;
// ⇣ 合成（字段保留原名，不改名成 _Foo）
public IDisposable add_Foo(Action<T> h) {
    if (this.Foo != null) throw new InvalidOperationException("single-cast event already bound");
    this.Foo = h;
    return Disposable.From(() => this.remove_Foo(h));
}
public void remove_Foo(Action<T> h) {
    if (DelegateOps.ReferenceEquals(this.Foo, h)) this.Foo = null;
}

// === 多播事件 ===
public event MulticastAction<T> Bar;        // 隐式 `= new MulticastAction<T>()`
// ⇣
public IDisposable add_Bar(Action<T> h) => this.Bar.Subscribe(h);
public void remove_Bar(Action<T> h) => this.Bar.Unsubscribe(h);
```

`Std.Disposable` 是通用 `IDisposable` 实现，`Disposable.From(Action)` 工厂返回一个
**幂等**（多次 `Dispose()` 只触发一次）的 token。

### 5.4 用户视角

```z42
public class Button {
    // 多播事件（最常见）
    public event MulticastAction<MouseArgs> Clicked;
    public event MulticastFunc<int, bool> Validate;

    // 单播事件（Cocoa 风格的回调属性）
    public event Action<KeyArgs> OnKeyDown;
    public event Func<DialogResult, bool> ShouldClose;

    void Fire(MouseArgs a) { this.Clicked.Invoke(a); }   // 类内直接触发
}

// 外部使用
button.Clicked += args => log(args.X);
button.Clicked -= someHandler;

// 高级订阅走字段自身（不经 +=）
button.Clicked.SubscribeAdvanced(new OnceRef<Action<MouseArgs>>(handler));

button.OnKeyDown += handleKey;       // 单播：set
button.OnKeyDown += otherHandler;    // ✗ InvalidOperationException
button.OnKeyDown -= handleKey;       // 清空

using (button.Clicked.Subscribe(handler)) {   // scoped 订阅
    DoStuff();
}   // 块结束 → token.Dispose() → 自动退订
```

### 5.5 interface event

```z42
public interface IBus {
    event MulticastAction<int> Clicked;
}

public class Bus : IBus {
    public event MulticastAction<int> Clicked;   // 写同名 event 字段即满足契约
}

IBus iface = new Bus();
iface.Clicked += x => Console.WriteLine(x);      // 经 interface 引用 +=，vtable 派发
```

interface 里的 event 声明合成 abstract 的 `add_X` / `remove_X` 签名，implementer **无需**
手写访问器。

### 5.6 访问控制：现状

> ⚠️ **`event` 字段目前没有编译期访问控制。** 设计上保留了 `E0414`
> （event 字段外部直接读 / 直接 `Invoke` / 直接赋值），**但编译器从不发射它**：
> 字段名集合在符号收集阶段被收集，TypeChecker 从不查询。今天外部代码可以直接
> `button.Clicked.Invoke(...)` 或 `button.OnKeyDown = h`，编译通过。
> 需要真的封闭时，把字段声明为 `private` 并另外暴露方法。

## 6. 异常聚合：`MulticastException`

### 6.1 类型

```z42
namespace Std;

// 无值版本 —— MulticastAction 抛
public class MulticastException : AggregateException {
    public Exception[] Failures;       // 与 FailureIndices 平行：第 i 个失败的异常
    public int[]       FailureIndices; // 与 Failures 平行：失败 handler 的 0-based 序号
    public int         TotalHandlers;  // 本次实际调用到的 handler 总数
    public int SuccessCount();         // = TotalHandlers - Failures.Length
}

// 带值版本 —— MulticastFunc / MulticastPredicate 抛
public class MulticastException<TResult> : MulticastException {
    public TResult[] Results;          // 与 Invoke 的返回数组等长；失败位置 = default(TResult)
}
```

`Failures` / `FailureIndices` 是**两条平行数组**（不是字典、不是元组数组）——用
`Failures.Length` 取个数，用同一个下标去两边取值。索引口径统一 0-based，strong 通道在前、
advanced 通道在后。

### 6.2 行为矩阵

| 场景 | 抛出 |
|------|------|
| `continueOnException=false`，0 异常 | 不抛 |
| `continueOnException=false`，≥1 异常 | 第一个异常**原样**抛出（不包装；与 C# 一致）|
| `continueOnException=true`，0 异常 | 不抛 |
| `continueOnException=true`，≥1 异常 | 抛 `MulticastException` / `MulticastException<TResult>` |

**关键性质**：默认行为零包装 → 单 handler 多播退化为 C# delegate 的所有用法都不变。

### 6.3 用法

```z42
// 默认 fail-fast —— 接具体异常，不用理会 MulticastException
try {
    validators.Invoke(42);
} catch (ArgumentException e) {
    log.Error($"validator threw: {e.Message}");
}

// continueOnException —— 完整审计
try {
    string[] results = validators.Invoke(42, continueOnException: true);
} catch (MulticastException<string> e) {
    Console.WriteLine($"{e.Failures.Length}/{e.TotalHandlers} 失败");
    int i = 0;
    while (i < e.Failures.Length) {
        Console.WriteLine($"  [{e.FailureIndices[i]}] {e.Failures[i].Message}");
        i = i + 1;
    }
    string[] partial = e.Results;   // 失败位置 = default(string) = null
}
```

`catch` 子句接受泛型类型（`catch (MulticastException<int> e)`）。

## 7. 性能特征

| 订阅方式 | Subscribe 时的分配 | Invoke 每 handler 的开销 |
|---------|--------------|------------------|
| 裸 `+= h` / `Subscribe(h)` | **0**（直接进 strong 数组）| 1 次直接调用，与单播 delegate 等价 |
| `SubscribeAdvanced(OnceRef)` | 1 个包装对象 | 1 次 `IsAlive()` + `Get()` + 调用 + `OnInvoked()` |
| `SubscribeAdvanced(WeakRef)` | 1 个包装对象 | 额外 1 次弱引用 upgrade + 重建 closure |
| `CompositeRef(Once\|Weak)` | **1**（组合成一个对象）| 同上 + 1 次 flag 检查 |

选择策略时的逃生口：

| 选项 | 用法 | 适用场景 |
|------|------|--------|
| 只接裸 delegate | 不对外暴露 `SubscribeAdvanced` | 极致性能事件（60fps 渲染 / 高频输入）|
| 直接用 `MulticastAction<T>` 字段 | 不写 `event` 关键字 | 不需要 `+=` 糖时 |
| 退化到单播 `event Action<T>` | 至多 1 handler | 只有 1 个 listener 的场景 |

## 8. Promise / 一次性 + 回放 —— 不属于 event

> C# 里"事件触发一次后晚来的订阅者拿历史值"通常用 `TaskCompletionSource<T>` 表达，
> 而不是 event。z42 同样把这两件事分开。

`OnceRef` 提供的是 **per-subscription once**：

> "**这个**订阅触发一次后自动失活，其他订阅者不受影响"

z42 **不提供** per-event once with replay：

> "**事件本身**只触发一次，晚来的订阅者立即拿到历史值"

后者留给独立的 `Promise<T>` / `TaskCompletionSource<T>`（L3 async 落地后的独立议题），
当前 stdlib 中不存在。

## 9. 改进项映射（C# → z42）

### 9.1 delegate 侧

| C# 缺陷 | z42 改良 |
|--------|---------|
| `Action` × 16 + `Func` × 17 × 16 的 stdlib 爆炸 | 只提供 0–4 元；超出自己写具名 `delegate` |
| 多播返回值只保留最后一个 | `MulticastFunc.Invoke` 返回 `TResult[]` |
| 多播链中某 handler 抛异常 → 后续不调用 | `continueOnException=true` + `MulticastException` |
| 匿名 lambda 不可比较，难以退订 | `Subscribe` 返回 `IDisposable` token，不依赖 `==` |
| 变型必须显式 `in`/`out` | 暂不支持变型（见 §10）|

### 9.2 event 侧

| C# 缺陷 | z42 改良 |
|--------|---------|
| lapsed-listener 内存泄漏 | `IDisposable` token + `WeakRef` 包装 |
| `event?.Invoke(...)` 模板冗余 | 多播 event 字段自动初始化，空链 = no-op |
| invoke 异常不隔离 | `continueOnException` + `MulticastException` |
| `EventHandler<T>` 强制 sender + args | 推荐 plain `Action<T>`，需要 sender 时显式塞进 T |
| interface event 必须显式写 add/remove | 自动合成 |
| 无 once / weak / scoped 订阅 | `OnceRef` / `WeakRef` / `CompositeRef` + `IDisposable` |
| invoke 多线程不安全 | COW snapshot |

## 10. 明确不做

| 项 | 理由 |
|----|------|
| Variadic generics | 复杂度过高；C# / Rust 都没做 |
| 协变 / 逆变（`<in T, out R>`）| 推迟到 L3 后期 |
| `delegate.GetInvocationList()` | 多播是独立类型，不需要此 API |
| `BeginInvoke` / `EndInvoke` | 用 L3 async/await 替代 |
| `Delegate` / `MulticastDelegate` 抽象基类 | 类型分离设计下无意义 |
| 反射式 `DynamicInvoke` | 与反射轨道一并设计 |
| `[Weak]` / `[Once]` attribute | 由 `ISubscription` 包装替代；声明端零 attribute |
| `weak event` / `once event` 修饰符 | 同上；订阅端包装完全替代 |
| per-event once with replay | 不属于 event，见 §8 |

## 11. 与 C# 视觉对照

```z42
// === C# 用户看 z42 代码 ===
public class Button {
    public event MulticastAction<MouseArgs> Clicked;   // 比 C# `event Action<T>` 多一个 Multicast 前缀
    public event Action<KeyArgs> OnKeyDown;            // 单播事件 —— C# 无对应（C# Action 是多播）
}

button.Clicked += args => Console.WriteLine(args.X);   // 与 C# 一字不差
button.Clicked -= someHandler;                         // 与 C# 一字不差

// z42 新增：异常不中断 + 完整审计（C# 必须自己写 GetInvocationList 循环）
button.Clicked.Invoke(args, continueOnException: true);

// z42 新增：弱引用 / 一次性订阅（走字段，不走 +=）
button.Clicked.SubscribeAdvanced(new WeakRef<Action<MouseArgs>>(handler));
button.Clicked.SubscribeAdvanced(new OnceRef<Action<MouseArgs>>(handler));
```

**迁移要点**：

- `event Action<T>` → `event MulticastAction<T>`（语义诚实的代价）；
- `Count` 属性 → `Count()` 方法；
- 手写 `WeakReference + WeakEventManager` → `WeakRef` 包装；
- 手写 `GetInvocationList` 循环 → `continueOnException: true` + `MulticastException`。

## 关联文档

- [访问权限控制](access-control.md) —— `public` / `private` 等修饰符（event 字段当前不受额外限制）
- [泛型约束](generic-constraints.md) —— `ISubscription<TD>` 的 `TD` 为什么没有约束
- [命名约定](../conventions/naming.md) —— delegate / event 类型的命名规则
