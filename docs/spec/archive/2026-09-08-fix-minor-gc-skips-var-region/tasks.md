# Tasks: 变长区参与分代回收

> 状态：🟢 已完成 | 创建：2026-09-08 | 完成：2026-09-08

## 进度概览
- [x] 阶段 1: 块头带 `gen_age`（决策 1）—— 四条写入路径同步
- [x] 阶段 2: young 块集合 + 晋升（决策 2）+ `IN_YOUNG_BIT` 去重（实施期补）
- [x] ~~阶段 3: `VarRegion` 卡表~~ —— **实施期取消**，变长块不产生跨代写（design 决策 3）
- [x] 阶段 4: 接进 minor sweep + `gen_age_of` 读真实年龄（含 UAF 修复，决策 3b）
- [x] 阶段 5: 测试（单测 26 + 集成 24 全绿；三条新集成测试已验修复前必红）
- [x] 阶段 6: GREEN gate + 实测对账 + 文档同步 + 归档

## 实施期的两处偏离（已同步进 proposal / design / spec）
1. **阶段 3 整个取消**：原决策 D-2 的理由（"`Closure` 没有可借的头 → 借卡会漏跨代写"）不成立
   —— `ClosureData` 创建后不可变，变长块根本不被写。
2. **查证 ① 时发现一个 use-after-free**：minor 不清变长块的 mark 位 + `gen_age_of` 对变长块是瞎的
   → 闭包块带着陈旧 mark 进入下一轮 minor → `mark()` CAS 失败 → children 不再被追
   → 仅经它可达的年轻 `env` 数组被提前释放。回归测试验证：不修则第 2 轮必红。
3. **`young_list` 必须按模式设闸门**（`set_generational`，同 #524）：这个区有 270 万个块，
   非分代模式下一张没人消费的表实测多吃 20 MB RSS。Scope 追加
   `src/runtime/src/gc/arc_heap/interface.rs`（`set_mode` 转发）与
   `src/runtime/src/gc/arc_heap/construct.rs`（按模式构造）。
4. **Scope 追加 `src/runtime/src/metadata/vstr.rs`**：`Value::Str` 持的是 `vstr::Str` 而非
   `VarGcRef`，`gen_age_of` 需要一个转发。

---

## 阶段 1: 块头带 gen_age

- [x] 1.1 `var_region/block.rs`：`type_tag: u8` → `AtomicU8`；加位布局常量
      （`TAG_MASK = 0b111`、`AGE_SHIFT = 3`、`AGE_MASK = 0b11`）
- [x] 1.2 `var_region/block.rs`：`block_type()` 解包先 `& TAG_MASK` 再 `from_u8`；
      新增 `gen_age()` / `set_gen_age()` / `bump_gen_age()`（读 `Relaxed`）
- [x] 1.3 `var_region/block.rs`：确认 `size_of::<GcBlockHeader>() == 16` 静态断言未破
- [x] 1.4 `var_region.rs`：`write_fresh_header` 打包 `block_type | gen_age << 3`
- [x] 1.5 `var_region/chunk.rs`：`VarChunkClaim::fill` 同步（TLAB 无锁路径）
- [x] 1.6 `var_region/var_ref.rs`：`alloc_leaked` / `leak_block_for_test` 两处写入点同步
      （核对方式：`grep -n 'type_tag: block_type as u8'` 必须四处全改）
- [x] 1.7 `cargo test --lib gc::var_region` 全绿（既有 19 个测试不回归）

## 阶段 2: young 块集合 + 晋升

- [x] 2.1 `var_region.rs`：加 `young_list: Vec<NonNull<GcBlockHeader>>`；
      `alloc` 的 bump 与 free-list 复用两条路径都 push（复用槽 `gen_age = 0`）
- [x] 2.2 `var_region.rs`：`iterate_young(visit)` —— 遍历 `young_list`，
      跳过已 tombstone 的（懒删除，见决策 2）
- [x] 2.3 `var_region.rs`：`sweep_young()` —— 存活则 `bump_gen_age`，
      达 `PROMOTION_THRESHOLD` 则不写回新表；未标记则终结 + tombstone；返回 `(reclaimed, credited)`
- [x] 2.4 `var_region.rs`：`young_count()`（供升级启发式用）
- [x] 2.5 `var_region/chunk.rs`：`reclaim_dead_var_chunks` 的 `retain` 加上 `young_list`
      （复用现成 `in_reclaimed` 闭包）—— **漏了 = 悬垂指针**
- [x] 2.6 `cargo test --lib gc::var_region` 全绿

## ~~阶段 3: 卡表 + 跨代写屏障~~（实施期取消）

变长块不产生跨代写，卡表没有存在理由；老数组头经脏卡入根后 `mark_backing()` 已覆盖其元素块的标记。
依据与三类块的逐一核对见 design.md 决策 3。

## 阶段 4: 接进 minor + freed_bytes 口径

- [x] 4.1 `var_ref.rs` / `vstr.rs`：`VarGcRef::gen_age()` / `Str::gen_age()` 访问器
- [x] 4.2 `generational.rs`：`gen_age_of` 读变长块真实年龄
      （`Closure` 改读自身而非 env；`Str` / `FuncRef` 不再落到 `_ => 0`）—— **修 UAF**
- [x] 4.3 `generational.rs`：`sweep_phase_young_only` 调 `region_var.sweep_young()`，
      `credited` 并入 `freed_bytes`
- [x] 4.4 数组头的退账与元素块回收现在同轮发生（决策 5：虚账随覆盖到位自动消失）
- [x] 4.5 `control.rs`：`young_before` / `young_after` 的分母加上 `region_var.young_count()`
- [x] 4.6 `cargo test --lib` 全绿（1170+）

## 阶段 5: 测试

- [x] 5.1 `var_region_tests.rs`：`BlockType` × `gen_age` 全组合往返无干扰 + 头仍 16 字节
- [x] 5.2 各分配路径产出的块都 `gen_age == 0` 且在 young 表
- [x] 5.3 `sweep_young` 回收未标记 / 保留已标记 / 清 mark 位 / 达阈值晋升
- [x] 5.4 **复用的槽不会在 young 表里出现两次**（`IN_YOUNG_BIT` 去重）
- [x] 5.5 `reclaim_dead_var_chunks` 同时 purge `young_list`（断言 `young_list ⊆ all_blocks`）
- [x] 5.6 `arc_heap_tests`：minor 回收未 root 的年轻字符串块（`used_bytes` 实降）
- [x] 5.7 数组头 + 元素块同一次 minor 内一起回收
- [x] 5.8 **UAF 回归**：闭包 `env` 连跑 `PROMOTION_THRESHOLD + 2` 次 minor 仍存活
- [x] 5.9 老字符串不再被当作年轻块反复重标
- [x] 5.10 `freed_bytes ≤ used_before − used_after`
- [x] 5.11 **反向验证**：临时退回修复，5.6 / 5.8 / 5.9 三条必红（5.8 在 round 2，`0 vs 1`）
- [x] 5.12 集成测试逐个点名跑（**跳过 `signal_handler_e2e`**，本机会卡死，见 memory）

## 阶段 6: GREEN + 实测 + 文档 + 归档

- [x] 6.0 实测对账（`z42c.semantics --release --no-incremental`，macOS arm64）

      | 配置 | 基线 | 改后 | Δ |
      |---|---|---|---|
      | **stw 256M（默认路径）** RSS | 742.8–743.0 MB | 742.9–743.0 MB | **无变化** |
      | stw 256M 指令 | 79.31–79.37 G | 79.34–79.38 G | 无变化 |
      | 未武装 RSS | 888.67 MB | 888.70 MB | 无变化（分配路径未动） |
      | generational 256M RSS | 935.0 MB | 966.4 MB | **+31.4 MB（+3.4%）** |
      | generational 256M 指令 | 78.43 G | 78.65 G | +0.3% |
      | generational 回收次数 | 3 minor / 0 major | 2 minor / 0 major | −1 |

      **RSS 变差是账变诚实的直接后果**：基线 `freed_bytes` 虚高 → 预算闸门以为回收得少 →
      更早触发；修好后 `used_bytes` 真降 → 闸门更久才重新武装 → 少一次回收 → 高水位更高。
      而 minor 不做 chunk 级回收，释放的槽只进 free-list，压不下高水位。
      **本 change 交付的是正确性，不是 RSS。**

      ⚠️ **量测口径坑**：`--stats` 退出前会做一次全量 live snapshot，本身吃 20+ MB RSS。
      两边必须同带或同不带，否则会量出一个不存在的 48 MB「回归」。
- [x] 6.0b `./xtask test` 完整 GREEN（改动收尾后重跑，3m43s，10/10 stage）
- [x] 6.1 `docs/book/src/runtime/gc-tlab-chunk-exclusive.md`：新增「变长区的分代」一节
      —— 位布局、young 表的重建式维护、陈旧 mark 的坑、四条写入路径的同步要求
- [x] 6.2 `gc/mode.rs` 的 `GenerationalMarkSweep` 文档注释更新
      （「minor 扫 young_list + 脏卡」→ 覆盖三个 region）
- [x] 6.3 tasks.md 头部改 🟢 + 完成日期；`changes/` → `archive/2026-XX-XX-fix-minor-gc-skips-var-region/`
- [x] 6.4 归档与代码在**同一个 PR** 内（workflow 阶段 9 铁律，不得合并后补推）

---

## 交给后续 change 的发现

- **分代模式收得比 STW 还少**：256MB 预算下 0 次 major（升级启发式存活率 ≥ 0.75 从未触发），
  老垃圾从没被回收 → RSS 966 MB vs 纯 STW 743 MB。归 `add-bounded-nursery` /
  `arm-gc-by-default`。
- **minor 不做 chunk 级回收**：`reclaim_dead_chunks` / `reclaim_dead_var_chunks` 只在 major
  路径调。要让 minor 真正压低 RSS 高水位就得加，但该函数曾是停顿热点（见 book），
  每次 minor 都跑需要先量代价。
- **老块的陈旧 mark**：脏卡以老数组头入根时 `mark_backing()` 标记老元素块，minor 不清它 →
  该块若成垃圾会多活一个 major 周期。浮动垃圾，非正确性问题。

## 验收标准

1. `Z42_GC_MODE=generational` 下，minor GC 的实际回收字节 > 0 且与 `used_bytes` 下降一致
   （今天：几乎为 0，账面却虚高）
2. 数组头与其元素存储在同一次 minor 内一起回收
3. 跨代写屏障覆盖 `ArrayValue` / `Closure` 两类变长 owner，无漏记
4. `size_of::<GcBlockHeader>() == 16` 不变
5. `./xtask test` 全绿
