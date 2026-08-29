# Tasks: GC 线程本地分配（TLAB / chunk 独占）

> 状态：🟡 进行中 | 创建：2026-08-29

## 进度概览
- [ ] 阶段 1: stats 原子化（option B，去 inner 锁；byte-identical）
- [ ] 阶段 2: Region\<T\> TLAB（对象/数组并行零锁）
- [ ] 阶段 3: VarRegion TLAB（string/closure/array 数据）
- [ ] 阶段 4: 翻并行默认 + jobs-scaling 实测
- [ ] 阶段 5: 文档同步

## 阶段 1: stats 原子化（去 inner 锁）
- [ ] 1.1 `gc/types.rs`：`HeapStats.used_bytes` / `allocations` → `AtomicU64`（其余字段沿旧）
- [ ] 1.2 `gc/arc_heap/alloc.rs`：`record_alloc` 改原子 `fetch_add`，去 inner 锁 step 1
- [ ] 1.3 `check_pressure` / `maybe_auto_collect` / `would_oom_after_alloc` 改原子读 used_bytes
- [ ] 1.4 `interface.rs` `used_bytes()` / `stats()` 等读点改原子读；51 处 inner 读点核对无回归
- [ ] 1.5 `cargo test --lib` GC 单测全绿 + `xtask test` 全绿 + byte-identical 自举（行为不变）

## 阶段 2: Region\<T\> TLAB（对象/数组）
- [ ] 2.1 `gc/tlab.rs`（NEW）：`ChunkClaim{chunk_idx, slots 裸指针, next, cap, high_water}` + `Tlab{obj,arr,var}` + `unsafe impl Send`
- [ ] 2.2 `gc/region.rs`：`borrow_chunk()->ChunkClaim`（pool 优先 / grow）+ `retire_chunk(claim, hw)`（批量 initialized+young_list+stats flush）+ `free_chunk_pool` + 全死 chunk 回收（sweep 尾）
- [ ] 2.3 `vm_context/types.rs` + `construct.rs`：VmContext 加 `tlab` + 初始化；drop 前 retire
- [ ] 2.4 `arc_heap/interface.rs` + `alloc.rs`：`finish_alloc`/`alloc_array_obj` 走 TLAB fast path（带 ctx）；满则 retire+borrow；strict_oom 退化逐对象锁（D6）
- [ ] 2.5 `gc/safepoint.rs`：`request_gc_pause` 握手中各 mutator park 前 retire TLAB
- [ ] 2.6 单测：borrow/retire 逐字段等价、跨 chunk、多线程并发对象/数组、chunk 回收、跨线程引用+线程退出、strict_oom 退化
- [ ] 2.7 `cargo test --lib` + `xtask test` 全绿 + byte-identical 自举

## 阶段 3: VarRegion TLAB
- [ ] 3.1 `gc/tlab.rs`：`VarChunkClaim{chunk base, cap, bump_off, 本地块头 Vec, high_water}`
- [ ] 3.2 `gc/var_region.rs`：`borrow_chunk()`/`retire_chunk()`（批量 append all_blocks+live_count+stats）；oversized/free-list 仍走锁路径
- [ ] 3.3 `arc_heap/interface.rs` + `alloc.rs`：`alloc_str/closure/var_block` 走 TLAB fast path；safepoint retire var TLAB
- [ ] 3.4 单测：单线程跨 chunk、retire 等价、多线程并发 var 分配
- [ ] 3.5 `cargo test --lib` + `xtask test` 全绿 + byte-identical 自举

## 阶段 4: 翻并行默认 + 实测
- [ ] 4.1 jobs-scaling 实测：workspace build 墙钟扫 jobs∈{1,2,4,8,24}，确认 CpuCount 快于 1（转正）
- [ ] 4.2 `src/compiler/z42c.semantics/src/ParallelFor.z42`：`ParallelConfig` 默认 `_jobs` 1 → `CpuCount()`
- [ ] 4.3 byte-identical 自举（默认并行下 gen1==gen2，确定性）
- [ ] 4.4 完整 `xtask test` 全绿

## 阶段 5: 验证与文档
- [ ] 5.1 `cargo build --release`（z42vm）无错
- [ ] 5.2 `xtask test`（e2e/cross-zpkg/stdlib/compiler/vscode-syntax）全绿
- [ ] 5.3 spec scenarios 逐条覆盖确认
- [ ] 5.4 `docs/book/` GC 机制页：TLAB/chunk 独占（borrow/retire/回收 + safepoint + mermaid）+ Deferred（slot 级复用）
- [ ] 5.5 `docs/roadmap.md` Deferred Backlog Index 加「TLAB slot 级复用」索引行
- [ ] 5.6 `src/runtime/src/gc/README.md`：功能索引 + 核心文件（新增 tlab.rs）
- [ ] 5.7 归档矩阵逐行核对（内部机制变更 → book 机制页；无对外行为/格式变更）

## 备注
- 最难的并发不变量在阶段 2（borrow chunk 独占写 + retire 批量并元数据 + safepoint retire）——`// SAFETY` + 单测夯实。
- **正确性门**：每阶段 byte-identical 自举（gen1==gen2）。**性能门**：阶段 4 jobs-scaling 转正才翻默认。
- cold 自举路径本地不可验，交 CI。
- 本机 24 核；nightly 种子 `/tmp/z42-nightly`（post-shrink，含 threading）；本机 stale 主树种子勿用。
