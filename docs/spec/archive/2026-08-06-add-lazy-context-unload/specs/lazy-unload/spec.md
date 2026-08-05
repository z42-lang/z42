# Spec: 惰性卸载（Lazy Context Unload）

## ADDED Requirements

### Requirement: Unload 标记 collectible 上下文为 Unloading

#### Scenario: Unload 一个 collectible 上下文不再抛异常
- **WHEN** 对 `AssemblyLoadContext.CreateCollectible("x")` 建的上下文调用 `ctx.Unload()`
- **THEN** 不抛异常（Phase 1 的 `NotSupportedException` 行为被取代）；`ctx` 进入 Unloading 状态

#### Scenario: Unload root 抛 InvalidOperationException
- **WHEN** 对 `AssemblyLoadContext.Default()`（root）调用 `Unload()`
- **THEN** 抛 `Std.InvalidOperationException`（root 永驻不可卸载）

#### Scenario: Unloading 上下文拒绝再 Load
- **WHEN** 对已 `Unload()` 的上下文调用 `Load(...)`
- **THEN** 抛异常（上下文正在卸载，不接受新 assembly）

#### Scenario: 重复 Unload 幂等
- **WHEN** 对已 Unloading 的上下文再次 `Unload()`
- **THEN** 不抛异常，无副作用（幂等）

### Requirement: 无引用的 Unloading 上下文被 GC 回收

#### Scenario: 无活实例的 collectible 上下文 Unload 后被回收
- **WHEN** 建 collectible 上下文 + `Load` 一个 zpkg（不保留任何该 zpkg 类型的活实例/反射对象）→ `Unload()` → 触发 GC（`Std.GC.ForceCollect()`）
- **THEN** 该上下文的 arena 被确定性 free；回收后其 assembly 不再出现在活动上下文中（可经回收计数/后续行为观测）

#### Scenario: 有活实例时不回收（Erlang 等自然死）
- **WHEN** 建 collectible 上下文 + `Load` + **持有一个该上下文类型的活实例（或其 `Std.Type` 反射对象）** → `Unload()` → GC
- **THEN** 该上下文**不被回收**（arena 保留）；直到该活实例/反射对象不可达后的某轮 GC 才回收

#### Scenario: 反射对象也是保留边
- **WHEN** 持有一个 collectible 上下文里类型的 `Std.Type`（`asm.GetTypes()[i]`）或其 `Assembly` 对象 → `Unload()` → GC
- **THEN** 只要该反射对象可达，上下文不被回收（`NativeData::{TypeHandle,AssemblyHandle,LoadContextHandle}` 计入保留边）

### Requirement: 正常无卸载时零回归

#### Scenario: 无 Unloading 上下文时 GC 行为不变
- **WHEN** 运行任意不涉及 `Unload()` 的现有 e2e / stdlib 用例
- **THEN** GC mark/sweep 行为与本 change 前一致（context-liveness 钩子仅在 `unloading_count > 0` 时激活）；全量测试逐字节/结果不变

## IR Mapping

无新 IR 指令、无 zbc/zpkg 格式 bump。`Unload()` 从 z42 `throw` body 改为 `[Native("__lctx_unload")]`
extern；能力全部经 native builtin + GC 内部逻辑落地。

## Pipeline Steps

- [ ] Lexer / Parser / TypeChecker / IR Codegen —— 不涉及
- [x] VM interp / GC —— context 状态机 + TypeDesc→ctx 反查表 + mark 保留边钩子 + reclaim pass（**核心**）
- [x] stdlib —— `AssemblyLoadContext.Unload()` body 改 extern
