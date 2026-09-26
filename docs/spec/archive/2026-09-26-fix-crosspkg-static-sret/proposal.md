# Proposal: 跨包静态调用返回 blob struct 时漏传 sret

> 状态：✅ 已实施（2026-09-26）。类型：**fix**（轻量变更：无新语义、无格式变更、无新诊断）
> ⇒ 按 workflow 直接 IMPL；无 design.md —— **没有设计分叉**，修法就是让静态捷径与
> **早已正确的实例捷径**同口径。

## Why

`CallEmitter._emitCall` 里有三条「绕过 VCall 的捷径」，其中两条实例路一直正确处理 sret，
**静态那条从一开始就没处理**：

| 捷径 | 落点 | sret |
|---|---|---|
| devirt（sealed/精确类） | `CallEmitter.z42:230-240` | ✅ 拼 `sretH` |
| DepIndex **instance** | `CallEmitter.z42:253-265` | ✅ 拼 `sretH` |
| **DepIndex `static`** | `CallEmitter.z42:295-303` | 🔴 **直接 `new CallInstr`，无 sret** |
| **静态属性访问器（依赖分支）** | `_emitStaticAccessor` | 🔴 同上（本地分支走 `_emitCallSretAware`）|

后果：**跨包调用一个返回 blob 值 struct（≥2 字段）的静态方法/静态属性，编译期零诊断、
运行期必抛** `takes N+1 physical argument(s), the call passes N` —— 生产方按 sret 编（末尾隐藏
返回槽），消费方少传一个。

**为什么四个月没人发现**：全仓**没有任何 golden 跨包调用过「返回 struct 的方法」**。
`struct_cross_pkg` 只测跨包**构造**与**字段读**；而 `z42.core` 里**一个多字段 struct 都没有**
（只有 `GCHandle`/`Guid` 各 1 字段 + 12 个零字段基元 wrapper）⇒ 这条路在 stdlib 上根本走不到。

发现过程：做坑点 ⑤（单字段 struct 值语义）时翻开 `IsBlobStruct` 闸门，`GCHandle.AllocStrong`
跨包崩。原以为是 ⑤ 引入的，**自造跨包 fixture 后发现双字段 `Pair.Make` 在 main 上就崩**
⇒ 与 ⑤ 无关的既存 bug，⑤ 只是把它暴露出来。

⚠️ **一条被撤回的证据**：我曾用 `z42c --dump-ir` 看调用点，得出「跨包发的是 loose VCall」——
**那是工具假象**：`--dump-ir` / `--dump-bound` **从不加载 stdlib/依赖**（带 `Z42_LIBS` 也一样，
恒 19 个 error）。真相靠编译器内打点（`Z42C_TRACE_SRET` 临时探针）取得：
两处 sret-aware 发射器**一次都没被走到**，因为静态捷径根本不经过它们。

## What Changes

两处依赖分支改走**同一个** sret-aware 发射器：

- `_emitCall` 的 `c.Kind == "static" && !ownerIsLocal` 分支 → `_emitCallSretAwareG(depFq, null, args, …)`
  （顺带把 `MethodTypeArgs` 交给它，行为不变）
- `_emitStaticAccessor` 的依赖分支 → `_emitCallSretAware(depFq, null, args, …)`

无新指令、无 wire 格式变更、无新诊断码。

## 判别力

新 fixture `src/tests/cross-zpkg/single_field_struct_cross_pkg/`：
**双字段** `Pair.Make(3,4)`（修前即崩 = 本 change 的正面用例）+ **单字段** `Handle.Make(7)`
（形状对齐 `Std.GCHandle`：私有字段 + 静态工厂；今天非 blob ⇒ 走通，坑点 ⑤ 翻门后才变 blob，
届时这条 fixture 自动升级成 ⑤ 的守卫）。

**阴性对照**：同一 fixture 在修复前红在
`Demo.SfsTarget.Pair.Make$2$long$long ... takes 3 physical argument(s), the call passes 2`。
