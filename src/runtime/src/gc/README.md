# gc/

## 职责

z42 VM 的 GC 子系统：堆对象（`ScriptObject` / `Array`）的分配、引用追踪与
回收抽象 —— 通过 `trait MagrGC` 提供 host-friendly 嵌入接口（pin roots /
observers / profiler / weak refs / finalizers / strict OOM / ...）。

## 核心文件

| 文件 | 职责 |
|------|------|
| `heap.rs` | `trait MagrGC` —— GC 抽象接口（对齐 MMTk porting contract，10 能力组 ~30 方法）|
| `arc_heap.rs` | `ArcMagrGC` 协调器 —— struct/字段、`RcHeapInner`、句柄表、类型别名、`Default`/`Debug`、`new()` + concern 子模块声明（`GcRef` backing 是 `Rc<GcAllocation<T>>`，wrapper 含 finalizer Cell + 自定义 Drop）|
| `arc_heap/alloc.rs` | region 分配尾部 + OOM 兜底 + 内存压力检查 + size 估算/查询（`object_size_bytes`）|
| `arc_heap/collect.rs` | mark-sweep 原语：mark/sweep 阶段 + soft-ref 复活 + live 快照 |
| `arc_heap/control.rs` | 环回收编排与控制 API：`run_cycle_collection(_stw)` + `collect_cycles`/`force_collect` + finalize + soft-ref |
| `arc_heap/generational.rs` | 分代 GC：minor/major/promotion/card + `gen_age` + write barriers |
| `arc_heap/roots.rs` | roots/retention 扫描：root 快照 + marked-context 扫描 + 反向引用图 |
| `arc_heap/observe.rs` | 观测：barrier observer(test) + 事件分发 + pause 计时 + snapshot/stats |
| `arc_heap/interface.rs` | `impl MagrGC for ArcMagrGC` —— GC 公共 trait 接口（薄委托层，重方法体下沉到上列 concern 模块）|
| `arc_heap/debug.rs` | `#[cfg(test)]`/`#[cfg(debug_assertions)]` 辅助：test accessors + `debug_validate_invariants` |
| `refs.rs` | `GcRef<T>` / `WeakGcRef<T>` 不透明句柄 + `GcAllocation<T>` wrapper |
| `types.rs` | 支持类型 —— `RootHandle` / `FrameMark` / `GcEvent` / `GcObserver` / `WeakRef` / `HeapSnapshot` / `HeapStats` / `FinalizerFn` / `AllocSamplerFn` / ... |
| `heap_tests.rs` | trait 默认方法契约测试 |
| `arc_heap_tests.rs` | ArcMagrGC 行为单元测试（覆盖全 11 能力组 + cycle / drop-time finalizer / strict OOM 等）|

## 入口点

- `crate::gc::MagrGC` —— GC 接口 trait（10 能力组）
- `crate::gc::ArcMagrGC` —— 默认实现（GcRef backing 是 `Rc<GcAllocation<T>>`，Phase 3e 起 wrapper Drop 自动触发 finalizer）
- `crate::gc::GcRef<T>` / `crate::gc::WeakGcRef<T>` —— 堆引用不透明句柄；后续 backing 切换（自定义堆 / mark-sweep / MMTk）零 callsite 修改
- 嵌入相关类型：`RootHandle` / `FrameMark` / `GcEvent` / `GcObserver` / `AllocSample` / `WeakRef` / `HeapSnapshot` / `HeapStats` / ...

### 能力组（按 trait 内分组）

| # | 能力组 | 主要方法 |
|---|--------|---------|
| 1 | Allocation | `alloc_object` / `alloc_array` |
| 2 | Roots | `pin_root` / `unpin_root` / `enter_frame` / `leave_frame` / `for_each_root` |
| 3 | Write barriers | `write_barrier_field` / `write_barrier_array_elem`（默认 no-op，generational / 自定义堆 backend 时可重载）|
| 4 | Object Model | `object_size_bytes` / `scan_object_refs` |
| 5 | Collection | `collect` / `collect_cycles` / `force_collect` / `pause` / `resume` |
| 6 | Heap config | `set_max_heap_bytes` / `used_bytes` / `set_strict_oom` |
| 7 | Finalization | `register_finalizer` / `cancel_finalizer` |
| 8 | Weak refs | `make_weak` / `upgrade_weak` |
| 9 | Observers | `add_observer` / `remove_observer` |
| 10 | Profiler | `set_alloc_sampler` / `take_snapshot` / `iterate_live_objects` |
| 11 | Stats | `stats` |

### 典型使用

```rust
// 脚本驱动分配（VM 内部）
let v = ctx.heap().alloc_array(vec![Value::Null; n]);

// Host-side 嵌入集成
let h = ctx.heap().pin_root(value.clone());
let id = ctx.heap().add_observer(Arc::new(MyTelemetry {}));
ctx.heap().set_max_heap_bytes(Some(64 * 1024 * 1024));
ctx.heap().set_strict_oom(true);  // 启用后越限返 Null
let snap = ctx.heap().take_snapshot();
```

z42 脚本端可调 `Std.GC.Collect()` / `UsedBytes()` / `ForceCollect()`（见
`src/libraries/z42.core/src/GC.z42`）。

## 依赖关系

- 上游：`metadata::{Value, ScriptObject, TypeDesc, NativeData}`
- 下游：`vm_context::VmContext` 持有 `Box<dyn MagrGC>` + 注入 external root
  scanner 闭包（扫描 static_fields / pending_exception / interp+JIT exec_stack）；
  `interp/exec_instr.rs` 与 `jit/helpers_*.rs` 通过 `ctx.heap()` 调用

## Phase 路线

详见 [`docs/design/runtime/gc.md`](../../../../docs/design/runtime/gc.md).

**至 add-generational-gc 完成（2026-05-22）GC 主功能完整 —— A1 / A2 / A3 / A4
均已落地（custom allocator / mark-sweep / generational / concurrent mark），
三种 GcMode 可选 opt-in。可投产。**

后续可选迭代规划（剩余性能轨道 / 嵌入式工具 / 测试质量 / MMTk 集成）见同文档
["GC 后续迭代规划"](../../../../docs/design/runtime/gc.md#gc-后续迭代规划) 段，
每条目带 What / Why / Deps / Size / Risk 四元组，可按优先级独立启动 spec。

### 已完成 Phase 速览

| Phase | 主要内容 |
|-------|---------|
| 1 / 1.5 | trait MagrGC + ArcMagrGC + 全 host-side 嵌入接口 + corelib NativeFn 签名带 `&VmContext` |
| 3a | `GcRef<T>` / `WeakGcRef<T>` 不透明句柄抽象 |
| 3b | Heap registry + snapshot/iterate Full coverage |
| 3c | Trial-deletion 环回收器（Bacon-Rajan）→ 修复环引用泄漏 |
| 3d | Finalizer 真触发 + 内存压力自动 collect + `near_limit_warned` reset |
| 3d.1 | External root scanner —— VmContext `static_fields` / `pending_exception` 自动暴露给 cycle collector |
| 3d.2 | `Std.GC.*` 脚本暴露 + 端到端 golden test 验证（110_gc_cycle）|
| 3e | `GcRef<T>` backing 升级 `Rc<GcAllocation<T>>` —— Drop 自动触发 finalizer（含纯 Rc Drop 路径）|
| 3f | interp 栈扫描（`FrameGuard` RAII push/pop frame.regs 到 exec_stack）|
| 3f-2 | JIT 栈扫描（6 个 JitFrame::new callsite 同样模式）+ 112_gc_jit_transitive 验证 |
| 3-OOM | strict OOM 模式（`set_strict_oom`；启用后 alloc 越限返 Null 不入 registry/stats）|

## 命名

**MagrGC** 取自《银河系漫游指南》中的 **Magrathea** —— 那颗专门建造定制行星的传奇世界。
