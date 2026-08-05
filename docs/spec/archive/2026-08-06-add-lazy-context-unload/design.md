# Design: 惰性卸载（Lazy Context Unload）—— ALC Phase 2

> 上位设计：[load-context.md](../../../design/runtime/load-context.md) §8 卸载流程。
> Phase 1 地基（ContextRegistry / NativeData 句柄 / Assembly 反射）见 archive
> `2026-07-30-add-load-context-model`。本 change 只做**惰性卸载**，不含诊断/强制/跨 context 执行。

## Architecture

```
  AssemblyLoadContext.Unload()  ──[Native __lctx_unload]──▶  ContextRegistry.unload(ctx)
                                                               ├ root → InvalidOperation
                                                               ├ 已 Unloading → 幂等
                                                               └ Active → state=Unloading, unloading_count++

  GC collect（STW 安全点）:
    mark_phase:  每 mark 一个 ScriptObject
                   └─(仅当 unloading_count>0)─▶ note_live_context(obj):
                        obj.type_desc  ─┐
                        obj.native 句柄 ─┴─▶ 反查表 *const TypeDesc→ctx / 句柄→ctx
                                              命中 collectible → live_contexts.insert(ctx)
    sweep_phase: 照常回收用户堆对象
    reclaim_pass（末尾，STW 内）:
       for ctx in Unloading:
         if ctx ∉ live_contexts:  drop(AssemblyEntry.module) → Arc 归零确定性 free
                                   + registry 除名 + 反查表清 + unloading_count--
         else:                    保持 Unloading（等自然死，下轮再判）
```

## Decisions

### D1: TypeDesc → ContextId 用 registry 侧 `*const TypeDesc` 反查表（不 mutate TypeDesc）
**问题：** GC mark 到活 `ScriptObject` 只有 `type_desc: Arc<TypeDesc>`，如何反查其 context？
**选项：** A—给 `TypeDesc` 加 `context` 字段（要动 loader build + TypeDesc 变 Clone）；B—registry 建
`HashMap<*const TypeDesc, ContextId>`，`load_into` 时登记 collectible 类型。
**决定（User 确认）：** 选 B。延续 Phase 1 D5——不 mutate `TypeDesc`（其非 Clone、无锁读契约）。
collectible 类型少 → 表小、O(1) 查。root 类型**不登记**（不在表里 = root = 不参与回收）。
`Arc::as_ptr` 给稳定指针；drop arena 时同步清表项。

### D2: reclaim 挂在 major GC sweep 后（惰性，跟随 GC 节奏）
**问题：** 何时判定 + 回收？
**决定（User 确认）：** GC major collect 末尾、sweep 之后、仍在 STW 内做 reclaim pass。**不做**
"unload 立即触发一次 GC"——纯惰性，跟随正常 GC 节奏。minor GC 也带 liveness 钩子（避免漏标），
但 reclaim 只在 major collect 做（major 才完整扫全堆，保证 live_contexts 完备）。

### D3: `Unload()` 保持 `void`（fire-and-forget）
**问题：** Unload 返回值？
**决定（User 确认）：** `void`，对齐 .NET 语义（卸载异步/惰性，不保证立即回收）。"是否已真正回收"
留待后续 whyRetained/事件观察。保持最简。

### D4: 保留边 = 活实例 type_desc + 反射对象 native 句柄（都要看）
**问题：** 哪些活对象算"引用了 collectible context"？
**决定：** 两类，缺一不可：
1. **活实例**：`ScriptObject.type_desc` 指向 collectible 类型（该 context 类型的实例）。
2. **反射对象**：`ScriptObject.native` 为 `TypeHandle(Arc<TypeDesc>)` / `AssemblyHandle(aid)` /
   `LoadContextHandle(cid)` 指向 collectible。**`typeof(collectibleType)` 的 `Std.Type` 对象其
   `type_desc` 是 root 的 `Std.Type` 类，但 native 句柄钉住 collectible TypeDesc**——只看 type_desc
   会漏，导致提前 free 仍被引用的 arena。这条是正确性关键。

### D5: 回收为何确定性 free
reclaim 只在 `live_contexts` 不含该 ctx 时发生 → 此刻**无任何活实例/反射对象持有其 `Arc<TypeDesc>`**
→ `drop(AssemblyEntry.module)` 使 `Vec<Function>` / 串池 / `type_registry` 的 Arc refcount 归零 →
全部立即释放。liveness 判定**精确 gate** 了"drop 即能全放"这个前提。

## Implementation Notes

- **零回归**：`note_live_context` 钩子在 mark 循环里先查 `unloading_count > 0`（AtomicUsize，Relaxed）
  再做任何事——正常无卸载路径零额外开销。
- **GC 访问 registry**：GC reclaim 需读写 `VmCore.context_registry`。GC 已持 `Weak<VmCore>`（scanner
  闭包）；reclaim pass 经它 upgrade 后取 registry。mark 钩子的反查表可在 collect 开始时快照进 GC 局部
  （避免 mark 循环里反复锁 registry），collect 结束时用累积的 live_contexts 调 registry.reclaim。
- **`Arc::as_ptr` 稳定性**：TypeDesc 一旦建成不移动（Arc 堆固定），`as_ptr` 在其存活期稳定。drop 前
  清表项，无悬垂 ptr 复用风险（reclaim 与登记都在 STW/registry 锁下）。
- **minor GC**：young-only mark 也带 liveness 钩子（collectible 新实例可能在 young），但 reclaim 只
  major 做（minor 不扫 old，live_contexts 不完备，不能据此回收）。
- **单测可测性**：ContextRegistry 的 state 机 + reclaim 判定纯逻辑（给 registry 喂合成 Module +
  模拟 live_contexts 集），不需真 GC。e2e 测 z42 级端到端（ForceCollect 后行为）。

## Testing Strategy

- **Rust 单测**（`metadata/context_tests.rs`）：unload→Unloading / root 拒绝 / 幂等 / 再 Load 拒绝；
  reclaim 判定（ctx ∉ live → drop + 除名；ctx ∈ live → 保留）；反查表 `*const TypeDesc→ctx` 登记与清除。
- **e2e golden**（`src/tests/reflection/load_context_unload/`）：建 collectible + Load + Unload +
  `Std.GC.ForceCollect()` → 观测回收（如 GC.UsedBytes 降 / 后续访问语义）；有活实例分支不回收；root
  Unload 抛。
- **零回归**：完整 `xtask test`（e2e + cross-zpkg + stdlib + compiler 自举 + vscode-syntax）。
- **GC 正确性**：ForceCollect 反复调不误回收仍被引用的 context（活实例/反射对象保留边）。
