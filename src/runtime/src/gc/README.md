# gc/

## 职责

z42 VM 的 GC 子系统：堆对象（`ScriptObject` / `Array`）的分配、引用追踪与
回收抽象 —— 通过 `trait MagrGC` 提供 host-friendly 嵌入接口（pin roots /
observers / profiler / weak refs / finalizers / strict OOM / ...）。

## 核心文件

| 文件 | 职责 |
|------|------|
| `heap.rs` | `trait MagrGC` —— GC 抽象接口（MMTk porting contract 形态，11 能力组） |
| `mode.rs` | `GcMode`：`GenerationalMarkSweep`（默认）/ `StwMarkSweep`，`Z42_GC_MODE` 选择 |
| `os_mem.rs` | 页粒度内存：slab 的 `mmap`/`munmap`（非 unix 走分配器）、`decommit`/`recommit`（`madvise`），chunk 池 decommit 用。机制见 [book: GC TLAB · decommit](../../../../docs/internals/src/runtime/gc-tlab.md#池中空-chunk-的-decommit) |
| `footprint.rs` | `Footprint`：堆的真实占用（committed / pooled / occupied），三个 region 共用一份、随变随记；`malloc_size` 分配器取整模型。机制见 [book: GC 调参 · 真实占用记账](../../../../docs/internals/src/runtime/gc-tuning.md#真实占用记账与软上限) |
| `arc_heap.rs` | `ArcMagrGC` 协调器 —— struct/字段、句柄表、类型别名、`Default`/`Debug` + concern 子模块声明 |
| `arc_heap/construct.rs` | `Default` 构造（`new()` 委托到它） |
| `arc_heap/alloc.rs` | region 分配尾部 + OOM 兜底 + 内存压力检查 + size 估算/查询（`object_size_bytes`）|
| `arc_heap/alloc_black.rs` | 增量 major 周期内的 allocate-black |
| `arc_heap/auto_collect.rs` | 自动 collect 触发策略（分配压力） |
| `arc_heap/collect.rs` | mark-sweep 原语：mark/sweep 阶段 + soft-ref 复活 + live 快照 |
| `arc_heap/control.rs` | 环回收编排与控制 API：`run_cycle_collection(_stw)` + `collect_cycles`/`force_collect` + finalize + soft-ref |
| `arc_heap/generational.rs` / `barrier.rs` | 分代 GC：minor/major/promotion/card + `gen_age`；分代写屏障 |
| `arc_heap/card_verify.rs` | 卡表不变量的从头核对（`verify_card_invariant`；`Z42_GC_VERIFY_CARDS` 每次 minor 前跑）——抓漏掉的写屏障。见 [book: GC · 卡表不变量核对](../../../../docs/internals/src/runtime/gc.md#卡表不变量核对z42_gc_verify_cards) |
| `arc_heap/promotion_policy.rs` / `pause_budget.rs` | 自适应晋升判定 / 停顿预算化 nursery |
| `arc_heap/young_policy.rs` | 年轻代策略：按回收收益（对比上一次 major）判定 minor，不划算时把 minor 换成 **tenure**（年轻代整体晋升、不标记）。机制见 [book: GC 调参 · 年轻代策略](../../../../docs/internals/src/runtime/gc-tuning.md#年轻代策略按收益判定-minor不划算就-tenure) |
| `arc_heap/roots.rs` | roots/retention 扫描：root 快照 + marked-context 扫描 + 反向引用图 |
| `arc_heap/observe.rs` | 观测：barrier observer(test) + 事件分发 + pause 计时 + snapshot/stats |
| `arc_heap/footprint.rs` | 真实占用的堆侧：region 的 payload 度量函数、`committed` / `occupied` 读数、软上限的两种单位（`SoftCap`）、sweep 尾部的 chunk 回收 + 侧表重量 |
| `arc_heap/incremental.rs` | **增量 major**：切片状态机（Marking / Sweeping）、切片预算、pacer、`admit_resurrected`（弱读 / 堆遍历不交出待清扫的死对象）。机制见 [book: 增量 major](../../../../docs/internals/src/runtime/gc-incremental-major.md) |
| `arc_heap/interface.rs` | `impl MagrGC for ArcMagrGC` —— 公共 trait 接口（薄委托层，重方法体下沉到上列 concern 模块）|
| `arc_heap/debug.rs` | `#[cfg(test)]`/`#[cfg(debug_assertions)]` 辅助：test accessors + `debug_validate_invariants` |
| `region.rs` + `region/` | 定长 `Region<T>` chunk 分配器（对象/数组）+ TLAB borrow/retire/reclaim；`claim.rs`（`ChunkClaim`：TLAB 的无锁填充）、`footprint.rs`（region 的占用记账：chunk 常量、挂接、侧表重量）、`slab.rs`（chunk 从页对齐 slab 里切）、`decommit.rs`（池中空 chunk 交还物理页 + generation 下限）、`entry.rs`（`RegionEntry`：值 + GC 元数据）、`generation.rs`（年轻代记账 / 晋升 / 卡表）、`invariants.rs`（debug 不变量） |
| `var_region.rs` + `var_region/` | 变长 `VarRegion` 字节 bump 分配器（字符串/闭包）：`block.rs`（`GcBlockHeader` 16 B 头 + payload）、`chunk.rs`（尺寸类 + `VarChunkClaim`）、`var_ref.rs`（`VarGcRef` 8 字节 tagged 句柄）、`generation.rs`（young list） |
| `satb.rs` | **SATB 删除屏障**：进程级 `MARKING_HEAPS` 快路径 + 线程本地记录缓冲，`retire_thread_tlab` 时交给堆 |
| `tlab.rs` | thread-local `Tlab{obj,arr,var}` + arm 门（仅 VmContext 线程走零锁 TLAB）。机制见 [book: GC TLAB](../../../../docs/internals/src/runtime/gc-tlab.md) |
| `safepoint.rs` | GC safepoint 协议 |
| `ambient.rs` | thread-local 当前 VM 堆指针，供无堆参数的调用点分配 GC block |
| `refs.rs` | `GcRef<T>` / `WeakGcRef<T>` 不透明句柄（8B 标记指针：低 48 位 `RegionEntry` 地址 + 高 16 位 generation 快照；`Clone` 无 atomic，`Drop` no-op，finalizer 在 sweep 触发） |
| `types.rs` | 支持类型 —— `RootHandle` / `FrameMark` / `GcEvent` / `GcObserver` / `WeakRef` / `HeapSnapshot` / `HeapStats` / `FinalizerFn` / `AllocSamplerFn` / ... |
| `retention.rs` / `snapshot.rs` | 堆保留诊断（反向引用图 + `whyRetained`）/ V8 `.heapsnapshot` 导出 |
| `soft_registry.rs` | soft-reference 注册表（堆压力下可被清除的引用） |
| `sampler.rs` / `phase_timer.rs` / `trace.rs` | safepoint 采样 profiler / `Z42_GC_PHASES` 分阶段停顿计时 / `Z42_GC_TRACE` 每次 collect 的 stderr trace |
| `heap_tests.rs` / `arc_heap_tests/` / `*_tests.rs` | trait 默认方法契约测试 / `ArcMagrGC` 行为单测（分配 / 收集 / 标记队列 / 分代 / 增量 / finalizer / 不变量等）/ 各模块单测 |

## 入口点

- `crate::gc::MagrGC` —— GC 接口 trait（11 能力组）
- `crate::gc::ArcMagrGC` —— 默认实现（`GcRef` 指向 `Region<T>` 内的 `RegionEntry`；默认 `GenerationalMarkSweep`）
- `crate::gc::GcRef<T>` / `crate::gc::WeakGcRef<T>` —— 堆引用不透明句柄；backing 切换（如 MMTk）零 callsite 修改
- 嵌入相关类型：`RootHandle` / `FrameMark` / `GcEvent` / `GcObserver` / `AllocSample` / `WeakRef` / `HeapSnapshot` / `HeapStats` / ...

### 能力组（按 trait 内分组）

| # | 能力组 | 主要方法 |
|---|--------|---------|
| 1 | Allocation | `alloc_object` / `alloc_array` |
| 2 | Roots | `pin_root` / `unpin_root` / `enter_frame` / `leave_frame` / `for_each_root` |
| 3 | Write barriers | `write_barrier_field` / `write_barrier_array_elem`（默认 no-op，generational / 自定义堆 backend 时可重载）；`verify_card_invariant`（核对卡表不变量）|
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
`src/libraries/z42.core/src/GC/GC.z42`）。

## 依赖关系

- 上游：`metadata::{Value, ScriptObject, TypeDesc, NativeData}`
- 下游：`vm_context::VmContext` 持有 `Box<dyn MagrGC>` + 注入 external root
  scanner 闭包（扫描 static_fields / pending_exception / interp+JIT exec_stack）；
  `interp/` 与 `jit/helpers/` 通过 `ctx.heap()` 调用

## 如何测试验证

```bash
(cd src/runtime && cargo test --lib gc)     # GC 单测（含 arc_heap_tests/）
./xtask test e2e --dir gc                   # GC golden 端到端
```

## 关联文档

设计与机制见 [`docs/internals/src/runtime/gc.md`](../../../../docs/internals/src/runtime/gc.md)（含「GC 后续迭代规划」），
句柄见 [`gc-handle.md`](../../../../docs/internals/src/runtime/gc-handle.md)。
两种 `GcMode`（分代 / STW）均可用，默认分代；每次停顿都是 STW，分代模式的 major 默认拆成增量切片。

## 命名

**MagrGC** 取自《银河系漫游指南》中的 **Magrathea** —— 那颗专门建造定制行星的传奇世界。
