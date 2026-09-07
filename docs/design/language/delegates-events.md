# z42 Delegates & Events 设计

> **状态**：D1 + D2a + D2b + D-5 + D2c-多播 + D2c-interface + D2d-1 + D2d-2-Action + D-1a + D-7 单播 event + D-6 嵌套 delegate dotted-path 外部引用 + D-7-residual 单播 event IDisposable token + 严格 access control（E0414）+ **instance method group conversion + D-1b WeakRef ISubscription wrapper + CompositeRef.Mode.Weak** 已落地（最新 2026-05-04）；D2d-1 = `Std.MulticastFunc<T,R>` + `Std.MulticastPredicate<T>` 双 vec 多播类（含 Predicate.All / Any 短路）+ event keyword 类型校验放宽接受三种 multicast 类型；**D2d-2-Action = `Std.AggregateException` + `Std.MulticastException` 异常聚合类 + `MulticastAction.Invoke(continueOnException=true)` 全跑完后聚合抛**（Func/Predicate 聚合留 D-8b follow-up）；D1 = `delegate` 关键字 / 单播 / 方法组缓存 / stdlib delegate 类型 / TSIG 跨 zpkg 导出；D2a = `MulticastAction<T>` 基础多播 + COW snapshot + IDisposable token + fail-fast 异常路径；D2b = `ISubscription<TD>` 接口 + StrongRef / OnceRef / CompositeRef wrapper + ModeFlags 位运算 mode；MulticastAction 双 vec strong / advanced 通道 + SubscribeAdvanced 重载；D-5 = `__delegate_eq` builtin + `MulticastAction.Unsubscribe(handler)`；D2c-多播 = `event` 关键字（FieldDecl.IsEvent）+ 多播 event field auto-init `new MulticastAction<T>()` + 合成 `add_X`/`remove_X` + `+=`/`-=` desugar；**D2c-interface (I10) = interface body 接受 `event MulticastAction<T> X;` 声明并合成 instance abstract `add_X`/`remove_X` MethodSignatures；class 实现 interface 同名 event 即满足契约**。WeakRef 已由 D-1a / D-1b 实施（2026-05-04）。D2c-单播（event Action<T> 字段 + throw on double-bind + 严格 access control）/ D2d（MulticastFunc + MulticastException）待实施。
> **历史状态**：L2/L3 前瞻性设计草案（2026-05-01 定稿）
> **定位**：与 `generics.md` / `concurrency.md` / `static-abstract-interface.md` 同级 — 长期规范，等具体 docs/spec/changes/ 启动时实施
> **参考**：C# delegate/event（视觉与心智蓝本）+ Rust/Kotlin/Swift（单播/多播分离）+ Rx/Combine `Subject` 体系（订阅策略 wrapper）
> **核心选型**：
> - 单播 / 多播**类型分离**（不走 C# `MulticastDelegate` 统一路径）
> - 保 C# 视觉表层（`delegate` / `event` / `+=` / `-=`）
> - **订阅策略全部走 `ISubscription` wrapper**（零 attribute / 零修饰符堆叠）
> - **极致默认性能**（strong 订阅与 C# delegate 等价）
>
> 阅读对象：使用者视角（语法、心智模型、API 形状）+ 实现者性能要求。
> IR opcode、zbc 元数据、JIT 内联等纯实现细节在各 spec 落地时补齐。

---

## 1. 设计目标

| # | 目标 | 衡量标准 |
|---|------|---------|
| 1 | **C# 用户零迁移成本** | 表层语法（`delegate` / `event` / `+=` / `-=`）与 C# 视觉一致 |
| 2 | **消除 C# delegate/event 长期痛点** | 对应 [§10 改进项映射](#10-改进项映射c-缺陷--z42-改良) 全部 14 项 |
| 3 | **API 表面极简** | 1 个 `event` 关键字；0 个修饰符 attribute；订阅策略全部 wrapper |
| 4 | **关键字 / 修饰符不堆叠** | 永远不会出现 `[Weak] [Once] event ...` 这种链式修饰符灾难 |
| 5 | **默认路径零开销** | 裸 `+= h` 订阅与 C# delegate 性能等价 |
| 6 | **未来扩展不动语法** | throttle / debounce / dispatch 等新策略加 wrapper 类即可 |
| 7 | **单播 / 多播视觉与心智统一** | 都用 `event TYPE NAME` + `+=` / `-=`；类型决定语义 |

---

## 2. 选型决策

### 2.1 单播 vs 多播路径选择

| 路径 | 描述 | 主要语言 | z42 决策 |
|------|------|---------|---------|
| **A 统一类型**（C#） | `Action<T>` 一个类型既可 1 个也可 N 个 handler | C# / F# | ❌ 不采用 |
| **B 分离类型**（主流）| 单播 `Func<T,R>` + 独立多播 `MulticastX<T>` 类型 | Rust / Kotlin / Swift / Java / TS / Go / Python | ⭐ **采用** |
| C 反应式流 | 多播完全交给 Flow / Subject | Kotlin Flow / Combine | ❌ 工程量过大；留 L3 |

### 2.2 订阅策略表达方式

| 路径 | 描述 | z42 决策 |
|------|------|---------|
| 关键字修饰 | `weak event` / `once event` | ❌ 关键字堆叠 |
| Attribute 标注 | `[Weak] event` / `[Once] event` | ❌ 仍是声明端责任倒置 |
| **wrapper 包装** | `+= h.AsWeak().AsOnce()` | ⭐ **采用** |

### 2.3 关键决策汇总

| ID | 决策 | 原因 |
|----|------|------|
| K1 | 单播 = 编译期 `delegate` 类型；多播 = `sealed class` | 两类语义不同，类型层面就分开 |
| K2 | 多播命名 = `Multicast` + 单播名 | `MulticastFunc<T,R>` / `MulticastAction<T>` / `MulticastPredicate<T>` 严格对称单播 |
| K3 | `event` 关键字统一单播 + 多播 | 类型决定 cardinality；`+=`/`-=` 行为按类型 dispatch |
| K4 | `event Action<T>` = 单播事件；`event MulticastAction<T>` = 多播事件 | 类型 100% 诚实；无歧义 |
| K5 | `Invoke(arg, bool continueOnException = false)` | 默认 fail-fast = C# 语义 |
| K6 | `continueOnException=false` 时**不包装**异常 | 单 handler 场景与 C# 100% 一致 |
| K7 | `continueOnException=true` 时抛 `MulticastException` | opt-in 包装 |
| K8 | `MulticastException<R>` 携带 `Results: R[]`（失败位置 = `default(R)`） | 简单；用户查 `Failures` 索引判定真成功 |
| K9 | 不做 `TryInvoke` / `InvokeAggregate` | `MulticastException` 已承载诊断；用 LINQ 折叠数组 |
| K10 | `MulticastPredicate` 提供 `All` / `Any`（短路求值）| LINQ 习惯；性能优势 |
| K11 | **订阅策略全部走 wrapper** —— `StrongRef` / `WeakRef` / `OnceRef` / `CompositeRef` | 责任归属订阅者；零 attribute；无限可扩展 |
| K12 | 不做 variadic generics | 复杂度过高；改为脚本生成 N arity 类型 |
| K13 | `event` 关键字 = 多播类型字段的访问控制糖衣 | C# 视觉一致；底层无新机制 |
| K14 | **三个性能优化为实现强约束** | strong 路径必须达 C# 等价；详见 §9 |

---

## 3. 单播：`Func` / `Action` / `Predicate`

### 3.1 类型定义

```z42
namespace Std;

public delegate void Action<T>(T arg);
public delegate R    Func<T, R>(T arg);
public delegate bool Predicate<T>(T arg);
```

`delegate` 是新关键字（D1 spec 引入），声明编译期专属的 callable 类型。

### 3.2 语义

| 项 | 行为 |
|----|------|
| 一个 delegate 实例持有 0 或 1 个 handler（target + method） | |
| 调用 `Invoke(arg)` = 调用那一个 handler | |
| **null delegate** → 抛 `NullReferenceException`（C# 一致）；用 `?.Invoke()` 短路 | |
| 方法组转换 | `Action<int> a = SomeMethod;` 编译期合成 delegate 实例 |
| Lambda 转换 | `Func<int,int> f = x => x*2;` 编译期合成 delegate 实例 |
| 调用语法糖 | `f(arg)` 等价 `f.Invoke(arg)` |
| 不支持 `+=` / `-=`（非 event 上下文） | 单播类型上这两个操作符直接编译错误（提示用 `MulticastX` 或 `event` 字段）|

### 3.3 方法组转换缓存（I12，D1 内置优化）

```z42
button.Click += OnClick;     // OnClick 是方法 → 合成 Action<MouseArgs> 实例
```

编译器在 call site 合成的 delegate 实例**自动缓存到 static slot**，避免每次进入作用域都新分配 → 消除 C# 高频 callback 路径的 GC 压力。

### 3.4 多 arity 类型（脚本生成）

`Action<T1, ..., T16>` × 16 + `Func<T1, ..., T16, R>` × 17 × 16 共 ~290 个类型由 `tools/gen-delegates.z42` 脚本生成；每次 stdlib 构建前自动 regen。手写维护成本为 0。

> **不做 variadic generics**：参考 C#（20+ 年未做）/ Rust（RFC 1935 仍 blocked）；脚本生成 = 同样的用户体验，零语言复杂度。

### 3.5 嵌套 delegate 与 dotted-path 外部引用（D-6, 2026-05-04）

delegate 可以声明在 class 内部（嵌套形式），与 C# `class Btn { public delegate void OnClick(int x); }` 一致：

```z42
public class Btn {
    public delegate void OnClick(int x);
    public delegate int  Compute(int a, int b);
}
```

**类内部**继续支持 simple-name 引用（`OnClick handler;`）；**外部**用 dotted-path 引用：

```z42
// 字段类型
public class Listener { public Btn.OnClick handler; }

// 参数 + 返回类型
Btn.OnClick MakePrinter() { return (int x) => Console.WriteLine(x); }
void Run(Btn.OnClick h, int v) { h(v); }
```

> 🔴 **本节的「实现细节」与上面「类内部继续支持 simple-name 引用」均已过期**（2026-09-07
> `fix-binder-emitter-gaps-batch2` 实测校正）。下面四条描述的是 **C# 编译器时代**的实现：
> z42 自举迁移时 `MemberType` AST 节点没有移植，`MemberParser._parseMemberBody` 也没有
> `delegate` 分支 ⇒ **嵌套 delegate 声明在自举编译器里根本没被解析过**（`delegate` 被当成一个
> 类型名，整条声明报废、该 delegate 类型整体消失）。本节的 D-6 golden
> （`src/tests/delegates/nested_delegate_dotted.z42`）因此带着 12+ 条编译错误却"通过"了四个月
> —— `--emit-zbc` 吞诊断所致。现行实现与限制（走 `NestedFlatten` 的 `Outer+Inner` 展平、
> **类内部裸名不解析**、泛型嵌套已可用）见
> [`docs/book/src/compiler/source-compile.md`「名字与『拿名字当键』的三条纪律」](../../book/src/compiler/source-compile.md)。
> 按 [doc-system.md 决策 D2](../../agent/rules/doc-system.md)，`docs/design/` 不再更新，此处只留
> 指针、不改写正文。

实现细节（**C# 时代，已不适用**）：
- AST 节点 `MemberType(Left, Right)` 表达 `Outer.Inner` —— 与 expression-level member access 同结构
- TypeParser 在 `NamedType / GenericType` 后 lookahead `.` + Identifier，左结合 wrap MemberType 链
- SymbolTable 在嵌套 delegate 注册时早已写入 qualified key（`Btn.OnClick` / `Btn.OnClick$N`），ResolveType 把 `MemberType` 拍平成该 key 查 `Delegates` map
- v1 仅支持 1 层嵌套（class 内的 delegate）+ 非泛型嵌套；深嵌套 / 嵌套泛型 delegate 暂不支持，留待后续 spec 时再激活已注册路径

---

## 4. 多播：`MulticastAction` / `MulticastFunc` / `MulticastPredicate`

### 4.1 类型定义

```z42
namespace Std;

public sealed class MulticastAction<T> {
    public int Count { get; }
    
    public IDisposable Subscribe(ISubscription<Action<T>> sub);   // 完整路径
    public IDisposable Subscribe(Action<T> handler);               // 便利 → 走 fast path
    public void Unsubscribe(Action<T> handler);
    
    public void Invoke(T arg, bool continueOnException = false);
}

public sealed class MulticastFunc<T, R> {
    public int Count { get; }
    public IDisposable Subscribe(ISubscription<Func<T, R>> sub);
    public IDisposable Subscribe(Func<T, R> handler);
    public void Unsubscribe(Func<T, R> handler);
    public R[] Invoke(T arg, bool continueOnException = false);
}

public sealed class MulticastPredicate<T> {
    public int Count { get; }
    public IDisposable Subscribe(ISubscription<Predicate<T>> sub);
    public IDisposable Subscribe(Predicate<T> handler);
    public void Unsubscribe(Predicate<T> handler);
    public bool[] Invoke(T arg, bool continueOnException = false);
    public bool All(T arg);   // && 折叠（短路）
    public bool Any(T arg);   // || 折叠（短路）
}
```

### 4.2 调用语义

| 多播类型 | `Invoke` 返回 | continueOnException=false（默认） | continueOnException=true |
|---------|------|--------------------------------|------------------------|
| `MulticastAction<T>` | `void` | 首抛即停；抛原异常 | 全跑完；多失败聚合抛 `MulticastException` |
| `MulticastFunc<T, R>` | `R[]` | 首抛即停；前面值丢失；抛原异常 | 全跑完；返回 `R[]`（含 `default(R)` 占位）；多失败聚合抛 `MulticastException<R>` |
| `MulticastPredicate<T>` | `bool[]` | 同上 | 同上 |

**调用顺序 spec 化为 FIFO**（注册顺序 = 调用顺序，I8 改进项）。C# 模糊定义为"unspecified"，z42 钉死。

### 4.3 多线程安全（I14，D2 内置）

多播链表用 **COW snapshot** —— `Invoke` 触发时拷贝当前 invocation list 作为 snapshot；`Subscribe` / `Unsubscribe` 在 invoke 期间生效但**不影响本次触发**。这与并发模型对齐，避免"边遍历边修改"竞态。

---

## 5. 订阅策略：`ISubscription` wrapper 体系

### 5.1 接口定义

```z42
namespace Std;

public interface ISubscription<TDelegate> where TDelegate : Delegate {
    TDelegate? TryGet();           // null = 已失效（如 weak ref target 被 GC，或 once 已触发）
    bool IsAlive { get; }
    void OnInvoked();              // 每次 invoke 后回调；wrapper 用此更新自身状态
}
```

### 5.2 内置 wrapper 实现

```z42
namespace Std;

// 强引用包装（默认；裸 handler 自动转此；fast path 通常不真实例化）
public sealed class StrongRef<TDelegate> : ISubscription<TDelegate> 
    where TDelegate : Delegate {
    public StrongRef(TDelegate handler);
}

// 弱引用包装 —— 弱持 handler.Target；handler.Method 仍强引用
// 注意：静态方法 handler 无 target；weak 退化为 strong（详见 §13 开放问题 #8）
public sealed class WeakRef<TDelegate> : ISubscription<TDelegate>
    where TDelegate : Delegate {
    public WeakRef(TDelegate handler);
}
// 2026-05-04 D-1b 实施：WeakRef **不**强持原 handler（那会反向锁住 receiver
// 因为 Closure 自己强持 env=[receiver]，weak 失效）。改为构造时拆解为：
//   - WeakHandle to receiver（弱引用，不锁 GC）
//   - fnName: string（thunk 函数全限定名）
// Get 时通过 `DelegateOps.MakeClosure(fnName, [WeakHandle.Upgrade(...)])` 重建
// 一个新的 Closure。Listener 被 GC 回收后 Upgrade 返回 null，Get 返回 null，
// IsAlive 返回 false，MulticastAction.Invoke 自然短路。
//
// 配套基础（D-1b Phase 1）：instance method group conversion `obj.Method`
// 编译期合成 thunk function `__mg_thunk_<Class>_<Method>$<arity>__`，emit
// `MkClos(thunk, [recv])`。Thunk 内部 vcall env[0].method(args)。这套约定
// 让 receiver 落在 env[0]，配合 `__delegate_target` builtin 提取出来弱化。
//
// 配套 corelib builtins（D-1b Phase 2）：
//   - `__delegate_target(d) -> Object?`：从 Closure 提取 env[0]
//   - `__delegate_fn_name(d) -> string`：提取 fn_name
//   - `__make_closure(name, env) -> Closure`：用 (name, env array) 重建 Closure
// stdlib 端通过 `Std.DelegateOps` 静态类暴露这三个。

// 一次性包装 —— OnInvoked 第一次后 IsAlive = false
public sealed class OnceRef<TDelegate> : ISubscription<TDelegate>
    where TDelegate : Delegate {
    public OnceRef(TDelegate handler);
}

// 组合包装 —— 任意策略 flag 组合（fusion，详见 §9 性能优化）
public sealed class CompositeRef<TDelegate> : ISubscription<TDelegate>
    where TDelegate : Delegate {
    [Flags] public enum Mode { Weak = 1, Once = 2, /* future: Throttled = 4, OnUiThread = 8, ... */ }
    public Mode Modes { get; }
    public CompositeRef(TDelegate handler, Mode modes);
    public CompositeRef<TDelegate> WithMode(Mode additional);   // 累加 flag，复用对象
}
```

### 5.3 便利扩展（impl 块）

```z42
// 让裸 delegate 直接获得 .AsWeak() / .AsOnce()
impl<TDelegate> TDelegate where TDelegate : Delegate {
    public CompositeRef<TDelegate> AsWeak() => new CompositeRef<TDelegate>(this, Mode.Weak);
    public CompositeRef<TDelegate> AsOnce() => new CompositeRef<TDelegate>(this, Mode.Once);
}

// 让 ISubscription 链式叠加 —— 自动 fusion（详见 §9.2）
impl<TDelegate> ISubscription<TDelegate> where TDelegate : Delegate {
    public ISubscription<TDelegate> AsWeak() {
        if (this is CompositeRef<TDelegate> c) return c.WithMode(Mode.Weak);
        return new CompositeRef<TDelegate>(/* unwrap inner */, Mode.Weak);
    }
    public ISubscription<TDelegate> AsOnce() { /* 同上 */ }
}
```

### 5.4 策略组合矩阵

| 用户写法 | 实际策略 | 分配次数（优化后）|
|---------|---------|--------------|
| `+= h` | strong | **0**（fast path）|
| `+= h.AsWeak()` | weak | 1 (CompositeRef) |
| `+= h.AsOnce()` | once | 1 (CompositeRef) |
| `+= h.AsWeak().AsOnce()` | weak ∧ once | **1**（融合后）|
| `+= h.AsOnce().AsWeak()` | weak ∧ once | **1**（融合后；顺序无关）|

### 5.5 未来扩展示例（不在 D2 spec，仅展望）

每个新策略 = stdlib 加一个新 wrapper 类 + impl 块的扩展方法 + 对应 `Mode` flag。**不动 event 关键字 / 不加 attribute**。

```z42
// 节流：100ms 内多次触发只调一次
button.MouseMove += handler.AsThrottled(ms: 100);

// 去抖：连续触发，最后一次后 200ms 才调
input.TextChanged += saveHandler.AsDebounced(ms: 200);

// 调度到 UI 线程
worker.Progress += updateLabel.OnUiThread();

// 任意组合（融合为单 CompositeRef）
worker.Progress += updateLabel
    .AsWeak()
    .AsThrottled(ms: 50)
    .OnUiThread();
```

---

## 6. `event` 关键字 — 统一的访问控制糖

### 6.1 角色定位

`event` 不假设 cardinality —— 它的**唯一**职责：
1. 把字段的 `Invoke` / 直接写入访问限制为 **declaring class 内部可达**
2. 把外部 `+=` / `-=` desugar 为对应的 `Subscribe` / `Unsubscribe` 调用
3. 为 interface 中声明的 event 自动合成 add/remove 实现

cardinality 100% 由字段类型决定。

### 6.2 类型 → 行为分派表

| 字段类型 | cardinality | `target.X += h` | `target.X -= h` | 内部触发 |
|---------|:----------:|----------------|----------------|---------|
| **`event Action<T>`** | 单播 | 若 X = null → 设置；若 X 已设 → 抛 `InvalidOperationException` | 若 X == h → 清空；否则 no-op | `X?.Invoke(args)` |
| **`event Func<T, R>`** | 单播 | 同上 | 同上 | `R? r = X?.Invoke(args)` |
| **`event Predicate<T>`** | 单播 | 同上 | 同上 | `bool? b = X?.Invoke(args)` |
| **`event MulticastAction<T>`** | 多播 | `Subscribe(h)` 加入链 | `Unsubscribe(h)` 从链中移除 | `X.Invoke(args)`，空链 = no-op |
| **`event MulticastFunc<T, R>`** | 多播 | 同上 | 同上 | `R[] r = X.Invoke(args)` |
| **`event MulticastPredicate<T>`** | 多播 | 同上 | 同上 | `bool[] b = X.Invoke(args)` |

**Wrapper 走多播 overload**（仅多播事件接受 `+= h.AsWeak()`）：

```z42
button.Clicked += handler.AsWeak();   // ✅ event MulticastAction<T> 接受 ISubscription
button.OnKeyDown += handler.AsWeak(); // ❌ event Action<T> 不接受 wrapper（单播无策略概念）
```

### 6.3 Desugar 规则

```z42
// === 单播事件 desugar ===
public event Action<T> Foo;
// ⇣
private Action<T>? _Foo;
public IDisposable add_Foo(Action<T> h) {
    if (_Foo != null) throw new InvalidOperationException("single-cast event already bound");
    _Foo = h;
    return new Disposable(() => { if (_Foo == h) _Foo = null; });
}
public void remove_Foo(Action<T> h) { if (_Foo == h) _Foo = null; }
// 内部访问：直接读 _Foo（可 ?.Invoke）

// === 多播事件 desugar ===
public event MulticastAction<T> Bar;
// ⇣
private MulticastAction<T> _Bar = new MulticastAction<T>();
public IDisposable add_Bar(Action<T> h) => _Bar.Subscribe(h);
public IDisposable add_Bar(ISubscription<Action<T>> s) => _Bar.Subscribe(s);
public void remove_Bar(Action<T> h) => _Bar.Unsubscribe(h);
// 内部访问：_Bar.Invoke
```

### 6.4 用户视角

```z42
public class Button {
    // 多播事件（最常见）
    public event MulticastAction<MouseArgs> Clicked;
    public event MulticastFunc<int, bool> Validate;
    
    // 单播事件（Cocoa-style 回调属性）
    public event Action<KeyArgs> OnKeyDown;
    public event Func<DialogResult, bool> ShouldClose;
}

// 外部使用
button.Clicked += args => log(args.X);
button.Clicked += handler.AsWeak();              // weak 订阅
button.Clicked += handler.AsOnce();              // 一次性订阅
button.Clicked += handler.AsWeak().AsOnce();    // 组合

button.OnKeyDown += handleKey;                   // 单播：set
button.OnKeyDown += otherHandler;                // ❌ InvalidOperationException
button.OnKeyDown -= handleKey;                   // 清空

using (button.Clicked.Subscribe(handler)) {     // scoped 订阅
    DoStuff();
}   // 块结束 → 自动 Unsubscribe
```

### 6.5 改进项

| 改进 | 说明 |
|------|------|
| **I4 null-safe 默认** | 多播 event 字段初始化为空 `MulticastX<T>` 实例；`Invoke` on empty = no-op；杜绝 `event?.Invoke` 模板代码 |
| **I10 interface event 默认实现** | interface 中声明 event 自动合成 add/remove，implementer 无需写访问器 |
| **I11 去 `EventHandler<T>` 强制** | 推荐 plain `Action<T>`；`sender` 需要时显式塞 T 字段 |
| **I14 COW 多线程安全** | invoke 期间 add/remove 不影响本次触发 |
| **I_AC 严格 access control（D-7-residual, 2026-05-04）** | event field 外部任意 read / 直接 invoke / 直接赋值都报 `E0414`；只允许 `+=` / `-=`（已在 BindAssign 阶段 desugar 到 `add_X` / `remove_X`，不命中 BindMemberExpr）。`EventFieldNames` HashSet 在 SymbolCollector 已收集，TypeChecker O(1) 查询；类内部访问不限。多播 + 单播双路径同样生效 |
| **I_TOK 单播 IDisposable token（D-7-residual, 2026-05-04）** | 单播 `add_X` 由 void 返回值升级为 `IDisposable`，与多播对称。实现：合成的 `add_X` body 末尾 `return Disposable.From(() => this.remove_X(h))`，新 stdlib 类 `Std.Disposable : IDisposable` 提供 `From(Action)` 工厂 + 幂等 Dispose。Interface 端 `add_X` 同款返回 IDisposable |

---

## 7. 异常处理：`MulticastException`

### 7.1 类型定义

```z42
namespace Std;

// 无值版本 — MulticastAction 抛
public class MulticastException : AggregateException {
    public Exception[] Failures;          // 与 FailureIndices 平行：第 i 个失败异常
    public int[]       FailureIndices;    // 与 Failures 平行：失败 handler 在 strong/advanced 序列中的 0-based 索引
    public int         TotalHandlers;     // strong + advanced 活跃订阅总数
    public int SuccessCount() => TotalHandlers - Failures.Length;
}

// 带值版本 — MulticastFunc / MulticastPredicate 抛（D-8b-1, 2026-05-07 落地）
public class MulticastException<R> : MulticastException {
    public R[] Results;   // 与 Invoke 返回的 R[] 等长；失败位置 = default(R)（待 D-8b-3 Phase 2 generic-T `default(R)`）
}
```

继承 `AggregateException`（z42.core 已规划）。

**实施记录（D-8b-1，2026-05-07）**：

- 泛型类型 `MulticastException<R>` 已 land 进 `src/libraries/z42.core/src/Exceptions/MulticastException.z42`，与已有非泛型 `MulticastException` 共存通过 add-class-arity-overloading（D-8b-0）的 shadow-only mangling：IR-side `MulticastException$1`，源码 `MulticastException`
- 构造器使用 ctor delegation `: base(failures, indices, totalHandlers)`（D-1a 落地的 `: base(...)` 链式 ctor），由父类负责设置 `Failures` / `FailureIndices` / `TotalHandlers`，子类构造体只设置 `Results`
- 对外字段为 plain field（非属性），与现有 stdlib 风格一致；`SuccessCount()` 走 method 而非 property（z42 暂无 property setter / getter 语法）
- 平行数组 `Failures: Exception[]` + `FailureIndices: int[]` 而非 `Dictionary<int, Exception>` —— 写规范时还有 Dictionary 形态的 K8 提案，实际实施 D2d-2-Action 时为避开 Dictionary 依赖改为平行数组，本节同步规范现状

**K8 完整语义实施记录（switch-multicast-funcpredicate-to-generic-exception，2026-05-07）**：

- `MulticastFunc<T,R>.Invoke(continueOnException=true)` 与 `MulticastPredicate<T>.Invoke(continueOnException=true)` 切换到完整聚合路径：失败位置 `result[i] = default(R)` / `default(bool)`（依赖 D-8b-3 Phase 2 的运行时 type_args 解析），失败收尾抛 `MulticastException<R>` / `MulticastException<bool>`，携带 `Results: R[]` 与平行 `Failures` / `FailureIndices`
- Scope 扩展：本 spec 同时扩 catch 子句 parser 接受泛型类型（`catch (MulticastException<int> e)`），mangle 到 `Name$N` 与 D-8b-0 类注册键对齐；TypeChecker 路径已就绪
- JIT 跨步：新增 `jit_default_of` helper 镜像 interp，让 stdlib 的 `default(R)` 在 JIT 模式下也被 emit；但 `jit_obj_new` 仍未 propagate type_args，故 JIT-allocated 实例的 `default(T)` 仍走 graceful Null。新 2 个 golden（`multicast_func_aggregate` / `multicast_predicate_aggregate`）带 `interp_only` 标记
- 用户能写：`try { ... } catch (MulticastException<int> e) { e.Results[i] /* 失败位 = default(R) */ }`

### 7.2 行为矩阵

| 场景 | 抛出 |
|------|------|
| `continueOnException=false`，0 异常 | 不抛 |
| `continueOnException=false`，1 异常 | **直接抛该异常**（不包装；C# 一致） |
| `continueOnException=false`，N 异常 | 第 1 个抛即停，与上同 |
| `continueOnException=true`，0 异常 | 不抛 |
| `continueOnException=true`，N≥1 异常 | 抛 `MulticastException` / `MulticastException<R>`，含全部 `Failures` + `Results` |

**关键性质**：默认行为零包装 → 单 handler 多播退化为 C# delegate 的所有用法都不变。

### 7.3 用户体验

```z42
// 默认 fail-fast — 用户接具体异常，不需理 MulticastException
try {
    validators.Invoke(42);
} catch (ArgumentException e) {
    log.Error($"validator threw: {e.Message}");
}

// continueOnException — 完整审计
try {
    string[] results = validators.Invoke(42, continueOnException: true);
} catch (MulticastException<string> e) {
    Console.WriteLine($"{e.Failures.Count}/{e.TotalHandlers} 失败");
    foreach (var (idx, ex) in e.Failures) {
        Console.WriteLine($"  [{idx}] {ex.GetType().Name}: {ex.Message}");
    }
    string[] partial = e.Results;   // 失败位置 = null
}
```

---

## 8. Promise / 一次性 + 回放 — 不属于 event 范畴

> 在 C# 里，"事件触发一次后晚来订阅者拿历史值"通常通过 `TaskCompletionSource<T>` 表达，而非 event。z42 同样把这分开。

`OnceRef` wrapper 提供 **per-subscription once**：
> "**这个**订阅触发一次后自动解绑，其他订阅者不受影响"

不提供 **per-event once with replay**：
> "**事件本身**只触发一次，晚来订阅者立即拿历史值"

后者用独立的 `Promise<T>` 类型表达（后续 stdlib spec 单独立项；L3 async 落地后用 `TaskCompletionSource<T>`）：

```z42
public class App {
    private TaskCompletionSource<Config> _initialized = new();
    public Task<Config> Initialized => _initialized.Task;
    
    void StartUp() => _initialized.SetResult(cfg);
}

// 使用方
var cfg = await app.Initialized;   // 晚来 awaiter 立即拿历史值
```

---

## 9. 性能注释（实现强约束）

> 朴素实现 wrapper 模式会让 invoke per-handler 比 C# delegate 慢 ~3-4×，且 `AsWeak().AsOnce()` 多次包装会有 GC 压力。**以下三个优化必须实现**，使默认 strong 路径与 C# 等价。

### 9.1 优化 A — 双存储 fast / slow 通道

> 99% 订阅是 strong；为 strong 走零开销 fast path。

VM 内部 `MulticastStorage` 必须分两个 vec：

```rust
struct MulticastStorage<T> {
    strong:   Vec<Action<T>>,                              // ⚡ fast path（无 wrapper）
    advanced: Vec<Box<dyn ISubscription<Action<T>>>>,      // 🐢 slow path（weak/once/...）
}

fn invoke(&self, arg: T) {
    // Fast loop — 无 virtual dispatch，等价 C# delegate
    for h in &self.strong { h(arg); }
    // Slow loop — 仅 advanced 订阅走这里
    for sub in &self.advanced {
        if let Some(h) = sub.try_get() {
            h(arg);
            sub.on_invoked();
        }
    }
}
```

`Subscribe(Action<T>)` 便利重载**不**在内部实例化 `StrongRef`，直接进 strong vec。

### 9.2 优化 B — Composite 融合（消除多次包装）

> `AsWeak().AsOnce()` **不应**分配两个对象。

```z42
impl<TDelegate> ISubscription<TDelegate> {
    public ISubscription<TDelegate> AsWeak() {
        // 已是 CompositeRef → 累加 flag，复用对象
        if (this is CompositeRef<TDelegate> c) return c.WithMode(Mode.Weak);
        // 否则首次包装
        return new CompositeRef<TDelegate>(/* unwrap */, Mode.Weak);
    }
}
```

效果：任意链式 `AsWeak().AsOnce().AsThrottled(...)` 最终**只分配 1 个 CompositeRef**。

### 9.3 优化 C — 跳过无状态 wrapper 的 OnInvoked

> StrongRef / WeakRef 是无状态的；`OnInvoked` 每次空转浪费。

实现两个路径之一：

**方案 C-1 — Mode flag short-circuit**：
```z42
public void OnInvoked() {
    if ((Modes & Mode.Once) != 0) _consumed = true;
    // 无 Once → 整个方法等价 nop；JIT 可内联消除
}
```

**方案 C-2 — Marker interface**：
```z42
public interface IStatefulSubscription<T> : ISubscription<T> { void OnInvoked(); }
// invoke loop 中 type check：
if (sub is IStatefulSubscription<T> stateful) stateful.OnInvoked();
```

C-1 简单；C-2 更利于 JIT 单一类型多播链去虚化。**实现时择一**，不强制。

### 9.4 优化后性能特征

| 订阅类型 | Subscribe 分配 | Invoke per handler | 与 C# 对比 |
|---------|--------------|------------------|--------|
| Strong（裸 `+= h`）| **0** | 1 直接调用 | **等价** |
| Weak | 1 CompositeRef | 1 weak upgrade + 1 调用 | C# 不支持原生 weak |
| Once | 1 CompositeRef | 1 调用 + 1 flag 检查 | C# 不支持原生 once |
| Weak+Once 任意组合 | **1**（融合） | 1 weak upgrade + 1 调用 + 1 flag | C# 不支持 |

### 9.5 性能逃生口（用户主动选择）

| 选项 | 用法 | 适用场景 |
|------|------|--------|
| 强制全 strong | 不暴露 ISubscription overload；只接 raw delegate | 极致性能事件（60fps render / 高频 input）|
| 直接用 `MulticastAction<T>` 字段 | 跳过 `event` 关键字 desugar | 编译期不需要访问控制保护 |
| 退化到单播 `event Action<T>` | 至多 1 handler；零多播开销 | 仅 1 listener 场景 |

---

## 10. 改进项映射（C# 缺陷 → z42 改良）

### 10.1 Delegate 侧

| ID | C# 缺陷 | z42 改良 | 落地阶段 |
|----|--------|---------|:------:|
| D1 | Action × 16 + Func × 17 × 16 stdlib 爆炸 | 脚本生成 `tools/gen-delegates.z42`；维护成本 0 | D1 |
| D2 | 多播返回值只保留最后一个 | `MulticastFunc.Invoke` 返回 `R[]` | D2 |
| D3 | 多播链中某 handler 抛异常 → 后续不调用 | `continueOnException=true` + `MulticastException` | D2 |
| D4 | `delegate ==` 比较 target+method；匿名 lambda 不可比 | `IDisposable` 订阅 token；不依赖 `==` 比对 | D2 |
| D5 | 变型必须显式 `in`/`out`；不能跨 delegate 类型互转 | 暂不支持变型；推迟到 L3 后期评估 | — |
| D6 | 方法组转换每次新分配 | call-site static cache（I12） | D1 |
| D7 | delegate 调用比直接方法慢 | JIT 内联优化（基础设施已就绪） | D1 + JIT 后续 |
| D8 | `BeginInvoke`/`EndInvoke` APM 历史遗留 | z42 不引入；统一走 L3 async | — |
| D9 | 委托对象大 | sealed class 设计紧凑；后续可对象池 | D2 |

### 10.2 Event 侧

| ID | C# 缺陷 | z42 改良 | 落地阶段 |
|----|--------|---------|:------:|
| E1 | lapsed-listener memory leak | `IDisposable` 订阅 token + `WeakRef` wrapper | D2 |
| E2 | `event?.Invoke(...)` 模板冗余 | 多播 event 空链 = no-op，杜绝 `?.` | D2 |
| E3 | invoke 异常不隔离 | `continueOnException` + `MulticastException` | D2 |
| E4 | 不能 `await` event | `await event` 桥接到 Task | L3 async |
| E5 | `EventHandler<T>` 强制 sender + args 冗余 | 推荐 plain `Action<T>` | D2 |
| E6 | interface event 必须显式 add/remove | 自动合成 | D2 |
| E7 | 无 once / weak / scoped 修饰符 | 全部走 `ISubscription` wrapper（`OnceRef` / `WeakRef` + IDisposable）| D2 |
| E8 | invoke 多线程不安全 | COW snapshot | D2 |

---

## 11. 实施阶段（D1 + D2）

### 11.1 依赖关系

```
Lambda（已在路上） → D1 → D2
                              ↓
                    await event（待 L3 async 落地）
                    Throttled / Debounced / OnUiThread wrapper（按需独立 spec）
```

### 11.2 Spec 范围

| Spec | 内容 | 工作量 |
|------|------|------|
| **D1** `add-delegate-type` | `delegate` 关键字 + 命名 / 泛型 delegate 一次到位 + 方法组转换（含缓存 I12）+ 单播 `Invoke` + 新 IR opcodes (`DelegateNew` / `DelegateInvoke`) + zbc DELG section + 脚本生成 N arity stdlib 类型 | 大（Lexer + Parser + TypeCheck + IrGen + VM + JIT）|
| **D2** `add-multicast-and-event` | 多播三件套 (`MulticastAction/Func/Predicate`) + `ISubscription` wrapper 体系（`StrongRef` / `WeakRef` / `OnceRef` / `CompositeRef`）+ `event` 关键字（统一单/多播 access control + `+=`/`-=` 糖）+ `MulticastException` / `MulticastException<R>` + `Invoke(continueOnException)` + 三个性能优化实现 + COW 多线程 + interface event 默认实现 | 大 |

**总规模**：2 个 lang spec + 1 个 stdlib codegen 脚本（`tools/gen-delegates.z42`）。
比原计划 4 spec 大幅简化。

---

## 12. 不做（明确不在本设计范围）

| 项 | 理由 |
|----|------|
| Variadic generics | 复杂度过高；C# / Rust 都没做；脚本生成等价用户体验 |
| 协变 / 逆变（`<in T, out R>`）| 推迟到 L3 后期 |
| `delegate.GetInvocationList()` | 多播是独立类型 + COW 公开化，不需此 API |
| `BeginInvoke` / `EndInvoke` APM | 用 L3 async/await 替代 |
| `Delegate` / `MulticastDelegate` 抽象基类 | 类型分离设计下无意义 |
| 反射式 `delegate.DynamicInvoke` | 与 L3-R 反射轨道一并设计 |
| **`[Weak]` / `[Once]` attribute** | 完全由 `ISubscription` wrapper 替代；声明端零 attribute |
| **`weak event` / `once event` 修饰符** | 同上；订阅端 wrapper 完全替代 |
| **per-event once with replay** | 不属于 event；用 `Promise<T>` / `TaskCompletionSource<T>` 表达（独立 spec）|

---

## 13. 开放问题（待 spec 实施时决断）

| # | 问题 | 临时倾向 |
|---|------|--------|
| 1 | 多 arity delegate 在 zbc 中如何编码 typeparam？是否扩展 SIGS section？| 复用 L3-G3a 的约束元数据通道 |
| 2 | `MulticastException.Failures` 是 `Dictionary<int, Exception>` 还是 `(int, Exception)[]`？| 倾向 Dictionary（稀疏 + 索引查询便利）|
| 3 | `MulticastFunc<T,R>.Invoke` 失败位置 `default(R)` 对 reference type R = `null`，对 value type 怎么处理（`default(int)` = 0）？| 接受这是用户责任；他们应查 `Failures` 判定真假 |
| 4 | event 字段的 `+=` / `-=` 是否允许跨线程？ | 是（COW + 原子 swap） |
| 5 | `+=` 操作符 desugar 规则在 z42 通用化（不只 event）的影响 | 评估是否所有 LHS 重载 `+=` 的类型都改为方法调用语义；可能影响 `List<T>` 等 |
| 6 | 性能优化 9.3 选 C-1 还是 C-2 | 实施期 benchmark 决策 |
| 7 | `ISubscription` 的 `OnInvoked` 在 `continueOnException=true` 模式下，wrapper 抛异常应聚合到 `MulticastException` 还是隔离？ | 倾向聚合（与 handler 异常同处理）|
| 8 | **静态方法 handler 的 weak 引用语义** —— 静态方法 delegate 无 `Target` object，弱引用无对象可弱持。当前方案：weak **退化为 strong**（method handle 永久持有）。**待 hot-reload 落地后**需重新考虑：类型 / 程序集被卸载时 method handle 失效；weak 是否要弱持类型元数据（`Type` 对象）？涉及 `[HotReload]` 与 `MagrGC` 类型表 GC 的协同。**留到 hot-reload spec 期一并设计**，本期不做。 | 暂退化 strong；备忘待定 |

---

## 14. 与其他文档的关系

- **`features.md`** §6 "Lambda & Closures (L3)" → 改为 "Lambda 是 Delegate 的前置；详见 `delegates-events.md`"
- **`generics.md`** → 泛型 delegate 走既有泛型机制；`delegate R Func<T,R>` 复用现有 type param 元数据
- **`concurrency.md`** → `await event` (E4 改进项) 在 L3 async 实施时桥接；UI 线程 dispatch wrapper 等可与 concurrency 协同设计
- **`stdlib.md`** Module Catalog → `z42.core` 加 Func/Action/Predicate + Multicast 三件套 + ISubscription/wrapper 类
- **`stdlib-roadmap.md`** → 不影响（这些是 z42.core 内置类型，不是新独立包）
- **`error-codes.md`** → D1 / D2 实施时新增 E0930+ 段
- **`object-protocol.md`** → 涉及 delegate 是否算 reference type / equality 协议时同步

---

## 15. 与 C# 视觉对照

```z42
// === C# 用户看 z42 代码 ===
public class Button {
    public event MulticastAction<MouseArgs> Clicked;            // 比 C# `event Action<T>` 多一个 Multicast 前缀
    public event Action<KeyArgs> OnKeyDown;                     // 单播事件 — C# 无对应（C# Action 是多播）
}

button.Clicked += args => Console.WriteLine(args.X);            // 与 C# 一字不差
button.Clicked -= someHandler;                                  // 与 C# 一字不差
button.Clicked += handler.AsWeak();                             // z42 新增：弱引用订阅
button.Clicked += handler.AsOnce();                             // z42 新增：一次性订阅

// 默认行为更安全
button.Clicked.Invoke(args, continueOnException: true);         // z42 新增；C# 没有

// 异常审计能力（C# 必须自己写 GetInvocationList）
try { ... } catch (MulticastException e) {
    foreach (var (i, ex) in e.Failures) { ... }
}
```

**迁移成本评估**：C# 项目搬到 z42，delegate / event 相关代码改动率预计 < 10%：
- 主要改动：`event Action<T>` → `event MulticastAction<T>`（语义诚实代价）
- 主要受益：`SubscribeWeak` 模式取代手动 `WeakReference + WeakEventManager`；`continueOnException` 取代手动 `GetInvocationList` 循环

---

## Deferred / Future Work

> 索引也存于 [docs/roadmap.md](../../roadmap.md) "Deferred Backlog Index"。

### D-2: ISubscription chain `.AsOnce()` / `.AsWeak()` 跨 generic interface impl

- **来源**：[docs/spec/archive/2026-05-03-add-isubscription-wrapper/](../../spec/archive/2026-05-03-add-isubscription-wrapper/) design Decision 3
- **触发原因**：z42 现有 `impl<T> ISubscription<T> { ... }` 跨 generic interface 的扩展方法机制未验证。D2b v1 仅支持 `impl<T> Action<T> { public ... AsOnce() }`（具体 delegate 类型上的扩展），ISubscription 实例的链式 chain 不可用。
- **前置依赖**：验证 / 修复 z42 generic interface 上的 impl 扩展；或实现 wrapper 类自身的 fluent API（`composite.WithMode(Once)` 已可用，只是不通过 `.AsOnce()` 链式）。
- **触发条件**：D2b 后用户希望 `someSubscription.AsOnce().AsWeak()` 这样链式融合。当前用 `new CompositeRef<T>(handler, Mode.Once | Mode.Weak)` 直接构造也能达到相同结果。
- **当前 workaround**：`new CompositeRef<T>(h, Once|Weak)` + `.WithMode(...)` 在 CompositeRef 实例上链式已覆盖功能需求（仅美感差）
- **2026-05-04 重新评估**：[`SymbolCollector.Impls.cs:19-23`](../../../src/compiler/z42.Semantics/TypeCheck/SymbolCollector.Impls.cs#L19-L23) 的 switch 仅接受 `NamedType`，**不接受** generic 实例化 target（`ISubscription<T>` / `Action<T>` 都不行）。要做需 Parser/AST/SymbolCollector 三层联动，2-4 hr 真实编译器工作。**Deprioritize**：等真有用户因美感受阻再启动。

### D-3: N>4 arity Action / Func

- **来源**：[docs/spec/archive/2026-05-02-add-generic-delegates/](../../spec/archive/2026-05-02-add-generic-delegates/)
- **设计文档**：本文件 §3.4
- **触发原因**：design 推荐 `tools/gen-delegates.z42` 脚本自动生成 Action/Func 0-16 arity，但 z42 未自举（编译器是 C#），跑 z42 脚本生成 z42 源码是循环依赖。
- **前置依赖**：z42 自举完成（编译器用 z42 实现），或独立 spec 用 C# 写代码生成。
- **触发条件**：用户实际需要 5+ arity callable（按 C# 经验罕见，<5% 场景）。
- **当前 workaround**：手写 0-4 arity（覆盖 95% 场景）。
- **2026-05-04 重新评估**：探索 examples/ + tests/ 确认 0 个 5+ arity 真实使用；compiler / runtime 无 per-arity 特殊路径，加 5-16 是纯机械重复。**结论：保持 deferred**。等真有用户场景；自举完成后用 z42 自身写生成器（`tools/gen-delegates.z42`）。当前不做 C# 一次性生成器，避免引入永久的"非 z42 写的 z42 stdlib 源"反向依赖。
