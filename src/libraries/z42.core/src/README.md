# z42.core/src

## 职责

z42 隐式 prelude 的源码。VM 启动时无条件加载；用户项目**不可**显式声明依赖。

`sources.include` 走默认 `src/**/*.z42` 递归通配，子目录自动拾取。

## 目录与子目录职责

| 路径 | 内容 |
|------|------|
| `Object.z42` / `Type.z42` / `TypeVisibility.z42` | 引用类型基类（`ToString` / `Equals` / `GetHashCode`）；运行时类型对象（`typeof` 结果）；反射可见性枚举 |
| `Array.z42` | 所有 `T[]` 的基类（`sealed`）：`Length` / `Clone` / 反射式 `CreateInstance` / `GetValue` / `SetValue` + 静态算法（排序 / 查找 / 谓词 / 变换 / `AsReadOnly`），供反射式 serde |
| `String.z42` / `String.Split.z42` / `String.Edit.z42` | `string` 的 partial 三片：最小 intrinsic 核 + 纯脚本方法；`Split` / `Join` / `Concat` / `Format`；`Insert` / `Remove` / `Pad*` |
| `SplitOptions.z42` | `String.Split` 的 bitwise 选项常量 |
| `Primitives/` | 数值 / 布尔 / 字符 primitive（`Boolean` / `Char` / `Byte` / `SByte` / `Int16/32/64` / `UInt16/32/64` / `Single` / `Double`）的成员方法 |
| `Protocols/` | 接口契约：`IEquatable` / `IComparable` / `IDisposable` / `IFormattable` / `INumber` / `IEnumerable` / `IEnumerator` / `IComparer` / `IEqualityComparer` / `IBasicCollection` |
| `Collections/` | 基础泛型集合：`List<T>`（`List.z42` + `List.Query.z42`）/ `Dictionary<K,V>` / `HashSet<T>` / `ReadOnlyCollection<T>` / `KeyValuePair<K,V>` + 对应 Enumerator；`HashIndex.z42` 是 `Dictionary` / `HashSet` 共用的插入有序紧凑哈希表骨架（布局见其头注释） |
| `Exceptions/` | `Exception` 基类 + 标准子类（`ArgumentException` / `InvalidOperationException` / `AggregateException` / `MulticastException` 等） |
| `Delegates/` | callable + multicast + 订阅策略（详见 `docs/reference/src/language/delegates-events.md`）：`Delegates.z42` / `DelegateOps.z42`（Action / Func / Predicate + `==`）、`Multicast*.z42`、`ISubscription.z42` + `SubscriptionRefs.z42` |
| `Reflection/` | 反射成员对象：`MemberInfo` / `FieldInfo` / `MethodInfo` / `PropertyInfo` / `ConstructorInfo` / `MethodBase` / `ParameterInfo` / `Activator` / `Assembly`（详见 `docs/reference/src/stdlib/reflection.md`）；`Attribute.z42` / `ForwardAttribute.z42` / `Enum.z42` 在顶层 |
| `Runtime/` | `AppProperties` / `RuntimeConfig`（只读配置面）/ `AssemblyLoadContext` / `ModuleSearch`；顶层 `Runtime.z42` 为 `Std.Runtime` 动态加载入口（`LoadZpkg` / `CallStatic`） |
| `Time/` | `DateTime` / `DateTimeOffset` / `TimeSpan` / `Stopwatch` / `TimeZone` |
| `IO/` | 控制台 / 文件 / 目录 / 路径 / 环境 / 进程的 native 语义层（`Console` / `File` / `Directory` / `Path` / `Environment` / `FileStreamNative` / `ProcessNative`） |
| `Native/` | 网络（TCP / UDP / TLS / DNS）与线程 / Monitor 的 native 语义层，供 z42.net / z42.threading 包装 |
| `GC/` | GC 控制 + 句柄类型（见 [GC/README.md](GC/README.md)；机制 `docs/internals/src/runtime/gc-handle.md`） |
| `Convert.z42` / `Math.z42` | 类型转换；`Std.Math`（libm 原语 `__math_*` 唯一声明点 + 纯脚本派生 + 常量） |
| `BitConverter.z42` / `Clock.z42` / `Entropy.z42` | cross-cutting native 原语的**唯一声明点**：IEEE-754 位重解释；`WallMillis` / `MonoNanos` 时钟；OS 熵源 |
| `Platform.z42` / `OperatingSystem.z42` | OS / 架构标识（`Platform` / `OSKind` / `ArchKind`）；进程与机器信息 |
| `Assert.z42` / `Failure.z42` | 全仓唯一断言 API；`TestFailure` / `SkipSignal` |
| `Disposable.z42` | `IDisposable` 通用实现 + `Disposable.From(Action)` |
| `Guid.z42` / `Lazy.z42` / `Version.z42` / `ValueTuple.z42` | `Guid`（v4）/ `Lazy<T>` / `Version` / `ValueTuple<…>`（元组类型的运行时载体） |

## 设计原则

详见 [src/libraries/README.md](../../README.md)：
- **Script-First**：尽可能脚本实现；extern 仅限 syscall / libm / GC barrier / 类型元数据 / UTF-8 codepoint / 数值字面量 parse
- **interop 收缩两层模型**：interop 只在 core（全平台通用基础原语）+ 独立平台能力库（io/net/threading/compression 等）；其余库纯脚本零 interop。每 native 符号**单一声明点**（cross-cutting 原语归 core，如 `BitConverter` / `Clock`）。详见 [organization.md「平台边界库 vs 全平台共享库」](../../../../docs/internals/src/stdlib/organization.md)

## 跨目录依赖（包内 forward ref，无环约束）

| 子目录 | 依赖（同包内）|
|--------|------------|
| Object / Type / String | 无 |
| Primitives | Object（实现 ToString 等）+ Protocols（实现 IEquatable / IComparable / INumber）|
| Delegates | Object + Protocols (IDisposable for Subscribe token) + Exceptions (MulticastException) + GC (WeakHandle) |
| Protocols | Object（接口的"被实现者"）|
| Exceptions | Object + Collections (MulticastException.Failures) |
| Collections | Object + Protocols (IEnumerable / IEqualityComparer) + Array（`CopyRange` 批量拷贝）+ Exceptions（枚举期间修改字典抛 InvalidOperationException） |
| Convert / Assert | Object + Exceptions（抛 ArgumentException 等）|
| BitConverter / Clock | 无（纯 VM extern 门面，无同包内依赖）|
| Math | 无（libm extern + 纯脚本派生，仅用 double primitive 算子）|
| GC | Object（GCHandle 接受 object target；HeapStats 是 class）|
| Disposable | Object + Protocols (IDisposable) + Delegates (Action) |

> 同包内 forward ref 由编译器处理，**不构成实际循环**。"层级"仅作组织约定。
> 跨包 DAG 严格性见 [docs/internals/src/stdlib/organization.md](../../../../docs/internals/src/stdlib/organization.md)。
