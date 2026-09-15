# Proposal: 增量 major 标记 —— GC 最大停顿与堆大小脱钩

## Why

分代 GC 是默认模式，但 **major 停顿随堆线性增长**：`z42c.semantics --release --no-incremental`（16M nursery）上
最大一次 major 34.6 ms（reset marks 6.2 / full mark 18.8 / sweep 6.3 / age survivors 3.2），4M nursery 下 50.4 ms；
full mark ~24 ns/存活条目、reset 与 sweep 按全部条目计 ⇒ 1 GB 堆会到上百 ms。**所有 max 停顿都来自 major**
（minor 在 16M 下 ≤ ~11 ms）。User 裁决：「最大停顿 40 ms 不可接受，不能用于生产」。

局部修剪（去 reset、合并 age survivors）只能到 ~25 ms，仍随堆增长。现有 `Z42_GC_MODE=concurrent`
不能拿来用：非分代；Dijkstra 插入式屏障但 remark 不重扫 roots（「从未扫描对象读出白对象进寄存器、再清字段」即漏标）；
sweep 仍 STW。

## What Changes

目标（User 2026-09-15 裁决）：**第一里程碑 max 停顿 ≤ 10 ms 且与堆大小无关，其后 ≤ 5 ms**；屏障选 **SATB**；
路线 **先增量切片，后并发**。本 change 覆盖 M0–M2，M3（minor 停顿上界）/ M4（切片挪后台线程）各自另开 change。

- **M0 度量与门禁**：新 perf scenario `13_gc_large_heap`（数百 MB 活对象 + 老年代流失，使 major 真正有活干），
  scenario 经 `Std.GC.PauseStatsRaw()` 自报 max / p99 停顿；`xtask bench` 采集为新指标并进 bench 门禁。
- **M1 停顿内修剪（不改屏障）**：
  - major 标记改 **epoch**：`marked` 字节 = bit0 minor 标记 + 高 7 位 major epoch；「已标记」⇔ epoch == 当前周期。
    **删除 `reset marks` 全堆遍历**（换代即全白，不存在陈旧 major 标记）。
  - `age survivors` 并入 major sweep 同一趟。
- **M2 增量 major 标记（SATB）**：
  - major 周期拆成有界 STW 切片（默认预算 2 ms，旋钮 `Z42_GC_SLICE_MS`）：snapshot 切片（灰化 roots）→ 若干 mark 切片
    → final 切片（排空 SATB 缓冲，**不重扫 roots**）→ sweep 切片（按 chunk 分片）。切片复用现有 safepoint / `request_gc_pause`。
  - **SATB 删除屏障下沉到写原语**（`ScriptObject::set_field_value` / `ArrayObj::set_boxed` / `write_struct_elem` 及少数
    `refs_mut` 直写点）：标记期覆盖前取旧值，未标记的堆引用进线程本地 SATB 缓冲；非标记期一次 relaxed load + 分支。
  - 标记与清扫期 **allocate-black**（出生即写当前 epoch），覆盖 TLAB 与加锁两条路径的三个 region。
  - **切片之间允许 minor**：灰队列与各线程 SATB 缓冲作为 minor 的额外根；minor 只动 bit0，不碰 epoch。
  - 分配驱动的节奏（每分配 X MB 至少标记 Y 条），堆到硬上限时退化为一次性完成（保正确，放弃停顿目标）。
- 知识库：新页 `docs/book/src/runtime/gc-incremental-major.md`；`gc-tuning-and-safepoint.md` 旋钮表。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/tests/perf/scenarios/13_gc_large_heap.z42` | NEW | M0：大堆 + 老年代流失 scenario，自报停顿 |
| `scripts/xtask_bench.z42` | MODIFY | M0：采集 scenario 自报的 max / p99 停顿为指标 |
| `src/tests/perf/baseline-schema.json` | MODIFY | M0：`metric` 枚举加 `pause` |
| `.github/workflows/bench-pr.yml` | MODIFY | M0：停顿指标进门禁（阈值见 design） |
| `src/runtime/src/gc/region/entry.rs` | MODIFY | M1：`marked` 字节拆 minor bit / major epoch |
| `src/runtime/src/gc/var_region/block.rs` | MODIFY | M1：同上（var 块头） |
| `src/runtime/src/gc/refs.rs` | MODIFY | M1：`GcRef` 标记 API 分 minor / major |
| `src/runtime/src/gc/var_region/var_ref.rs` | MODIFY | M1：`VarGcRef` 标记 API 分 minor / major |
| `src/runtime/src/gc/arc_heap/collect.rs` | MODIFY | M1：`mark_if_unmarked` major 版；删 reset 遍历 |
| `src/runtime/src/gc/arc_heap/control.rs` | MODIFY | M1：STW 周期换 epoch；M2：生产路径改走增量周期 |
| `src/runtime/src/gc/arc_heap/generational.rs` | MODIFY | M1：age survivors 并入 sweep；M2：minor 读 SATB/灰队列根 |
| `src/runtime/src/gc/region.rs` | MODIFY | M1：sweep 按 epoch 判活 + 顺手升龄；M2：按 chunk 分片清扫 |
| `src/runtime/src/gc/region/generation.rs` | MODIFY | M1：`age_young_survivors` 并入 sweep |
| `src/runtime/src/gc/var_region.rs` | MODIFY | M1/M2：同 region.rs |
| `src/runtime/src/gc/var_region/chunk.rs` | MODIFY | M2：var 分片清扫游标 |
| `src/runtime/src/gc/arc_heap/alloc_black.rs` | MODIFY | M1：出生写 epoch；M2：窗口覆盖标记 + 清扫期 |
| `src/runtime/src/gc/arc_heap/alloc.rs` | MODIFY | M2：allocate-black 三个 chokepoint 改写 epoch（行数棘轮基线 601，净增为 0） |
| `src/runtime/src/gc/incremental.rs` | NEW | M2：周期状态机、切片调度、节奏控制 |
| `src/runtime/src/gc/satb.rs` | NEW | M2：`MARKING_ACTIVE`、线程本地 SATB 缓冲、flush |
| `src/runtime/src/gc/mod.rs` | MODIFY | M2：注册新模块 |
| `src/runtime/src/gc/safepoint.rs` | MODIFY | M2：slow path 挂切片执行 |
| `src/runtime/src/gc/arc_heap/auto_collect.rs` | MODIFY | M2：major trip 启动增量周期而非 STW major |
| `src/runtime/src/gc/arc_heap/barrier.rs` | MODIFY | M2：卡表屏障保留；SATB 不走这里（见 design D2） |
| `src/runtime/src/gc/arc_heap/interface.rs` | MODIFY | M2：`force_collect` / 显式 `GC.Collect` 走同步完成路径；`upgrade_weak` / `soft_ref_get` 读取染色 |
| `src/runtime/src/gc/arc_heap.rs` | MODIFY | M2：GC 句柄 `target()` 读取染色（弱句柄）；持有 `IncrementalCycle` |
| `src/runtime/src/gc/soft_registry.rs` | MODIFY | M2：软引用读取染色 |
| `src/runtime/src/metadata/types/object.rs` | MODIFY | M2：`set_field_value` 内置 SATB |
| `src/runtime/src/metadata/types/array_access.rs` | MODIFY | M2：`set_boxed` / `write_struct_elem` 内置 SATB |
| `src/runtime/src/metadata/types/obj_storage.rs` | MODIFY | M2：`refs_mut` 收窄为 crate 内审计过的调用点 |
| `src/runtime/src/interp/exec_object.rs` | MODIFY | M2：`refs_mut` 直写点改走原语 |
| `src/runtime/src/jit/helpers/object_field.rs` | MODIFY | M2：同上 |
| `src/runtime/src/corelib/reflection/accessors.rs` | MODIFY | M2：同上 |
| `src/runtime/src/vm_context/types.rs` | MODIFY | M2：VmContext 持 SATB 缓冲（线程退出时 flush） |
| `src/runtime/src/config.rs` | MODIFY | M2：`gc_slice_ms` |
| `src/runtime/src/config/knob_table.rs` | MODIFY | M2：`Z42_GC_SLICE_MS` |
| `src/runtime/src/gc/arc_heap_tests/incremental.rs` | NEW | M1/M2：确定性单测（含漏标阴性对照） |
| `src/runtime/src/gc/arc_heap_tests/mod.rs` | MODIFY | 注册 |
| `src/runtime/tests/gc_satb_loom.rs` | NEW | M2：loom 模型 D（写屏障 × 切片 × minor） |
| `docs/book/src/runtime/gc-incremental-major.md` | NEW | 机制页 |
| `docs/book/src/runtime/gc-tuning-and-safepoint.md` | MODIFY | 旋钮表 + 停顿模型 |
| `docs/book/src/SUMMARY.md` | MODIFY | 挂新页 |

**只读引用**：`src/runtime/src/gc/arc_heap/roots.rs`（root 枚举）、`src/runtime/src/gc/tlab.rs`（TLAB retire 时序）、
`src/runtime/tests/gc_registration_race_loom.rs` / `gc_alloc_black_loom.rs`（既有模型，作门禁）、
`docs/spec/changes/investigate-concurrent-gc-stale-mark-race/design.md`（三条禁区）。

> M2 的「写原语下沉」审计若发现 Scope 外的直写点，**停下回阶段 3 补 Scope**。

## Out of Scope

- M3：minor 停顿上界（按停顿目标自适应 nursery）—— 另开 change。
- M4：切片挪到后台线程（真并发标记 / 清扫）—— 另开 change，复用本 change 的屏障与不变量。
- 删除旧 `Z42_GC_MODE=concurrent` —— M2 落地后另开 change（pre-1.0 直接删）。
- 空洞复用 / 对象载荷内联等内存项（见 memory「推进内存总量」）。
- 移动式 / 压缩式回收。

## Open Questions

- [x] M0 门禁阈值 —— **User 2026-09-15 同意：绝对上限，M0 起 16 ms，M2 落地后收紧到 10 ms**
- [x] `GC.Collect()` / `ForceCollect()` 语义 —— **User 2026-09-15 同意：保持同步完成**
