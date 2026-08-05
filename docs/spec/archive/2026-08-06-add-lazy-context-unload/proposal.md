# Proposal: 惰性卸载（Lazy Context Unload）—— ALC Phase 2

## Why

Phase 1（`add-load-context-model`）落地了 `AssemblyLoadContext` 边界 + zpkg 反射身份，但
`Unload()` 只是**声明并抛 `NotSupportedException`**——还不能真正回收 collectible context 的内存。
本 change 让 **`Unload()` 真正生效**：标记 context 为 `Unloading`，由 GC 惰性判定"无引用即回收"，
确定性 free 其 arena（码 + 元数据 + 串池）。这是"各种粒度可回收"目标的**第一次真实回收**。

采用 Erlang current/old 语义：**有活实例/反射引用就不回收，等它们自然死**（无 tombstone——
强制清理是 Phase 3）。

## What Changes

- **`AssemblyLoadContext.Unload()`**：从 `throw` 改为 `[Native("__lctx_unload")]`——标记 context
  `Unloading`（root 抛 `InvalidOperationException`；再 `Load` 进 Unloading context 抛）。
- **Context 状态机**：`ContextEntry` 加 `state: Active | Unloading`；`ContextRegistry` 加
  `unloading_count`（原子标志，正常无卸载时 GC 钩子零成本）。
- **TypeDesc → ContextId 反查表**：`ContextRegistry` 在 `load_into` 时登记
  `HashMap<*const TypeDesc, ContextId>`（collectible 类型 → 其 context），供 GC mark 反查。
- **GC mark 保留边钩子**：`mark_phase` 每 mark 一个 `ScriptObject`，检查它的 `type_desc` +
  `NativeData::{TypeHandle,AssemblyHandle,LoadContextHandle}` 是否指向某 collectible context →
  记入本轮 `live_contexts`。仅在 `unloading_count > 0` 时激活。
- **reclaim pass**：GC collect 末尾（sweep 后、STW 内），把 **`Unloading` 且不在 `live_contexts`**
  的 context 的 `AssemblyEntry.module` **drop**（Arc refcount 归零 → 确定性 free），registry 除名 +
  清反查表。
- **新 builtin `__lctx_unload`**。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/metadata/context.rs` | MODIFY | ContextState 枚举 + ContextEntry.state + unloading_count + TypeDesc→ctx 反查表 + `unload`/`reclaim`/`note_live_context` 方法 |
| `src/runtime/src/corelib/assemblyloadcontext.rs` | MODIFY | `__lctx_unload` builtin（标记 Unloading；root/已 Unloading 处理）；`load_into` 调用点登记反查表 |
| `src/runtime/src/corelib/mod.rs` | MODIFY | BUILTINS 表尾追加 `__lctx_unload` |
| `src/runtime/src/gc/arc_heap.rs` | MODIFY | `mark_phase`（+ minor）加 per-object context-liveness 钩子；collect 末尾 reclaim pass；持 `Weak<VmCore>` 或经 scanner 访问 registry |
| `src/runtime/src/vm_context.rs` | MODIFY | 暴露 registry 给 GC reclaim（或经既有 scanner 通道）；reclaim 回调接线 |
| `src/libraries/z42.core/src/Runtime/AssemblyLoadContext.z42` | MODIFY | `Unload()` body 改 `[Native("__lctx_unload")]` extern |
| `src/runtime/src/metadata/context_tests.rs` | NEW | 单测：状态机 / 反查表 / reclaim 判定（有引用不回收、无引用回收、root 拒绝） |
| `src/tests/reflection/load_context_unload/source.z42` | NEW | e2e：Unload collectible（无活实例）→ GC → 可回收；有活实例 → 不回收；root Unload 抛 |
| `src/tests/reflection/load_context_unload/expected_output.txt` | NEW | 期望输出 |
| `src/tests/reflection/load_context/source.z42` | MODIFY | Phase 1 test 的 Unload 断言随行为变更更新（collectible Unload 不再抛 NotSupported → 标 Unloading） |
| `src/tests/reflection/load_context/expected_output.txt` | MODIFY | 末行 `unload-not-supported` → `unloaded` |
| `docs/book/src/runtime/load-context.md` | MODIFY | 补「卸载 / 回收」节（状态机 + GC 保留边 + reclaim 机制） |
| `docs/design/runtime/load-context.md` | MODIFY | 页头对齐：Phase 2 惰性卸载已落地 |

**只读引用**：
- `src/runtime/src/gc/arc_heap.rs` `mark_phase`/`sweep_phase`/`ExternalRootScanner`（理解 GC 钩子点）
- `src/runtime/src/metadata/types.rs` `ScriptObject`/`NativeData`/`GcRef::type_desc`（保留边来源）

## Out of Scope

- **`whyRetained` 诊断**（第 1/2 层）—— User 定的延后（GC 相关，后续 change）。
- **强制卸载 / tombstone/trap**（Phase 3）——本 change 纯惰性，有引用就不回收。
- **跨 context 执行**及其保留边（cross_module_targets 缓存等）—— 后续；Phase 2 collectible 仍只反射可见。
- **静态状态迁移钩子**（load-context.md §7）—— 后续。
- **`unload` 立即触发一次 GC**——不做，跟随正常 GC 节奏（惰性）。

## Open Questions

- [ ] （无——3 处设计决策已与 User 敲定：registry 侧 `*const TypeDesc` 反查表 / reclaim 挂 major GC sweep 后 / `Unload()` 保持 `void`）
