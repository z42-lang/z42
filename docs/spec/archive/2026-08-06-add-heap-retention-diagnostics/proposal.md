# Proposal: 通用堆保留诊断（Heap Retention Diagnostics）

## Why

z42 自有精确 GC，能回答 .NET GC 回答不了的问题：**"这个对象为什么还活着 / 被谁钉住"**。这是
load-context.md §5「保留根诊断」的能力，但**不绑定 AssemblyLoadContext**——做成**通用**的、查询
任意堆对象保留关系的 `Std.Diagnostics` 接口（context 卸载迟迟不回收的诊断变成它的一个应用场景）。

典型用途：内存泄漏定位（"这个对象本该死却活着，被谁引用"）、collectible context 卸载不回收排查。

## What Changes

- **反向引用图堆扫描**（`gc/retention.rs`）：诊断时遍历两个对象区（`region_object` + `region_array`）
  + 分类根集，对每个对象 `trace_children` 记 `child ← parent` 反向边、`obj ← root` 根边。
- **分类根枚举**：现有 `external_root_scanner` 只吐匿名 Value；新增**带类别**的根枚举（StaticField /
  StackFrame / Pinned / FuncRefSlot），供 L2 报根到类别级。
- **whyRetained 查询**（两层）：
  - **L1 直接引用者**：反向边一跳 → 直接持有 target 的对象/根。
  - **L2 保留根**：从 target 反向 BFS 到根集 → 报可达的 GC 根（类别级）。
- **`Std.Diagnostics.Heap`（新 stdlib 类）**：`DirectReferrers(object) -> Retainer[]` /
  `RetainingRoots(object) -> RootRef[]`。触发一次 full GC 换取准确（存活即可达，无浮动垃圾误报）。
- **描述类型**：`Std.Diagnostics.Retainer`（引用者对象：Type + Label + id）、
  `Std.Diagnostics.RootRef`（根：Kind + Label）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/gc/retention.rs` | NEW | 反向图堆扫描 + L1/L2 查询 + 结果类型（`RetentionQuery` / `RetainerInfo` / `RootInfo` / `RootKind`） |
| `src/runtime/src/gc/mod.rs` | MODIFY | `pub mod retention;` |
| `src/runtime/src/gc/heap.rs` | MODIFY | MagrGC trait 加 `retention_direct_referrers` / `retention_roots` + 分类根 scanner 类型/setter（默认 no-op） |
| `src/runtime/src/gc/arc_heap.rs` | MODIFY | impl 上述（iterate 两区建反向图 + BFS）+ `categorized_root_scanner` 字段/setter |
| `src/runtime/src/vm_context.rs` | MODIFY | 接线分类根 scanner（按类别枚举 static_fields / 帧 regs / func_ref_slots / pinned） |
| `src/runtime/src/corelib/diagnostics.rs` | NEW | builtins `__heap_direct_referrers` / `__heap_retaining_roots`（调 heap 查询 → 建 z42 Retainer[]/RootRef[]） |
| `src/runtime/src/corelib/mod.rs` | MODIFY | `pub mod diagnostics;` + BUILTINS 表尾追加 |
| `src/libraries/z42.diagnostics/src/Heap.z42` | NEW | `Std.Diagnostics.Heap` 静态类（放 z42.diagnostics——`Std.Diagnostics` 命名空间的拥有者，避免跨包分裂） |
| `src/libraries/z42.diagnostics/src/Retainer.z42` | NEW | 引用者描述 |
| `src/libraries/z42.diagnostics/src/RootRef.z42` | NEW | 根描述 |
| `src/libraries/z42.diagnostics/src/RootKind.z42` | NEW | `RootKind` enum |

> **实施期 Scope 校正**：`Std.Diagnostics` 命名空间已由 **`z42.diagnostics` 库**拥有（Log/LogFields/LogLevel）。最初误放 z42.core → 命名空间跨 z42.core+z42.diagnostics 分裂，令 z42.threading 的 `Std.Diagnostics.Log.Error` 跨 zpkg 静态解析冲突（gate 实测 `undefined function`）。修：4 个类移入 z42.diagnostics（拥有者），命名空间单包不分裂。builtins 仍在 z42.core 的 corelib（全局注册，非包绑定）。
| `src/runtime/src/gc/retention_tests.rs` | NEW | Rust 单测：反向图 / L1 直接引用者 / L2 根可达 / 分类根 |
| `src/tests/reflection/heap_retention/source.z42` | NEW | e2e：构造引用关系 → DirectReferrers / RetainingRoots 断言 |
| `src/tests/reflection/heap_retention/expected_output.txt` | NEW | 期望输出 |
| `docs/book/src/runtime/heap-diagnostics.md` | NEW | book 机制页：反向图 + L1/L2 + 分类根 + API |
| `docs/book/src/SUMMARY.md` | MODIFY | 挂入新页 |
| `docs/design/runtime/load-context.md` | MODIFY | §5 对齐：whyRetained 已泛化为通用 Std.Diagnostics.Heap（L1+L2 落地，L3 延后） |

**只读引用**：
- `src/runtime/src/gc/arc_heap.rs` `iterate_alive` / `mark_phase` / `external_root_scanner`（复用堆遍历/根扫描范式）
- `src/runtime/src/metadata/types.rs` `Value::trace_children` / `ScriptObject` / `ArrayObj`（正向边来源，反向即取反）

## Out of Scope

- **L3 完整引用链**（根 → … → target 的整条路径 + 多路径去重 + 环处理）—— 下一 change。
- **具体根名标注**（哪个 static 字段名 / 哪个局部变量）—— 需 root-source 精确标签，本 change 只到**类别级**。
- **跨线程/跨 VmContext 精确归属**——分类根按 VmCore 聚合枚举，不细分到具体线程栈帧对象。
- **常态零开销的保留边注册**（load-context.md §5 第 1 层「框架边」）——本 change 走**按需堆扫描**（L2 第 2 层路线），不建常驻注册表。

## Open Questions

- [ ] （无——3 决策已敲定：L1+L2 本 change / 根类别级 / 触发 full GC）
