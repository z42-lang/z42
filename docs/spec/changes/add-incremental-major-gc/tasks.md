# Tasks: 增量 major 标记 —— GC 最大停顿与堆大小脱钩

> 状态：🟡 实施中（User 2026-09-15 批准）| 创建：2026-09-15
> 变更类型：`vm`（GC 运行时行为；不改语言 / IR / zbc 格式）
> 裁决（User 2026-09-15）：目标先 ≤ 10 ms 后 ≤ 5 ms；屏障 SATB；先增量切片后并发。
> 每个里程碑单独一个 PR（本仓 squash merge、不叠 PR），各自带实测。

## 进度概览
- [x] M0: 度量与门禁（大堆 scenario + 停顿指标进 bench）
- [x] M1: 停顿内修剪（epoch 标记删 reset marks）—— age 并入 sweep 挪 M2b、1.10 挪 M3（见各条）
- [x] M2a: SATB 屏障（周期仍一次性完成，只验证屏障与不变量）—— allocate-black epoch 化已在 M1 完成
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
- [x] 2.1 `gc/satb.rs`（NEW）+ `gc/mod.rs`：进程级 `MARKING_HEAPS`（热路径只读它）+ `MARKING_GEN` + `(heap, epoch)` 表；
      **记录进线程本地缓冲、按线程绑定的堆归属**（一个进程多个堆时，进程级队列会让 A 堆用自己的 epoch 染 B 堆的对象）；
      线程在 `retire_thread_tlab` 时把缓冲交给堆的 `satb_queue`（每条 park 路径都先 retire）。无「全局兜底队列」——没有绑定堆的线程不可能写堆引用
- [x] 2.2 `vm_context/construct.rs`（**不是** `types.rs`：缓冲放线程本地，VmContext 只负责绑定）：`new*` → `bind_thread`（栈式，嵌套 context LIFO 还原）；Drop → retire 后 `unbind_thread`
- [x] 2.3 `object.rs`：`set_field_value`（含 byte 内联引用：先 `read_inline_ref` 取旧值）+ 新 `set_ref_slot`；`array_access.rs`：`set_boxed`（Boxed 与 StructBytes 引用叶子）、`write_struct_elem`、新 `set_struct_ref`、
      **`copy_elems_from` 的 Boxed→Boxed 批量快路径**（`Array.Copy`：一次 `clone_from_slice`，先整段 `record_overwrite_all`）
- [x] 2.4 `obj_storage.rs`：`refs_mut` 改名 `refs_mut_raw`（只给新分配对象 + GC 断边）；`struct_refs_mut` 删除（改 `set_struct_ref`）；
      直写点实际在 `interp/exec_struct.rs`、`corelib/reflection/accessors.rs`（改走原语）与 `corelib/convert.rs`（新分配，保留 raw）；
      `interp/exec_object.rs` / `jit/helpers/object_field.rs` 经审计已经走 `set_field_value`，无需改。补 Scope 见 proposal
- [x] 2.5 弱 / 软引用读取染色：`interface.rs` `upgrade_weak` / `handle_target`（弱句柄）、`control.rs` `soft_ref_get` → `shade_if_marking`（进 `satb_queue`）
- [x] 2.6 `generational.rs` `mark_phase_minor`：`satb_queue` 与 `mark_queue` 当额外根
- [x] 2.7 测试钩子：`open_major_cycle` / `snapshot_roots_into_mark_queue` / `drain_mark_queue` / `close_major_marking` / `sweep_phase` 可在单测里逐步调用；`satb::set_disabled_for_test`
- [x] 2.8 `arc_heap_tests/incremental.rs` 13 测：字段读进寄存器再清字段（开屏障存活 / **关屏障被回收**）、byte 内联字段、数组元素、**批量数组拷贝（开 / 关）**、
      标记期外不记录、多堆隔离、弱读（读 / 不读）、minor 把记录当根（有 / 无）、记录值带本周期 epoch。
      allocate-black 两期（标记期 / 清扫期出生）要真实切片才有意义 → 挪到 M2b
- [x] 2.9 实测（与只有 M1 的二进制交错）：`09_alloc_ctorless` 指令 +0.26%、`z42c.semantics` −0.03%（噪声内），编译产物逐字节一致。GREEN 见 PR

## M2b: 切片调度
- [x] 3.1 `gc/arc_heap/incremental.rs`（NEW，**不是** `gc/incremental.rs`：状态机是 `ArcMagrGC` 的一块职责，
      与其它 concern 子模块并列）：`IncrementalState`（phase 原子 + `Mutex<Cycle>` 游标 + 请求标志）、
      `SliceBudget`（native 看时钟、wasm32 与单测按工作量）。**阶段合并成 Marking / Sweeping 两个**——
      Snapshot 是开周期那一瞬（不是独立阶段），Final 是「灰队列与 SATB 队列都空」这个条件（不是独立阶段）
- [x] 3.2 `auto_collect.rs`：`TripKind{Minor,Major,Slice}`；major trip 开周期，周期中按 pacer 排切片；
      **切片不动 minor 水位、不参与徒劳退避**（否则每 1/4 nursery 一个切片会把 minor 饿死）；
      顺带修 `arm_next_collect` 被拒绝的 trip 超调（arm 在 `floor+gate` 而不是 `baseline+gate`，
      退避 ×4 的 minor 实际在 5×nursery 才触发；`09_alloc_ctorless` 的大 minor 因此多扫 20%）
- [x] 3.3 **不需要改 `safepoint.rs`**：切片走既有 `collect_cycles_with_context` 那条路（`needs_auto_collect` → 安全点 → 取暂停），
      标志位区分这次停顿做什么
- [x] 3.4 `region.rs` `sweep_chunks(major, from, max, delist_young, prepare_dead)` + `var_region.rs` `sweep_buckets`；
      **增量清扫必须边 tombstone 边摘 young 表**（一次性 major 靠其后的升龄趟摘；切片之间有 minor，
      死条目的槽一旦复用，同一个 `(chunk, entry)` 会在 young 表里出现两次 ⇒ minor 回收活对象）
- [x] 3.5 `control.rs`：`GC.Collect`（没有任何策略请求的那次调用）/ `force_collect` / 软上限 ⇒ 同步做完当前周期；
      一次性 STW / 并发入口先把打开的周期做完（换 GC 模式时才可能撞上）
- [x] 3.6 `config.rs` / `parse.rs` / `knob_table.rs`：`Z42_GC_SLICE_MS`（默认 2，clamp `[0.01,1000]`）、`Z42_GC_INCREMENTAL`（默认开）
- [x] 3.7 `Z42_GC_PHASES`：`slice/mark`、`slice/sweep`、`slice/chunk reclaim` 各一行 + 开周期行 + 周期汇总行
- [x] 3.8 **模型 D 改为穷举交错枚举**（`src/runtime/tests/gc_incremental_model.rs`），**不用 loom**：
      切片与 mutator 步各自整体原子（切片停世界、mutator 只在 safepoint 之间被打断），只剩「顺序」这一个自由度，
      枚举顺序即完备；记忆化后 0.03 s，跑在默认测试集里而不是 `--cfg loom` 专腿。5 个策略开关各自给出反例
- [x] 3.9 **清扫期的 doomed 条目**（alive 但没有本周期 epoch）：弱 / 软读与堆遍历不交出（`admit_resurrected`）、
      minor 的脏卡播种跳过、校验器在周期打开时不查 epoch 与灰队列
- [x] 3.10 **周期期间的 minor 不得回收 major 已标记的条目**（`keep_major`）——
      0.05 ms 切片压测里「编译器读到已回收字符串」的根因；A/B：关掉规则两轮都在 ~6 个测试后挂死，打开连续 5 轮 50/50 全过
- [x] 3.11 实测见 M2c 表；GREEN 全绿（main `3c73e356`）

## M2c: 验收
- [ ] 4.1 性能验收表（**RSS 一项已知不达标，见备注「RSS 的真正来源是 chunk 碎片」**）（design「Testing Strategy」）：semantics 与 `13_gc_large_heap` 两档 max ≤ 10 ms；总停顿 ≤ +20%；墙钟 ≤ +2%；RSS ≤ +5%
- [ ] 4.2 正确性配方：冷 `package sdk`（默认 / 4M nursery）、`build stdlib`、`Z42_GC_SLICE_MS=0.05` 全套 stdlib 测试、`http_server_threaded` ×60
- [ ] 4.3 `xtask test` GREEN（含自举不动点）
- [ ] 4.4 文档：`docs/internals/src/runtime/gc-incremental-major.md`（NEW）、`gc-tuning-and-safepoint.md`、`docs/book/src/SUMMARY.md`
- [ ] 4.5 归档本 change；memory 更新

## 备注
- 2026-09-17 **RSS 的真正来源是 chunk 碎片，不是浮动垃圾**：`13_gc_large_heap` 上 M2b 的 RSS 比 M2a 高 30~43%，
  但**GC 计账的峰值 `used` 两者一样**（339M vs 355M）。差别在 chunk 数（obj 10453 vs 基线少一截）——
  切片 + 周期期间的保留让幸存者散布在更多 chunk 里，而 chunk 只有**全死**才能回池，TLAB 又只整块取、
  从不复用块内空洞（[[z42-memory-footprint-analysis]] 记的既有弱点）。⇒ 这条要么等空洞复用那条线，
  要么在 M2c 里单独定夺是否接受。试过两个便宜旋钮：pacer 目标 allowance/2→/4（RSS −8%、总停顿 +18%）、
  保留只覆盖标记期（RSS −10%、chunk −12%，压测 2 轮绿但**没有证明**，不敢拿它换刚修好的正确性）。
- 2026-09-16：**`copy_elems_from` 批量拷贝漏了屏障**——第一轮审计按「单元素写原语」找，批量快路径的 `clone_from_slice` 没进视野。
  M2b 用 `Z42_GC_SLICE_MS=0.05` 跑 `z42.net` 的 stdlib 测试时每轮 1~5 个文件 SIGSEGV（编译器 `TsigReconcile._rebuildClass` 里 `String.Substring` 读到被回收的字符串），
  审计照这条线索找到的漏洞随 M2a 一起提交（含阴性对照），规则已补「批量写同样是覆盖」；它是否就是那次 SIGSEGV 的全部原因，在 M2b 用修复后的二进制复跑压测确认。
- 2026-09-16：本地复测一度以为 M1 让 `cargo test --lib` 慢 10× 且间歇失败 —— 查清与本 change 无关：
  慢是环境（已供种 worktree 里 `VmContext::new` 会加载真 stdlib）；间歇失败是 main 既有的 **`config_tests::with_env`
  改真实环境时抢先初始化了进程级 `runtime_config()`，把整个测试进程的默认 GC 模式定成 concurrent**，已单独修（fix-config-tests-leak-gc-mode）。
- 行数棘轮：`gc/arc_heap/alloc.rs` 在基线 601 上，改动须净增 0 行（说明性注释放新模块）。
- 教训 49/50/51：每个里程碑的收益用两个二进制交错实测，并同时在 4M / 8M nursery 下验证（major 落点敏感）。
