# 加载上下文（AssemblyLoadContext / ALC 地基）

> 对齐：2026-08-05（Phase 1 `add-load-context-model` 地基 + Phase 2 `add-lazy-context-unload` 惰性卸载）。
> 目标架构全景（含**强制清理** / whyRetained 诊断，均未落地）见
> [`docs/design/runtime/load-context.md`](../../../design/runtime/load-context.md)、
> [`docs/design/runtime/tiered-execution.md`](../../../design/runtime/tiered-execution.md)。
> 本页写**已落地**：Phase 1 代码边界 + zpkg 运行时身份；Phase 2 惰性卸载（`Unload()` + GC 回收）。

## 为什么需要

z42 运行时以往**没有任何代码边界**：`metadata::merge::merge_modules` 把所有 zpkg 塌成
**一个扁平 `Module`**，zpkg 身份在运行时被销毁。没有边界 = 没有可卸载的单元、没有可问
"被谁引用"的对象、没有可界定"重载范围"的粒度。加载上下文（dotnet `AssemblyLoadContext`
对标）引入这个边界。**Phase 1 只建边界 + 反射身份**——卸载、热重载是后续 change。

## 模型：root + collectible

```
              ContextRegistry（VmCore.context_registry）
              ┌──────────────────┬───────────────────────────┐
              ▼                   ▼                           ▼
      root context(0)     collectible "A"(1)          collectible "B"(2)
      IsCollectible=F     IsCollectible=T             IsCollectible=T
      现有扁平 Module      独立 arena（owned Module）    独立 arena
      O(1) MethodId       载入的 zpkg → 反射可见         ...
      dispatch（不动）
```

- **root**（`ContextId(0)`，永驻不可回收）：core / stdlib / 主程序所在。**保持现有扁平
  merge + `MethodId` 位置索引 dispatch，一字不改**——99% 代码在 root，热路径零回归。
- **collectible**（`CreateCollectible` 按需创建）：各持一个独立 arena（Phase 1 = 一个
  context-owned `Module`）承载载入的 zpkg，反射可见。

**"粒度比 dotnet 更小"的落点**：dotnet 加载单元固定在 assembly；z42 的 context 单元大小
**由用户决定**（root 一大块 + 任意多个可细到单 zpkg 的 collectible），只在用户显式画的边界上
付跨 context 间接成本。

## 对外 API（z42）

| 类型 | 成员 | 说明 |
|------|------|------|
| `Std.Runtime.AssemblyLoadContext` | `Default()`（静态） | 永驻 root 上下文 |
| | `CreateCollectible(name)`（静态） | 建可回收上下文 |
| | `Name` / `IsCollectible`（实例属性） | 名字 / 是否可回收 |
| | `Load(zpkgPath) -> Assembly`（实例） | 载入 zpkg（Phase 1 反射可见，暂不可跨 context 调用） |
| | `GetAssemblies()`（实例） | 已载入的 assembly |
| | `Unload()`（实例） | 标记 Unloading，GC 惰性回收（Phase 2；root 抛 `InvalidOperationException`）——见下「卸载 / 回收」 |
| `Std.Reflection.Assembly` | `Name` / `IsCollectible` / `AssemblyLoadContext` / `GetTypes()` | zpkg 的运行时反射投影 |
| `Std.Type` | `IsCollectible` / `Assembly`（新增） | 镜像 .NET `Type.IsCollectible` / `Type.Assembly`；root 类型恒 `false` |

> **语言约束**：z42 stdlib 无静态属性先例，静态成员（`Default` / `CreateCollectible`）用
> extern **方法**，实例 getter 用 extern **属性**——与 `Std.GC` 一致。故 `Default()` 是方法调用。

## 机制 / 实现

### ContextRegistry（`metadata/context.rs`）
`VmCore.context_registry: Mutex<ContextRegistry>` 持两张表：`contexts`（按 `ContextId` 索引：
name / is_collectible / assemblies）+ `assemblies`（按 `AssemblyId` 索引：name / context /
owned `Module`）。root context(0) + root assembly(0) 由 `ContextRegistry::new()` 预置。
`AssemblyLoadContext` / `Assembly` z42 对象经 `NativeData::LoadContextHandle` / `AssemblyHandle`
携句柄（仿 `Std.Type` 的 `TypeHandle`），不可用户构造。

### 关键决策：关联放注册表 + Type 对象 `__asmId` 槽，不 mutate TypeDesc
`Type.IsCollectible` / `Type.Assembly` 的解析**不改 `TypeDesc`**（它只 derive `Debug`、非
`Clone`，且加载后被 `Arc` 别名进两处注册表，mutate 代价大且要改 loader build 路径）。改为：

- `Assembly.GetTypes()` 用 `make_type_object` 建 `Std.Type` 后，往其 `__asmId` 隐藏槽
  stamp 该类型所属 `AssemblyId`；`typeof(T)` / `obj.GetType()` **不 stamp**（→ Null → root）。
- `__type_is_collectible` / `__type_assembly` 读 `__asmId` → 注册表查 context 可回收性 /
  建 `Assembly`。Null/0 → root → `false`。

观测行为与"context 挂在 TypeDesc 上"完全一致，且保住 `TypeDesc` 无锁读的写一次契约。这为将来
`type identity = (context, type)`（版本共存）留出可达路径（Type → asmId → context）。

### 加载路径分叉（`metadata/loader.rs`）
root 走现有 `merge_modules`（不动）；collectible 走 `AssemblyLoadContext.Load` →
`loader::load_artifact` 解析 zpkg → 存入该 context 的 `AssemblyEntry.module`，**不 merge 进
root**。`GetTypes()` 按 FQ 名有序返回（`assembly_types` sort，避免 HashMap 非确定序）。

## 卸载 / 回收（Phase 2 · add-lazy-context-unload）

`AssemblyLoadContext.Unload()` 标记 collectible context 为 **Unloading**，由 GC 惰性回收其
arena。Erlang current/old 语义——**有活实例/反射引用就不回收，等它们自然死**（无 tombstone，
强制清理是后续）。`Unload()` 是 `void`（fire-and-forget，不保证立即回收）；root 卸载抛
`InvalidOperationException`；对 Unloading context 再 `Load` 抛异常。

### 状态机

```
Active ──Unload()──▶ Unloading ──(GC 判定无引用)──▶ Reclaimed（arena freed）
                        └─ 有活实例/反射引用 → 保持 Unloading，下轮 GC 再判
```

### 机制（GC 驱动，major STW）

1. **反查表**（`ContextRegistry.td_to_ctx: HashMap<*const TypeDesc, ContextId>`）：`load_into`
   时登记 collectible 类型的 `Arc::as_ptr`（root 类型不登记）。不 mutate `TypeDesc`（Phase 1 D5）。
2. **保留边扫描**（`unloading_count > 0` 才激活，零回归）：major GC mark 后、sweep 前，`ArcMagrGC::scan_marked_contexts`
   遍历 marked `ScriptObject`，解析其**两类保留边** → 累积 `live_contexts`：
   - **活实例**：`obj.type_desc` ptr → 反查表；
   - **反射对象**：`obj.native` 为 `TypeHandle`/`AssemblyHandle`/`LoadContextHandle` 指向 collectible
     （`typeof(collectibleType)` 的 `Std.Type` 其 `type_desc` 在 root，但 native 句柄钉住 collectible——
     只看 `type_desc` 会漏，提前 free 仍被引用的 arena；这条是正确性关键）。
3. **reclaim**（sweep 后，STW 内）：`Unloading` 且不在 `live_contexts` 的 context → `drop` 其
   `AssemblyEntry.module`（`Arc<TypeDesc>`/functions/串池 refcount 归零 → **确定性 free**）+ 清反查表 +
   标 `Reclaimed`。回收只在**精确 gate**（无引用）时发生，故 drop 即能全放。

GC 侧经 `ContextReclaim` trait（`VmCore` 用 `CoreContextReclaimer` 捕 `Weak<VmCore>` 接线，
`heap.set_context_reclaimer`）驱动，reclaim 只在 major collect 做（minor 不扫全堆，`live_contexts` 不完备）。

## 边界与后续

- **Phase 2 不含**：**强制卸载 / tombstone/trap**（有引用就不回收）、`whyRetained` 诊断、细粒度
  hot-reload、**跨 context 执行**（collectible zpkg 只保证反射可见，其函数暂不可跨 context 调用）。
- 既有 `Std.Runtime.Runtime.LoadZpkg` / `CallStatic`（DEFERRED stub）保留不动；`AssemblyLoadContext.Load`
  是动态加载能力的正确归宿，后续单开小 change 收敛。

## 关联

- 引入：change `add-load-context-model`（`docs/spec/archive/`）。
- 目标架构：[load-context.md](../../../design/runtime/load-context.md) /
  [tiered-execution.md](../../../design/runtime/tiered-execution.md) /
  [safepoint.md](../../../design/runtime/safepoint.md)。
