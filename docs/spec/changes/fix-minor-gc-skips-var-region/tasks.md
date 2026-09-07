# Tasks: 变长区参与分代回收

> 状态：🟡 进行中 | 创建：2026-09-08

## 进度概览
- [ ] 阶段 1: 块头带 `gen_age`（决策 1）—— 四条写入路径同步
- [ ] 阶段 2: young 块集合 + 晋升（决策 2）
- [ ] 阶段 3: `VarRegion` 卡表 + 跨代写屏障（决策 3）
- [ ] 阶段 4: 接进 minor mark / sweep + `freed_bytes` 口径（决策 5）
- [ ] 阶段 5: 测试 + 实测对账
- [ ] 阶段 6: 文档同步 + 归档

---

## 阶段 1: 块头带 gen_age

- [ ] 1.1 `var_region/block.rs`：`type_tag: u8` → `AtomicU8`；加位布局常量
      （`TAG_MASK = 0b111`、`AGE_SHIFT = 3`、`AGE_MASK = 0b11`）
- [ ] 1.2 `var_region/block.rs`：`block_type()` 解包先 `& TAG_MASK` 再 `from_u8`；
      新增 `gen_age()` / `set_gen_age()` / `bump_gen_age()`（读 `Relaxed`）
- [ ] 1.3 `var_region/block.rs`：确认 `size_of::<GcBlockHeader>() == 16` 静态断言未破
- [ ] 1.4 `var_region.rs`：`write_fresh_header` 打包 `block_type | gen_age << 3`
- [ ] 1.5 `var_region/chunk.rs`：`VarChunkClaim::fill` 同步（TLAB 无锁路径）
- [ ] 1.6 `var_region/var_ref.rs`：`alloc_leaked` / `leak_block_for_test` 两处写入点同步
      （核对方式：`grep -n 'type_tag: block_type as u8'` 必须四处全改）
- [ ] 1.7 `cargo test --lib gc::var_region` 全绿（既有 19 个测试不回归）

## 阶段 2: young 块集合 + 晋升

- [ ] 2.1 `var_region.rs`：加 `young_list: Vec<NonNull<GcBlockHeader>>`；
      `alloc` 的 bump 与 free-list 复用两条路径都 push（复用槽 `gen_age = 0`）
- [ ] 2.2 `var_region.rs`：`iterate_young(visit)` —— 遍历 `young_list`，
      跳过已 tombstone 的（懒删除，见决策 2）
- [ ] 2.3 `var_region.rs`：`sweep_young()` —— 存活则 `bump_gen_age`，
      达 `PROMOTION_THRESHOLD` 则不写回新表；未标记则终结 + tombstone；返回 `(reclaimed, credited)`
- [ ] 2.4 `var_region.rs`：`young_count()`（供升级启发式用）
- [ ] 2.5 `var_region/chunk.rs`：`reclaim_dead_var_chunks` 的 `retain` 加上 `young_list`
      （复用现成 `in_reclaimed` 闭包）—— **漏了 = 悬垂指针**
- [ ] 2.6 `cargo test --lib gc::var_region` 全绿

## 阶段 3: 卡表 + 跨代写屏障

- [ ] 3.1 `var_region.rs`：加 per-chunk `card_dirty` 位表；
      `mark_card_dirty(ci)` / `iterate_dirty_cards(visit)` / `clear_card_dirty()`
- [ ] 3.2 `var_region/chunk.rs`：`push_chunk` 同步增长 `card_dirty`
      （与 `borrowed` / `reuse_gen` 一样保持长度一致）
- [ ] 3.3 `arc_heap/generational.rs`：`maybe_mark_cross_gen_card` 支持变长块 owner；
      `Str` / `ArrayPrim`（叶子）短路返回，只有 `ArrayValue` / `Closure` 脏卡
- [ ] 3.4 `arc_heap/collect.rs`：`run_cycle_collection_major` 末尾同步清 `region_var` 的卡
- [ ] 3.5 `cargo test --lib gc::` 全绿

## 阶段 4: 接进 minor + freed_bytes 口径

- [ ] 4.1 `arc_heap/generational.rs`：`mark_phase_minor` 加 `region_var` 脏卡根
- [ ] 4.2 `arc_heap/generational.rs`：`mark_phase_minor` 的 BFS 把年轻变长块入队
      （`ArrayValue` / `Closure` 有出边需展开；`Str` / `ArrayPrim` 是叶子）
- [ ] 4.3 `arc_heap/generational.rs`：`sweep_phase_young_only` 调 `region_var.sweep_young()`，
      `credited` 并入 `freed_bytes`
- [ ] 4.4 `arc_heap/generational.rs`：确认数组头的退账与元素块回收现在同轮发生
      （决策 5：不改 `array_size_estimate`，虚账随覆盖到位自动消失）
- [ ] 4.5 `arc_heap/control.rs`：`young_before` / `young_after` 的分母加上
      `region_var.young_count()`
- [ ] 4.6 `cargo test --lib` 全绿（1170+ 个）

## 阶段 5: 测试 + 实测对账

- [ ] 5.1 `var_region_tests.rs`：`BlockType` × `gen_age` 全组合往返无干扰
- [ ] 5.2 `var_region_tests.rs`：各分配路径产出的块都 `gen_age == 0` 且在 young 表
- [ ] 5.3 `var_region_tests.rs`：连续 `PROMOTION_THRESHOLD` 次后移出 young 表
- [ ] 5.4 `var_region_tests.rs`：被回收 chunk 里的 young 块从 `young_list` 一并 purge
- [ ] 5.5 `arc_heap_tests/generational.rs`：年轻 `Str` 块在 minor 里被回收，`used_bytes` 实降
- [ ] 5.6 `arc_heap_tests/generational.rs`：数组头 + `ArrayValue` 元素块**同一次** minor 内一起回收
- [ ] 5.7 `arc_heap_tests/generational.rs`：已晋升的变长块不被 minor 访问
- [ ] 5.8 `arc_heap_tests/generational.rs`：老 `Closure` 写入年轻引用 → 脏卡 → 该对象在 minor 中存活
- [ ] 5.9 `arc_heap_tests/generational.rs`：`freed_bytes ≤ used_before − used_after`
- [ ] 5.10 集成测试逐个点名跑（**跳过 `signal_handler_e2e`**，本机会卡死，见 memory）
- [ ] 5.11 `./xtask test` 完整 GREEN
- [ ] 5.12 实测对账：`Z42_GC_MODE=generational Z42_GC_MAX_BYTES=256M` 跑
      `z42c.semantics --release --no-incremental`，确认 minor 累计 `freed_bytes`
      与 `used_bytes` 实测下降一致；记录 RSS / 指令 / 墙钟三项对基线的变化

## 阶段 6: 文档同步 + 归档

- [ ] 6.1 `docs/book/src/runtime/gc-tlab-chunk-exclusive.md`：新增「变长区的分代」一节
      —— 位布局、young 表的重建式维护、卡表、三条写入路径的同步要求
- [ ] 6.2 `gc/mode.rs` 的 `GenerationalMarkSweep` 文档注释更新
      （「minor 扫 young_list + 脏卡」→ 覆盖三个 region）
- [ ] 6.3 tasks.md 头部改 🟢 + 完成日期；`changes/` → `archive/2026-XX-XX-fix-minor-gc-skips-var-region/`
- [ ] 6.4 归档与代码在**同一个 PR** 内（workflow 阶段 9 铁律，不得合并后补推）

---

## 验收标准

1. `Z42_GC_MODE=generational` 下，minor GC 的实际回收字节 > 0 且与 `used_bytes` 下降一致
   （今天：几乎为 0，账面却虚高）
2. 数组头与其元素存储在同一次 minor 内一起回收
3. 跨代写屏障覆盖 `ArrayValue` / `Closure` 两类变长 owner，无漏记
4. `size_of::<GcBlockHeader>() == 16` 不变
5. `./xtask test` 全绿
