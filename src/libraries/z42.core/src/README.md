# z42.core/src

## 职责

z42 隐式 prelude 的源码。VM 启动时无条件加载；用户项目**不可**显式声明依赖。

`sources.include` 走默认 `src/**/*.z42` 递归通配，子目录自动拾取。

## 目录与子目录职责

| 路径 | 内容 |
|------|------|
| `Object.z42` | 所有引用类型的基类；`ToString` / `Equals` / `GetHashCode` 协议方法 |
| `Type.z42` | 运行时类型对象（`typeof` 结果） |
| `String.z42` | `string` primitive 的成员方法（`Substring` / `Contains` / 等）|
| `Primitives/` | 6 个数值/布尔 primitive 的成员方法（Bool / Char / Int / Long / Float / Double） |
| `Delegates/` | callable + multicast + 订阅策略整套（详见 `docs/design/language/delegates-events.md`）<br>• `Delegates.z42` / `DelegateOps.z42` — base Action/Func/Predicate + `==` / `!=`<br>• `MulticastAction/Func/Predicate.z42` — 多播容器<br>• `ISubscription.z42` + `SubscriptionRefs.z42` — 订阅策略 wrapper |
| `Protocols/` | 接口契约集中：IEquatable / IComparable / IDisposable / IFormattable / INumber / IEnumerable / IEnumerator / IComparer / IEqualityComparer |
| `Exceptions/` | `Exception` 基类 + 11 个标准子类（`AggregateException` / `MulticastException` / `ArgumentException` 等）|
| `Collections/` | 基础泛型集合：`List<T>` / `Dictionary<K,V>` / `KeyValuePair<K,V>` |
| `Convert.z42` | `Convert.ToInt32` / `ToDouble` / `ToString` 等转换辅助 |
| `BitConverter.z42` | `Std.BitConverter`：IEEE-754 位重解释 `SingleToBits`/`SingleFromBits`/`DoubleToBits`/`DoubleFromBits`（`__*_to_bits`/`__*_from_bits` 唯一声明点；z42.io.binary / z42.ir 调它——consolidate-core-intrinsics A1）|
| `Assert.z42` | `Assert.Equal` / `True` / `Null` 等运行时断言 |
| `GC/` | GC 控制 + 句柄类型（详见 `docs/design/runtime/gc-handle.md`）<br>• `GC.z42` — `Std.GC.*` 静态类（Collect / UsedBytes / ForceCollect / GetStats）<br>• `GCHandle.z42` — `Std.GCHandle` struct + `GCHandleType` enum（C# 风格 weak/strong + 显式 Free，corelib HandleTable backing）<br>• `HeapStats.z42` — `Std.GC.GetStats()` 返回类型（7 long 字段）<br>• `WeakHandle.z42` — 轻量 weak ref primitive（`Delegates/SubscriptionRefs.z42` 内部用）|
| `Disposable.z42` | `IDisposable` 的通用实现 + `Disposable.From(Action)` 工厂；用于单播 event token、`SubscribeScoped` 返回值等 |
| `Runtime.z42` | `Std.Runtime` 动态加载 + 静态调用：`LoadZpkg(path)` / `CallStatic(fqn, args)->int`（extern，VM builtins `__load_zpkg` / `__call_static`；实现待反射 + 自举完成后接入） |
| `Clock.z42` | `Std.Runtime.Clock`：`WallMillis()`（`__time_now_ms`）/ `MonoNanos()`（`__time_now_mono_ns`）时钟原语唯一声明点；z42.time / z42.io / z42.net / z42.test 调它——consolidate-core-intrinsics A1 |

## 设计原则

详见 [src/libraries/README.md](../../README.md)：
- **Script-First**：尽可能脚本实现；extern 仅限 syscall / libm / GC barrier / 类型元数据 / UTF-8 codepoint / 数值字面量 parse
- **interop 收缩两层模型**：interop 只在 core（全平台通用基础原语）+ 独立平台能力库（io/net/threading/compression 等）；其余库纯脚本零 interop。每 native 符号**单一声明点**（cross-cutting 原语归 core，如 `BitConverter` / `Clock`）。详见 [organization.md「平台边界库 vs 全平台共享库」](../../../../docs/design/stdlib/organization.md)

## 跨目录依赖（包内 forward ref，无环约束）

| 子目录 | 依赖（同包内）|
|--------|------------|
| Object / Type / String | 无 |
| Primitives | Object（实现 ToString 等）+ Protocols（实现 IEquatable / IComparable / INumber）|
| Delegates | Object + Protocols (IDisposable for Subscribe token) + Exceptions (MulticastException) + GC (WeakHandle) |
| Protocols | Object（接口的"被实现者"）|
| Exceptions | Object + Collections (MulticastException.Failures) |
| Collections | Object + Protocols (IEnumerable / IEqualityComparer) |
| Convert / Assert | Object + Exceptions（抛 ArgumentException 等）|
| BitConverter / Clock | 无（纯 VM extern 门面，无同包内依赖）|
| GC | Object（GCHandle 接受 object target；HeapStats 是 class）|
| Disposable | Object + Protocols (IDisposable) + Delegates (Action) |

> 同包内 forward ref 由编译器处理，**不构成实际循环**。"层级"仅作组织约定。
> 跨包 DAG 严格性见 [docs/design/stdlib/organization.md](../../../docs/design/stdlib/organization.md)。
