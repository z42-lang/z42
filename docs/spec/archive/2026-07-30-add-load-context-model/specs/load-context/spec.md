# Spec: AssemblyLoadContext 模型（Phase 1 地基）

## ADDED Requirements

### Requirement: Root 上下文默认存在且不可回收

#### Scenario: 默认上下文即 root
- **WHEN** z42 程序调用 `Std.Runtime.AssemblyLoadContext.Default()`（静态方法——z42 stdlib 无静态属性先例，静态成员一律 extern 方法，实例 getter 才用 extern 属性）
- **THEN** 返回非 null 的 root 上下文，其 `Name == "root"`，`IsCollectible == false`

#### Scenario: 现有代码的类型不可回收
- **WHEN** 对 core/stdlib/主程序中的任意类型求 `typeof(T).IsCollectible`
- **THEN** 结果为 `false`（这些类型都在 root）

#### Scenario: 无句柄类型（primitive / array）不可回收
- **WHEN** 求 `typeof(int).IsCollectible` 或 `typeof(int[]).IsCollectible`
- **THEN** 结果为 `false`（无句柄类型归属 root，恒 false，不抛异常）

### Requirement: 可创建 collectible 上下文并把 zpkg 载入其中

#### Scenario: 创建 collectible 上下文
- **WHEN** 调用 `AssemblyLoadContext.CreateCollectible("plugin-a")`（静态方法）
- **THEN** 返回一个新上下文，其 `Name == "plugin-a"`，`IsCollectible == true`，`ContextId` 与 root 不同

#### Scenario: 载入 zpkg 到 collectible 上下文
- **WHEN** 对该上下文调用 `ctx.Load("<path>/dep.zpkg")`
- **THEN** 返回一个 `Std.Reflection.Assembly`，`asm.IsCollectible == true`，`asm.AssemblyLoadContext` 即该 `ctx`，且 `asm` 出现在 `ctx.GetAssemblies()` 中

#### Scenario: collectible 上下文里的类型标记为可回收
- **WHEN** 对上一步载入的 assembly 调用 `asm.GetTypes()`，取其中任一用户类型 `t`
- **THEN** `t.IsCollectible == true`，`t.Assembly` 即 `asm`，`t.FullName` 正确解析

#### Scenario: root 加载路径行为不变（兼容）
- **WHEN** 正常运行任意现有 e2e / stdlib 用例（不经 collectible API）
- **THEN** 加载、dispatch、输出与本 change 前**逐字节一致**（root 仍走 `merge_modules` 扁平路径，MethodId dispatch 不变）

### Requirement: Assembly 反射投影保留 zpkg 运行时身份

#### Scenario: Assembly 基本属性
- **WHEN** 访问一个 collectible 上下文载入的 `Assembly` 的 `Name`
- **THEN** 返回该 zpkg 的逻辑名（非空）

#### Scenario: Type ↔ Assembly ↔ AssemblyLoadContext 三者链路自洽
- **WHEN** 从 `ctx.Load(...)` 得到 `asm`，`asm.GetTypes()[i]` 得到 `t`
- **THEN** `t.Assembly == asm` 且 `asm.AssemblyLoadContext == ctx` 且 `t.IsCollectible == asm.IsCollectible == ctx.IsCollectible`（三者一致）

### Requirement: Unload() Phase 1 声明但不生效

#### Scenario: 调用 Unload 抛 NotSupportedException
- **WHEN** 对任意 collectible 上下文调用 `ctx.Unload()`
- **THEN** 抛出 `Std.NotSupportedException`（或既有等价异常类型），消息明确指向"回收机制将在后续 change 落地"，且**不**破坏上下文状态（后续 `IsCollectible` / `GetAssemblies` 仍可正常访问）

#### Scenario: 对 Default（root）调用 Unload
- **WHEN** 对 `AssemblyLoadContext.Default` 调用 `Unload()`
- **THEN** 抛异常（root 永不可卸载；Phase 1 统一 `NotSupportedException`，语义上 root 更是 `InvalidOperation`——实现取其一并在 message 说明）

## IR Mapping

无新 IR 指令、无 zbc/zpkg 格式 bump。`typeof` 已存在、返回 `Std.Type`（不变）。新增能力全部经
**native builtin**（`__lctx_*` / `__asm_*` / `__type_is_collectible` / `__type_assembly`）+ stdlib
类落地，不触及 IR / 二进制格式。

## Pipeline Steps

受影响的 pipeline 阶段：

- [ ] Lexer —— 不涉及
- [ ] Parser / AST —— 不涉及
- [ ] TypeChecker —— 不涉及（新类是普通 stdlib 类 + extern 方法）
- [ ] IR Codegen —— 不涉及
- [x] VM interp —— 新 builtins + AssemblyLoadContext 运行时模型 + 加载路径分叉（**核心**）
- [x] stdlib —— 新 z42 类 `AssemblyLoadContext` / `Assembly` + `Type` 加成员
