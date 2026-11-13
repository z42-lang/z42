# gc/

## 职责

z42 VM 的 GC 子系统：堆对象（对象 / 数组 / 字符串 / 闭包）的分配、引用追踪与
回收能力 —— 通过具体 `Heap` 提供嵌入接口（pin roots /
observers / allocation sampling / weak refs / finalizers / strict OOM / ...）。

## 核心文件

| 文件 | 职责 |
|------|------|
| `config.rs` | `GcConfig`：显式传入堆的不可变策略值，GC 不解析全局运行时设置 |
| `to_vm.rs` | `GcRoots` / `GcToVm` / `GcPause`，以及不透明的上下文回收快照；VM 提供实现 |
| `heap.rs` | 具体 `Heap` 的状态、身份、句柄表与 concern 声明 |
| `mode.rs` | `GcMode`：`GenerationalMarkSweep`（默认）/ `StwMarkSweep`，`Z42_GC_MODE` 选择 |
| `footprint.rs` | `Footprint`：堆的真实占用（committed / pooled / occupied），三个 region 共用一份、随变随记；`malloc_size` 分配器取整模型。机制见 [book: GC 调参 · 真实占用记账](../../../../docs/internals/src/runtime/gc-tuning.md#真实占用记账与软上限) |
| `heap/construct.rs` | `new(GcConfig)` 构造；`Default` 只使用纯默认策略 |
| `heap/alloc.rs` | 分配尾部（TLAB / 锁路径 `acquire_var_block`、数组漏斗 `alloc_array_block`：对象模型 R9，数组 = 一个块，在块里建好再发布）+ OOM 兜底 + 内存压力检查 + size 查询（`object_size_bytes`）|
| `heap/alloc_object.rs` | 对象分配（对象模型 R6）：对象 = 一个 `BlockType::Object` 变长块，在块里建好（对象头 + 泛型一步）再发布 |
| `heap/alloc_black.rs` | 增量 major 周期内的 allocate-black |
| `heap/auto_collect.rs` | 自动 collect 触发策略（分配压力） |
| `heap/collect.rs` | mark-sweep 原语：mark/sweep 阶段 + soft-ref 复活 + live 快照 |
| `heap/control.rs` | 环回收编排与控制 API：`run_cycle_collection(_stw)` + `collect_cycles`/`force_collect`/`collect_full` + finalize + soft-ref |
| `heap/generational.rs` / `barrier.rs` | 分代 GC：minor/major/promotion + `gen_age`；分代写屏障（老属主进记忆集） |
| `heap/remembered.rs` | 记忆集（对象模型 R6 / R9）：变长区没有卡表，老对象 / 老数组收到年轻引用 / 带着年轻引用晋升时置块头 `REMEMBERED_BIT` 入集，minor 从它出根、排空。见 [book: GC](../../../../docs/internals/src/runtime/gc.md)「记忆集」 |
| `heap/side_tables.rs` | 对象与数组的侧表（对象模型 R6 / R9）：弱引用（带 chunk 复用 epoch）、finalizer、软引用按块头地址记账；`process_flagged_dead` 在 chunk 回收前处理死掉的带标记块 |
| `heap/card_verify.rs` | 记忆集不变量的从头核对（`verify_card_invariant`；`Z42_GC_VERIFY_CARDS` 每次 minor 前跑）——抓漏掉的写屏障。见 [book: GC · 记忆集不变量核对](../../../../docs/internals/src/runtime/gc.md#记忆集不变量核对z42_gc_verify_cards) |
| `heap/promotion_policy.rs` / `pause_budget.rs` | 自适应晋升判定 / 停顿预算化 nursery |
| `heap/young_policy.rs` | 年轻代策略：按回收收益（对比上一次 major）判定 minor，不划算时把 minor 换成 **tenure**（年轻代整体晋升、不标记）。机制见 [book: GC 调参 · 年轻代策略](../../../../docs/internals/src/runtime/gc-tuning.md#年轻代策略按收益判定-minor不划算就-tenure) |
| `heap/roots.rs` | 根快照、marked-context 扫描与锁内基础遍历 |
| `heap/observe.rs` | 观测：barrier observer(test) + 事件分发 + pause 计时 + stats |
| `heap/footprint.rs` | 真实占用的堆侧：region 的 payload 度量函数、`committed` / `occupied` 读数、软上限的两种单位（`SoftCap`）、sweep 尾部的 chunk 回收 + 侧表重量 |
| `heap/incremental.rs` | **增量 major**：切片状态机（Marking / Sweeping）、切片预算、pacer、`admit_resurrected`（弱读 / 堆遍历不交出待清扫的死对象）。机制见 [book: 增量 major](../../../../docs/internals/src/runtime/gc-incremental-major.md) |
| `heap/api.rs` | 公开固有方法；同名转发直接公开原有 concern 实现|
| `heap/debug.rs` | `#[cfg(test)]`/`#[cfg(debug_assertions)]` 辅助：test accessors + `debug_validate_invariants` |
| `var_region.rs` + `var_region/` | `VarRegion` 分配器：`chunk.rs` 的尺寸类、认领与复用，`generation.rs` 的年轻块集合，`holes.rs` 的 TLAB 空洞批量复用，`objects.rs` 的弱句柄校验、块遍历及清扫报告；区块头和句柄形状由 `object/header` 提供 |
| `side_bits.rs` | 字数组位集（置位 / 清位 / 逐位遍历），region 的年轻集合位图用 |
| `satb.rs` | **SATB 删除屏障**：进程级 `MARKING_HEAPS` 快路径 + 线程本地记录缓冲，`retire_thread_tlab` 时交给堆 |
| `tlab.rs` | thread-local `Tlab{var,holes}` + arm 门（仅 VmContext 线程走零锁 TLAB）。机制见 [book: GC TLAB](../../../../docs/internals/src/runtime/gc-tlab.md) |
| `types.rs` | 支持类型 —— `RootHandle` / `FrameMark` / `GcEvent` / `GcObserver` / `WeakRef` / `HeapStats` / `FinalizerFn` / `AllocSamplerFn` / ... |
| `walk.rs` | 存活对象、原始对象/数组和分类根遍历；工具实现位于 `profiler/` |
| `soft_registry.rs` | soft-reference 注册表（堆压力下可被清除的引用） |
| `phase_timer.rs` / `trace.rs` | `Z42_GC_PHASES` 分阶段停顿计时 / `Z42_GC_TRACE` 每次 collect 的 stderr trace |
| `heap_api_tests.rs` / `heap_tests/` / `*_tests.rs` | 公开堆 API 契约测试 / `Heap` 行为单测（分配 / 收集 / 标记队列 / 分代 / 增量 / finalizer / 不变量等）/ 各模块单测 |

对象头、标记位与强/弱句柄定义在 [`object/header`](../object/header)，GC 负责其分配、回收与存活判定。

## 入口点

- `crate::gc::Heap` —— 具体堆的分配、根、屏障、回收、引用、观测与统计接口
- `crate::object::ObjRef` / `crate::object::WeakObjRef` —— 对象句柄；`crate::object::ArrRef` / `crate::object::WeakArrRef` —— 数组句柄
- 嵌入相关类型：`RootHandle` / `FrameMark` / `GcEvent` / `GcObserver` / `AllocSample` / `WeakRef` / `HeapStats` / ...

### 能力组

| # | 能力组 | 主要方法 |
|---|--------|---------|
| 1 | Allocation | `alloc_object` / `alloc_array` / `alloc_array_with`（数组漏斗） |
| 2 | Roots | `pin_root` / `unpin_root` / `enter_frame` / `leave_frame` / `for_each_root` |
| 3 | Write barriers | `write_barrier_field` / `write_barrier_array_elem`（STW 模式不记忆，分代模式记录老到年轻引用）；`verify_card_invariant`（核对记忆集不变量）|
| 4 | Object Model | `object_size_bytes` / `scan_object_refs` |
| 5 | Collection | `collect_cycles` / `force_collect` / `collect_full` / `pause` / `resume` |
| 6 | Heap config | `set_max_heap_bytes` / `used_bytes` / `set_strict_oom` |
| 7 | Finalization | `register_finalizer` / `cancel_finalizer` |
| 8 | Weak refs | `make_weak` / `upgrade_weak` |
| 9 | Observers | `add_observer` / `remove_observer` |
| 10 | Tool hooks | `set_alloc_sampler` / `iterate_live_objects` |
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
let snap = crate::profiler::heap::take_snapshot(ctx.heap());
```

z42 脚本端可调 `Std.GC.Collect()` / `UsedBytes()` / `ForceCollect()`（见
`src/libraries/z42.core/src/GC/GC.z42`）。

### 构造与配置

`Heap::new(config)` 显式接收 `GcConfig`，每个堆保存自己的配置快照。
`Heap::default()` 使用纯默认值，不读取环境或进程设置。VM 入口由门面
`RuntimeConfig::gc_config` 转换已解析设置，继续应用原有配置优先级；独立基准同样显式转发。
软引用阈值、阶段计时、nursery 策略与停顿窗口均取自当前堆。safepoint 的轮询节流由 `threads` 的线程协议读取。
`collect_cycles_with_vm` 通过 `GcToVm` 申请暂停，GC 不持有线程上下文。根访问须处于暂停内，
暂停服务的实现必须满足 `GcToVm` 的安全契约；丢弃令牌后恢复线程。加载上下文身份由 VM 回调解析，
GC 只在标记后观察活对象，清扫结束后交回本轮不透明快照。

## 依赖关系

- 上游：`object::{Value, ScriptObject, OwnedObject, TypeDesc, NativeData}`
- 下游：`context::VmCore` 持有地址稳定的 `Box<Heap>` + 注入 external root
  scanner 闭包（扫描 static_fields / pending_exception / interp+JIT exec_stack）；
  `interp/` 与 `jit/helpers/` 通过 `ctx.heap()` 调用

## 如何测试验证

```bash
(cd src/runtime && cargo test --lib gc)     # GC 单测（含 heap_tests/）
./xtask test e2e --dir gc                   # GC golden 端到端
```

## 关联文档

设计与机制见 [`docs/internals/src/runtime/gc.md`](../../../../docs/internals/src/runtime/gc.md)（含「GC 后续迭代规划」），
句柄见 [`gc-handle.md`](../../../../docs/internals/src/runtime/gc-handle.md)。
两种 `GcMode`（分代 / STW）均可用，默认分代；每次停顿都是 STW，分代模式的 major 默认拆成增量切片。

堆摘要、V8 导出与保留链见 [`profiler/`](../../crates/z42-profiler/README.md)。
