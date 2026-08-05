# Tasks: 惰性卸载（Lazy Context Unload）—— ALC Phase 2

> 状态：🟢 已完成 | 创建：2026-08-05 | 完成：2026-08-06 | User 6.5 已确认
> 变更类型：`vm`（完整流程）| 分支：`worktree-lazy-context-unload`（off origin/main）

## 进度概览
- [x] 阶段 1: Context 状态机 + 反查表（context.rs）
- [x] 阶段 2: `__lctx_unload` builtin + load_into 登记反查表
- [x] 阶段 3: GC mark 保留边钩子 + reclaim pass
- [x] 阶段 4: stdlib Unload 改 extern
- [x] 阶段 5: 测试（Rust 单测 7/7 + e2e interp&jit）
- [x] 阶段 6: 验证 + 文档同步

## 验证报告（GREEN）
- ✅ cargo build (release z42vm) 无错、无新 warning
- ✅ Rust 单测 `context_tests`：7/7（状态机 / unload / reclaim 判定 / 反查表 / 反射保留边）+ Phase 1 `assemblyloadcontext_tests` 仍绿
- ✅ e2e：load_context_unload interp+jit 输出全对；load_context（Phase 1，Unload 断言更新）通过
- ✅ **完整 `xtask test` 全绿**：e2e 218/0 + cross-zpkg + stdlib [Test] 全库 0 failed + **compiler 自举 5/5 gen1==gen2 逐字节复现**（context.rs/GC/stdlib 改动不破不动点）+ vscode-syntax
- ✅ 零回归：无 Unloading 时 GC 钩子 gate on `unloading_count>0`，正常路径零开销

## 备注（实施期）
- **Scope 校正**：Phase 2 改了 `Unload()` 语义（collectible 不再抛 NotSupported）→ Phase 1 的
  `load_context` e2e 断言（末行 `unload-not-supported`）随之更新为 `unloaded`；已补入 Scope。
- 文档：book `load-context.md` 加「卸载/回收」节 + API 表 Unload 行 + 页头；design 页头 Phase 2 对齐。

## 阶段 1: Context 状态机 + 反查表（context.rs）
- [ ] 1.1 `ContextState { Active, Unloading }` + `ContextEntry.state`
- [ ] 1.2 `ContextRegistry.unloading_count: usize`（或包在 registry，GC 侧读原子标志——见 1.5）
- [ ] 1.3 `td_to_ctx: HashMap<*const TypeDesc, ContextId>` + `load_into` 登记 collectible 类型（root 不登记）
- [ ] 1.4 `unload(ctx)`：root→Err(InvalidOp) / 已 Unloading→幂等 Ok / Active→Unloading + count++
- [ ] 1.5 `note_live_context(td_ptr / handle) -> Option<ContextId>` 查询 + `reclaim(live: &HashSet<ContextId>)`：drop 无引用 Unloading ctx 的 module + 除名 + 清反查表 + count--
- [ ] 1.6 `is_unloading` 原子标志（`AtomicUsize`）供 GC 零成本 gate

## 阶段 2: builtin + 登记
- [ ] 2.1 `corelib/assemblyloadcontext.rs`：`__lctx_unload`（ctx_handle → registry.unload；root Err 转 z42 InvalidOperationException）
- [ ] 2.2 `builtin_lctx_load` 登记反查表（load_into 内或紧邻）；Unloading ctx 的 Load 抛
- [ ] 2.3 `corelib/mod.rs`：BUILTINS 表尾追加 `__lctx_unload`

## 阶段 3: GC 集成
- [ ] 3.1 `arc_heap.rs` `mark_phase`：per-object `note_live_context` 钩子（gate on unloading_count>0），累积 `live_contexts`
- [ ] 3.2 `mark_phase_minor`：同钩子（young 新实例）
- [ ] 3.3 collect 末尾 reclaim pass（major only）：经 `Weak<VmCore>` 取 registry → `reclaim(live_contexts)`
- [ ] 3.4 `vm_context.rs`：reclaim 通道接线（registry 访问）

## 阶段 4: stdlib
- [ ] 4.1 `AssemblyLoadContext.z42`：`Unload()` body 改 `[Native("__lctx_unload")]` extern（删 throw）

## 阶段 5: 测试
- [ ] 5.1 `metadata/context_tests.rs`：状态机 + reclaim 判定 + 反查表登记/清除
- [ ] 5.2 `src/tests/reflection/load_context_unload/`：e2e（Unload+ForceCollect 回收 / 活实例不回收 / root 抛）
- [ ] 5.3 spec scenarios 逐条覆盖

## 阶段 6: 验证 + 文档
- [ ] 6.1 `cargo build --release` 无错 + Rust 单测
- [ ] 6.2 完整 `xtask test`（e2e + cross-zpkg + stdlib + compiler 自举 gen1==gen2 + vscode）全绿
- [ ] 6.3 零回归确认（无 Unloading 时 GC 行为不变）
- [ ] 6.4 book `load-context.md` 补「卸载/回收」节 + design 页头对齐
- [ ] 6.5 归档 + PR（rebase origin/main + 重跑 GREEN + squash merge）

## 备注
- **决策（design.md）**：D1 registry 反查表不 mutate TypeDesc / D2 reclaim 挂 major sweep 后 / D3 Unload void / D4 保留边含反射对象 native 句柄（正确性关键）。
- **零回归铁律**：GC 钩子先 gate `unloading_count>0`，正常路径零开销。
- **验证重点**：ForceCollect 不误回收仍被引用（活实例 + Std.Type 反射对象）的 context。
