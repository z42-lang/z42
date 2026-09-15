# Tasks: 增量 major 标记 —— GC 最大停顿与堆大小脱钩

> 状态：🟡 实施中（User 2026-09-15 批准）| 创建：2026-09-15
> 变更类型：`vm`（GC 运行时行为；不改语言 / IR / zbc 格式）
> 裁决（User 2026-09-15）：目标先 ≤ 10 ms 后 ≤ 5 ms；屏障 SATB；先增量切片后并发。
> 每个里程碑单独一个 PR（本仓 squash merge、不叠 PR），各自带实测。

## 进度概览
- [x] M0: 度量与门禁（大堆 scenario + 停顿指标进 bench）
- [x] M1: 停顿内修剪（epoch 标记删 reset marks）—— age 并入 sweep 挪 M2b、1.10 挪 M3（见各条）
- [ ] M2a: SATB 屏障 + allocate-black epoch 化（周期仍一次性完成，只验证屏障与不变量）
- [ ] M2b: 切片调度（Snapshot / Mark / Final / Sweep 分片 + 节奏 + 退化）
- [ ] M2c: 验收（停顿目标、正确性配方、loom、文档）

## M0: 度量与门禁
- [x] 0.1 `src/tests/perf/scenarios/13_gc_large_heap.z42`：5 万条链 × 8 节点（带 `long[8]`）活堆 + 随机整链替换（老年代流失）
      + 临时分配（年轻代流失），churn = slots×8；参数 `large` 活堆 ×3；末行 `gc-pause max_us=… p99_us=… count=…`；
      校验和 25919994799997 / 233279984399994（interp 与 jit 一致）
- [x] 0.2 `scripts/common/xtask_bench_pause.z42`（NEW，`xtask_bench.z42` 已超 886 行）+ `xtask_bench.z42` 接线 + `xtask_cli_bench.z42` 两个选项：
      `// gc-pause: report` 场景另跑 3 次取中位数，单跑路径产出 `<name>-pause-max/-p99`，A/B 路径写 `metric: pause` 并判定；
      `--ab-selftest` 加 4 例（14/14），阴性对照（判定恒 false ⇒ 2 例红）已做
- [x] 0.3 `src/tests/perf/baseline-schema.json`：`metric` 加 `pause`
- [x] 0.4 `.github/workflows/bench-pr.yml`：`--pause-cap-ms 16 --threshold-pause 0.25`。
      ⚠️ **事实校正**：原定「绝对上限 16 ms」直接上会让 M2 前每个 PR 都红（main 上本场景 ~97 ms），
      故判红 = **超上限且比 base 差 25%**；M2 后 base/pr 都在上限内即退化为纯绝对上限，届时 cap 收紧到 10
- [x] 0.5 本地基线（main 47e43815 / 14925b02，机器有他会话负载）：

      | 负载 | max | p99 | 总停顿 | RSS |
      |---|---|---|---|---|
      | semantics 16M（×3） | 32.6~34.6 ms | 11.6~12.7 ms | 224~232 ms | 594 MB |
      | semantics 4M（×2） | 52.1~53.4 ms | 29.1~29.3 ms | 432 ms | 611 MB |
      | `13_gc_large_heap`（jit） | **97.5 ms** | = max（30 次回收） | — | 876 MB |
      | `13_gc_large_heap large`（jit） | **391.7 ms** | = max（65 次） | — | 2.9 GB |

      本场景 major ≈ reset 11.6 / full mark 35.6（800k 条）/ sweep 17.6 / **age survivors 25.2**；
      **minor 也有 25~27 ms**（M3 的输入）；最大那次 99 ms 是**徒劳退避把 gate 乘到 ×16（272 MB nursery）**的 minor（见 1.10）
- [x] 0.6 GREEN（`xtask test` 全绿，基于 main `14925b02`）+ 本地 A/A `bench --ab --tier gate --mode jit`：9 条对比零假红，停顿子门禁 base 87.9 / pr 89.3 ms → ok

## M1: 停顿内修剪（不改屏障）
- [x] 1.1 `gc/region/entry.rs` + `gc/var_region/block.rs`：`marked` 拆 bit0 minor / bits1..=7 major epoch（统一编码在 `gc/refs.rs` 的 `MarkKind` + `mark_cell` 系列）
- [x] 1.2 `gc/refs.rs` + `gc/var_region/var_ref.rs` + `metadata/vstr.rs` + `metadata/types/array_access.rs`：`mark(kind)` / `is_marked(kind)` / `clear_minor_mark`；`trace_children(kind)` / `visit_gc_children(Option<MarkKind>)`
- [x] 1.3 `collect.rs` + `control.rs`：epoch **按堆、初值 1**，`begin_major_mark` 只在周期入口；**删除 `reset_all_marks_in_regions`**
      （初值 0 读作 1 会与首周期撞：`stress_seeded_concurrent_short` 抓到并发屏障首次回收前的标记被当成已标记）
- [x] 1.4 `generational.rs`：minor 用 `Minor`；**穿透老对象 / `refers_to_young` 用不标记的遍历**（原先会给老数组 backing 置位，一直靠 reset 擦）
- [x] 1.5 major sweep 按 epoch 判活，存活者清 minor 位兜底。**「同一趟升龄」挪到 M2b**：稳态 age survivors 只 ~4 ms，M2b 要把 sweep 重写成切片，不做两遍
- [x] 1.6 `alloc_black.rs`：allocate-black 用当前 epoch
- [x] 1.7 `debug.rs` 校验器：alive ⇒ minor 位清 且 epoch ∈ {当前, 0}（新增陈旧 minor 位 / 当前 epoch 不算陈旧两测）
- [x] 1.8 单测：minor 位与 epoch 互不干扰、epoch 序列跳 0 且回绕；5 个编码「sweep 清位」旧语义的测试改为新不变量
- [x] 1.9 实测（交错 ×3，产物逐字节一致）：semantics max 36~39 → **28~32 ms（未达 ≤ 25 ms 目标）**、总停顿约 −6%；
      `13_gc_large_heap` reset 161 ms → 0 但 full mark +85 ms（旧 reset 在替标记预热 cache），总 −4%，max 不变 ~90 ms。GREEN 全绿（main `6d57a694`）
- [x] 1.10 **评估后否决现形态、挪到 M3**：去掉「退避放大 minor gate」后 `13_gc_large_heap` max 90 → 70 ms，但 `09_alloc_ctorless` 墙钟
      **+70%~160%**（回收 3 → 9 次、总停顿 179 → 507~542 ms、max 反升到 150+）。要的是按停顿预算自适应 nursery，不是删退避

## M2a: SATB 屏障（周期仍一次性完成）
- [ ] 2.1 `gc/satb.rs`（NEW）+ `gc/mod.rs`：`MARKING_ACTIVE`、`remember`、线程本地缓冲 + 全局兜底队列、flush
- [ ] 2.2 `vm_context/types.rs`：VmContext 持 SATB 缓冲；Drop 时 flush
- [ ] 2.3 `metadata/types/object.rs` / `array_access.rs`：`set_field_value` / `set_boxed` / `write_struct_elem` 内置 SATB
- [ ] 2.4 `metadata/types/obj_storage.rs`：`refs_mut` 收窄；审计并改掉 `interp/exec_object.rs`、`jit/helpers/object_field.rs`、
      `corelib/reflection/accessors.rs` 的直写；sweep 断边改用内部无屏障写。**发现 Scope 外直写点 → 停下补 Scope**
- [ ] 2.5 弱 / 软引用读取染色：`gc/arc_heap.rs` 句柄 `target()`、`interface.rs` `upgrade_weak` / `soft_ref_get`、`gc/soft_registry.rs`
- [ ] 2.6 minor 把灰队列与各线程 SATB 缓冲当额外根（`generational.rs`）
- [ ] 2.7 测试钩子：可在单测里手工驱动 Snapshot → 任意 mutator 操作 → 完成周期
- [ ] 2.8 单测（`arc_heap_tests/incremental.rs`）：漏标阴性对照（关 SATB ⇒ 被回收；开 ⇒ 存活）、allocate-black 两期、弱引用复活、SATB 年轻对象活过 minor
- [ ] 2.9 实测：非标记期指令回归 ≤ 1%（semantics + `09_alloc_ctorless`）；GREEN

## M2b: 切片调度
- [ ] 3.1 `gc/incremental.rs`（NEW）：`IncrementalCycle` 状态机（Idle / Snapshot / Mark / Final / Sweep）、epoch、sweep 游标
- [ ] 3.2 `gc/arc_heap/auto_collect.rs`：major trip 启动周期；minor 之后调度切片；落后时连续执行
- [ ] 3.3 `gc/safepoint.rs`：slow path 执行待办切片（复用 `request_gc_pause`）
- [ ] 3.4 `gc/region.rs` / `gc/var_region.rs` / `gc/var_region/chunk.rs`：按 chunk 分片清扫，清扫中 chunk 不借出
- [ ] 3.5 `gc/arc_heap/interface.rs` / `control.rs`：`GC.Collect` / `ForceCollect` / 软上限 / OOM ⇒ 同步完成当前周期
- [ ] 3.6 `config.rs` + `config/knob_table.rs`：`Z42_GC_SLICE_MS`（默认 2）、`Z42_GC_INCREMENTAL`（默认开，0 = 一次性 major）
- [ ] 3.7 `Z42_GC_PHASES`：每切片一行 + 周期汇总行
- [ ] 3.8 `tests/gc_satb_loom.rs`（NEW）：模型 D 穷举；既有 A/B/C 模型保持绿

## M2c: 验收
- [ ] 4.1 性能验收表（design「Testing Strategy」）：semantics 与 `13_gc_large_heap` 两档 max ≤ 10 ms；总停顿 ≤ +20%；墙钟 ≤ +2%；RSS ≤ +5%
- [ ] 4.2 正确性配方：冷 `package sdk`（默认 / 4M nursery）、`build stdlib`、`Z42_GC_SLICE_MS=0.05` 全套 stdlib 测试、`http_server_threaded` ×60
- [ ] 4.3 `xtask test` GREEN（含自举不动点）
- [ ] 4.4 文档：`docs/book/src/runtime/gc-incremental-major.md`（NEW）、`gc-tuning-and-safepoint.md`、`docs/book/src/SUMMARY.md`
- [ ] 4.5 归档本 change；memory 更新

## 备注
- 2026-09-16：本地复测一度以为 M1 让 `cargo test --lib` 慢 10× 且间歇失败 —— 查清与本 change 无关：
  慢是环境（已供种 worktree 里 `VmContext::new` 会加载真 stdlib）；间歇失败是 main 既有的 **`config_tests::with_env`
  改真实环境时抢先初始化了进程级 `runtime_config()`，把整个测试进程的默认 GC 模式定成 concurrent**，已单独修（fix-config-tests-leak-gc-mode）。
- 行数棘轮：`gc/arc_heap/alloc.rs` 在基线 601 上，改动须净增 0 行（说明性注释放新模块）。
- 教训 49/50/51：每个里程碑的收益用两个二进制交错实测，并同时在 4M / 8M nursery 下验证（major 落点敏感）。
