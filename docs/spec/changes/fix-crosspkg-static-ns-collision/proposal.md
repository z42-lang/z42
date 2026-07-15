# Proposal: 跨包静态调用按命名空间消歧（根治短类名 first-wins 串味）

> 状态：🟢 已完成 | 类型：fix（编译器 codegen 解析）| 子系统：compiler
> 这是 `converge-z42c-onto-z42-project`（path C，User 裁决）的**使能前置**：根治后 z42.project 与
> z42c.project 可安全共存，converge 无需 rename-前置 / CI 手术。

## Why

z42 flat `Z42_LIBS` 下，跨 zpkg 静态调用经 `DependencyIndex` 解析，键是**去命名空间的短类名**
`ShortCls.Method`（`DependencyIndex.AddModule`：`cls + "." + method`，cls 为末段类名），`TryAdd`
**first-wins**。→ 两个包若有**同短类名不同命名空间**的类（如 `Z42.Project.SourceDiscovery` 与
`Z42.Build.Project.SourceDiscovery`），谁先注册谁赢，调用方**无视自己的 `using`** 被绑到错的那份。

- 这正是 `common-pitfalls.md §1` 记的 `Std.Assert.Equal` vs `Std.Test.Assert.Equal` 同键问题
  ——此前仅靠「prelude-first 排序」让 first-wins 碰巧选对，是 workaround 非根治。
- 也是 `converge` 的 SPIKE（2026-07-11）实测「共存即炸」的根因：登记 `z42.project` 后
  z42c.driver 的 `SourceDiscovery.Discover` 被绑到 `Z42.Build.Project.SourceDiscovery`，还烤进
  `z42c.pipeline.zpkg`。

**VM 侧无需改**（实测确认）：Rust VM 按**全 FQN 精确匹配**解析 CallInstr，不同 ns 天然不撞键；
问题纯在编译期 z42c 把不同 FQN 映射到同一短键、烤错 FQN。→ 修 z42c 解析即可。

## What Changes（root-cause：using-scoped 解析）

调用方的 CU 只应看见它 `using` 到的命名空间里的类。据此：

1. **`DependencyIndex`**：`AddModule` 额外注册**全名键** `ns.ShortCls.Method[$arity]`；新增
   `GetStaticScoped(activeNs[], count, shortCls, method)`——按 CU 活跃 ns 集尝试全名键，**恰好一个
   不同实体命中**才返回（消歧成功）；0 个或 ≥2 个歧义 → 返回 null。
2. **活跃 ns 集**：`IrDump._activeNamespaces(cu)` = 全部 `using` + 本 CU ns；经 `IrGen`/`EmitContext`
   /`FunctionEmitter` 线程到 emit。
3. **`ExprEmitter` 静态调用**：先 `GetStaticScoped`（arity→bare），未唯一命中再回落既有短键
   `GetStatic` first-wins（**既有行为，字节不动**）。
4. **local-wins 守卫**：本包自有类（`LocalClasses`）恒走本地 emit、不查 DepIndex——否则依赖含同短名
   类时，self-exclude 的 DepIndex 让 scoped 落空、短键 fallback 误绑依赖那份。

## 为什么字节不动（byte-identical-safe）

`GetStaticScoped` 只在**活跃集内恰好一个实体**命中时消歧；活跃集内歧义（如 Assert.Equal 的
Std/Std.Test 同时激活）→ 返回 null → 回落短键 first-wins（prelude-first，原判不变）。非冲突调用
（短名唯一）两路同果。local-wins 守卫在无同名依赖时与 self-exclude 同果。→ 现有树零解析漂移。

## Scope（允许改动的文件）

| 文件 | 变更 |
|------|------|
| `src/compiler/z42c.ir/src/DependencyIndex.z42` | MODIFY：全名键注册 + `GetStaticScoped` |
| `src/compiler/z42c.semantics/src/EmitContext.z42` | MODIFY：`ActiveNs` 字段 |
| `src/compiler/z42c.semantics/src/IrGen.z42` | MODIFY：`ActiveNs` 字段 |
| `src/compiler/z42c.semantics/src/FunctionEmitter.z42` | MODIFY：3 处 copy `ActiveNs` |
| `src/compiler/z42c.semantics/src/IrDump.z42` | MODIFY：`_activeNamespaces` + 两入口设 `gen.ActiveNs` |
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` | MODIFY：scoped 优先 + local-wins 守卫 |
| `src/compiler/z42c.ir/tests/depindex/depindex_tests.z42` | MODIFY：`GetStaticScoped` 消歧单测 |
| `.claude/rules/common-pitfalls.md` | MODIFY：§1 注记根治 |

## 验证（本地实测）

- **自举不动点 7/7 byte-identical**（gen1==gen2；含 local-wins 守卫）→ 现有树零漂移。
- **共存实测**：临时登记 z42.project 为 member 编 z42c——**不再崩**（SPIKE 曾崩），且**与不登记
  z42.project 编出的 z42c 逐字节 7/7 相同**（z42.project 存在与否对 z42c 零影响）。已回退临时登记。
- 全 e2e/stdlib gate 以 CI 为权威（自举字节不动 ⇒ golden .zbc 不变 ⇒ 行为不变）。

## Out of Scope
- 真正的 converge（登记 z42.project member + 删 z42c.project manifest-model + z42c 切 z42.project）
  ——本前置落地后由 `converge-z42c-onto-z42-project` 原子完成（现已不需 rename / CI 手术）。
