# 委托与事件的实现

> 对齐：2026-09-17。用户视角的语法与 API 见参考手册的「委托与事件」；本页只回答
> 「为什么这样设计、编译期和运行期各做了什么、在哪改」。

涉及三层：编译器（`delegate`/`event` 的解析与脱糖、方法组转换）、运行时（closure 值表示 +
四个 `__delegate_*` builtin）、stdlib（三个 Multicast 类的双通道存储）。

## 1. 选型决策（为什么是现在这个形状）

### 1.1 单播 vs 多播路径

| 路径 | 描述 | 主要语言 | z42 决策 |
|------|------|---------|---------|
| A 统一类型 | `Action<T>` 一个类型既可 1 个也可 N 个 handler | C# / F# | ❌ 不采用 |
| B 分离类型 | 单播 `Func<T,R>` + 独立多播 `MulticastX<T>` | Rust / Kotlin / Swift / Java / TS / Go / Python | ⭐ **采用** |
| C 反应式流 | 多播完全交给 Flow / Subject | Kotlin Flow / Combine | ❌ 工程量过大 |

### 1.2 订阅策略的表达方式

| 路径 | 描述 | 决策 |
|------|------|------|
| 关键字修饰 | `weak event` / `once event` | ❌ 关键字堆叠 |
| Attribute 标注 | `[Weak] event` / `[Once] event` | ❌ 声明端责任倒置（是**订阅方**要弱引用，不是声明方）|
| 包装类 | `SubscribeAdvanced(new WeakRef<..>(h))` | ⭐ **采用** |

### 1.3 关键决策

| ID | 决策 | 原因 |
|----|------|------|
| K1 | 单播 = 编译期 `delegate` 类型；多播 = `sealed class` | 两类语义不同，类型层面就分开 |
| K2 | 多播命名 = `Multicast` + 单播名 | 严格对称单播 |
| K3 | `event` 关键字统一单播 + 多播 | 类型决定 cardinality；`+=`/`-=` 按类型 dispatch |
| K4 | `event Action<T>` = 单播；`event MulticastAction<T>` = 多播 | 类型 100% 诚实、无歧义 |
| K5 | `Invoke(arg, bool continueOnException = false)` | 默认 fail-fast = C# 语义 |
| K6 | `continueOnException=false` 时**不包装**异常 | 单 handler 场景与 C# 100% 一致 |
| K7 | `continueOnException=true` 时抛 `MulticastException` | opt-in 包装 |
| K8 | `MulticastException<TResult>` 携带 `Results`，失败位 = `default(TResult)` | 用户查 `FailureIndices` 判定真成功 |
| K9 | 不做 `TryInvoke` / `InvokeAggregate` | `MulticastException` 已承载诊断 |
| K10 | `MulticastPredicate` 提供短路的 `All` / `Any` | LINQ 习惯；性能优势 |
| K11 | 订阅策略全部走 `ISubscription` 包装类 | 责任归属订阅者；零 attribute；无限可扩展 |
| K12 | 不做 variadic generics | 复杂度过高；C# / Rust 都没做 |
| K13 | `event` 关键字 = 多播类型字段的语法糖 | C# 视觉一致；底层无新机制 |
| K14 | 三个性能优化是实现强约束 | strong 路径必须达 C# 等价；见 §4 |

**K8 的实施偏差**：写规范时 `Failures` 提案为 `Dictionary<int, Exception>`；实际落地为
`Exception[] Failures` + `int[] FailureIndices` **平行数组**，为的是避开 stdlib 里
`z42.core` 对 Dictionary 的依赖。用户可见 API 因此是 `.Length` + 双下标，不是 `.Count` +
`foreach (var (i, ex) in ...)`。

## 2. 编译期

### 2.1 没有专属 IR 指令

设计期曾规划 `DelegateNew` / `DelegateInvoke` 两条 IR opcode 和一个 zbc `DELG` section
——**都没有落地，也不该再去做**。实际走的是通用闭包路径：

| 用途 | 实际指令 |
|---|---|
| lambda / 实例方法组 → delegate 值 | `MkClos`（`OP_MK_CLOS = 0x57`）|
| 静态方法组 → delegate 值 | `LoadFn` / `LoadFnCached`（`0x55` / `0x58`）|
| delegate 调用 | `CallIndirect`（`0x56`）|

delegate 类型自身在 IR 里由 `StubEmitter._emitDelegateInvoke`
（`src/compiler/z42c.semantics/src/StubEmitter.z42`）合成一个 `Invoke` 桩函数，
由 `IrGenAuxEmitter` 挂进模块；跨 zpkg 导出走通用类型元数据通道。

### 2.2 方法组转换

**静态方法组** `Action<int> a = SomeStatic;` → `LoadFnCached`，函数引用缓存到**模块级 slot**
（slot 表在 `boot.rs` 的 `alloc_func_ref_slots` 分配）。反复进入同一作用域不重复分配，消除
C# 高频 callback 路径的 GC 压力。

**实例方法组** `obj.Method`（D-1b）→ 编译期合成一个 static thunk：

```
__mg_thunk_<safeClass>_<Method>$<arity>__
```

emit `MkClos(thunk, [recv])`；thunk 体内对 `env[0]` 做 vcall。合成点在
`CallEmitter.z42`（`_ctx.Qualify("__mg_thunk_" + ...)`）。
**这条路径没有 static slot 缓存**——每次求值分配一个 Closure。§2.3 的弱引用协议依赖
"receiver 一定落在 `env[0]`" 这个约定。

### 2.3 弱引用需要的三个 builtin

`WeakRef<TD>` 不能强持原 handler：Closure 自己强持 `env = [receiver]`，持住 handler 就等于
持住 receiver，弱引用直接失效。所以构造时把 handler **拆解**成 `(WeakHandle → receiver, fnName)`，
`Get()` 时重建一个新 Closure。这需要 corelib 暴露三个 builtin（注册表在
`src/runtime/src/corelib/builtin_table.rs`）：

| builtin | stdlib 入口（`Std.DelegateOps`）| 语义 |
|---|---|---|
| `__delegate_target` | `GetTarget(object) -> object` | 从 Closure 取 `env[0]`；`StackClosure` / `FuncRef` / 非 delegate 返回 null |
| `__delegate_fn_name` | `GetFnName(object) -> string` | 取 Closure 的 `fn_name` |
| `__make_closure` | `MakeClosure(string, object[]) -> object` | 用 `(fn_name, env)` 重建 Closure |
| `__delegate_eq` | `ReferenceEquals(a, b) -> bool` | 按 `FuncRef` / `Closure` / `StackClosure` 三变体各自的身份语义比较；跨变体不等；非 delegate 返回 false 不报错 |

`GetTarget` 返回 null 就是"无 receiver 可弱化" → 包装类退化为 strong。

### 2.4 `event` 脱糖在哪

- **解析 + 访问器合成**：`src/libraries/z42c.syntax/src/MemberParser.z42`
  （`_synthEventAccessor` 多播 / `_synthSinglecastAccessor` 单播 / `_isMulticastEventType` 判别）。
  多播 event 字段无初始化器时在这里补 `new MulticastXxx<...>()`。
- **`+=` / `-=` 改写**：`src/compiler/z42c.semantics/src/AssignTyper.z42` 的 `+=`/`-=` 分支，
  在接收者类型上查 `add_X` / `remove_X`（class / instantiated / interface 三种接收者类型都查），
  查到就整体绑成一次 `BoundCall`。**这是脱糖的唯一判据——不看"这个字段是不是 event"，
  而看"有没有同名访问器"。**
- 类型侧的 `Z42ClassType.EventFields`（`Z42Type.z42`）由 `MemberCollector.z42` 收集。

### 2.5 `E0414` 定义了但从不发射

`DiagnosticCodes.EventFieldExternalAccess = "E0414"`（`z42c.core/src/DiagnosticCodes.z42`）
声明了"event 字段外部直接读 / 直接 `Invoke` / 直接赋值"的诊断，`EventFields` 也确实在
`MemberCollector` 里被收集了——**但 TypeChecker 从不查询这张表，E0414 全仓零发射点。**

后果：外部代码今天可以 `button.Clicked.Invoke(...)`、`button.OnKeyDown = h`，编译通过。
参考手册已按"未实现"叙述。要补的话，挂载点是 `BindMemberExpr`（`+=`/`-=` 已在
`AssignTyper` 阶段被吃掉，不会误伤）。

### 2.6 嵌套 delegate

嵌套 `delegate` 声明曾经**根本没被解析**——自举迁移时 `MemberParser._parseMemberBody` 没有
`delegate` 分支，`delegate` 被当成一个类型名，整条声明报废、该 delegate 类型整体消失。
D-6 的 golden（`src/tests/delegates/nested_delegate_dotted.z42`）因此带着 12+ 条编译错误
"通过"了四个月——`--emit-zbc` 吞诊断所致。

现行实现走 `NestedFlatten` 的 `Outer+Inner` 展平（与嵌套 class/enum 同机制），引用侧写
`Outer.Inner`（`SymbolTable.ResolveTypeP` 的 dots→`+` 兜底）。
**代价：类内部裸名 `OnClick handler;` 不解析**，类内外一律得写限定名。
展平与"拿名字当键"的纪律见
[源代码编译流程](../compiler/source-compile.md)的「名字与『拿名字当键』的三条纪律」一节。

## 3. stdlib 的多播存储

三个 Multicast 类（`src/libraries/z42.core/src/Delegates/`）用**双数组通道**：

```
strong[]   + strongAlive[]     ← 裸 handler，invoke fast loop，无 wrapper 检查
advanced[] + advancedAlive[]   ← ISubscription 包装，slow loop 走 IsAlive/Get/OnInvoked
```

`Subscribe(handler)` 直接进 strong 数组，**不**在内部实例化 `StrongRef` —— 这是"裸订阅零分配"
的全部实现。`SubscribeAdvanced(wrapper)` 进 advanced 数组。

退订走两条路：`Subscribe*` 返回的 `MulticastSubscription` token（`Dispose()` 置
`alive[idx] = false`），或 `Unsubscribe(handler)`（只扫 strong 通道，`DelegateOps.ReferenceEquals`
比较，**命中不 break**——同一 handler 订阅多次，一次 `Unsubscribe` 全清，对齐用户对
`x -= h` 的预期"h 不再被通知"）。

`Invoke` 先把两个数组各拷一份快照再遍历（COW）；`continueOnException=true` 时用一个
初始容量 8、翻倍扩容的失败缓冲累积 `(exception, index)`，跑完后 trim 成精确长度再抛。

**索引口径**：`FailureIndices` 是"实际调用序"的 0-based 下标（strong 先、advanced 后），
**不是** `strong[]` / `advanced[]` 的物理下标——中间被跳过的死槽不计数。

**设计文档里那段 Rust 伪码是错的**：早期规范把 `MulticastStorage` 写成 VM 内部的 Rust
结构体。实际实现全在 z42 stdlib，VM 侧对多播一无所知。

## 4. 三个性能约束

朴素实现（每个订阅都包一层 `ISubscription`）会让 invoke per-handler 比单播 delegate 慢
~3–4×，且链式包装有 GC 压力。三条约束把默认路径拉回等价：

**A — 双通道**（已实现，§3）：99% 订阅是 strong，为它走零 wrapper 的 fast loop。

**B — Composite 融合**（部分实现）：`CompositeRef.WithMode(flag)` 在**同一实例**上累加
mode 位，任意多个策略最终只分配 1 个 `CompositeRef`。
⚠️ **构造后追加 `Weak` 不会重新拆解 handler**（避免反向耦合构造序），所以 weak 必须在
构造器里一次给全。链式 `.AsWeak().AsOnce()` 的门面**没有实现**（见 §6）。

**C — 跳过无状态 wrapper 的 `OnInvoked`**：采用方案 C-1（mode flag 短路）——
`OnInvoked` 里 `if ((Modes & Once) != 0) consumed = true;`，无 `Once` 时整个方法等价 nop，
JIT 可内联消除。备选的 C-2（marker interface `IStatefulSubscription`）未采用。

## 5. `MulticastException` 的实现记录

- 泛型 `MulticastException<TResult>` 与非泛型 `MulticastException` 共存，靠
  class arity overloading 的 shadow-only mangling：IR 侧名是 `MulticastException$1`，
  源码侧仍写 `MulticastException`。
- 构造器用 `: base(failures, indices, totalHandlers)` 委托，父类设三个字段，子类只设
  `Results`。
- 对外是 **plain field**（不是属性），`SuccessCount()` 是方法而非属性——与 stdlib 当时
  的整体风格一致。
- `catch` 子句 parser 已接受泛型类型（`catch (MulticastException<int> e)`），mangle 到
  `Name$N` 与类注册键对齐。
- **JIT 缺口**：`jit_default_of` helper 镜像了 interp 的 `default(T)`，但 `jit_obj_new`
  仍不 propagate `type_args`，所以 JIT 分配的实例里 `default(T)` 仍退化为 Null。
  `multicast_func_aggregate` / `multicast_predicate_aggregate` 两个 golden 因此带
  `interp_only` 标记。

> `src/libraries/z42.core/src/Exceptions/MulticastException.z42` 顶部注释里"当前
> MulticastFunc/Predicate 仍抛非泛型 base 版本"这句**已过期**——两者现在都抛泛型版
> （`MulticastFunc.z42:224` / `MulticastPredicate.z42:211`）。

## 6. 缺口与 Deferred

### `.AsWeak()` / `.AsOnce()` 链式门面

设计规范里的

```z42
impl<TDelegate> TDelegate where TDelegate : Delegate { ... AsWeak() ... }
impl<TDelegate> ISubscription<TDelegate> { ... AsWeak() ... }
```

**全仓零实现**。根因：SymbolCollector 收集 `impl` 目标时只接受 `NamedType`，**不接受**
泛型实例化 target（`ISubscription<T>` / `Action<T>` 都不行）。要做需 Parser / AST /
SymbolCollector 三层联动。
**当前 workaround**：`new CompositeRef<T>(h, ModeFlags.Once | ModeFlags.Weak)` 直接构造，
功能等价，只是不够 fluent。**Deprioritize**——等真有用户因美感受阻再启动。

### N>4 arity 的 `Action` / `Func`

规范里"用 `tools/gen-delegates.z42` 脚本生成 0–16 arity"的方案**从未实施，该脚本不存在**。
原始阻塞（编译器是 C#，跑 z42 脚本生成 z42 源码是循环依赖）已随自举消失，但复核结论是
**保持 deferred**：examples / tests 里 5+ arity 真实使用 0 个，编译器 / 运行时也无
per-arity 特殊路径，加 5–16 是纯机械重复。

### 静态方法 handler 的弱引用语义

静态方法 delegate 无 target object，无对象可弱持 → **退化为 strong**（method handle 永久持有）。
热重载落地后需重新考虑：类型 / 程序集被卸载时 method handle 失效，weak 是否要弱持类型元数据。
留到热重载 spec 期一并设计。

### 其他未决

- `+=` 脱糖规则若通用化到非 event 场景（所有重载 `+=` 的类型都改成方法调用语义），会影响
  `List<T>` 等，未评估。
- `continueOnException=true` 时**包装类自身**抛异常（不是 handler 抛）应聚合还是隔离，未定；
  当前实现是聚合（与 handler 异常同处理）。

## 关联文档

- [对象与值表示 ABI](object-abi.md) —— `Value::Closure` / `FuncRef` / `StackClosure` 的表示
- [源代码编译流程](../compiler/source-compile.md) —— 嵌套类型展平与命名键纪律
- [IR 格式](../formats/ir.md) —— `MkClos` / `LoadFnCached` / `CallIndirect` 指令
