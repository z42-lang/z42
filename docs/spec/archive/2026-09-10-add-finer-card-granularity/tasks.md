# Tasks: 卡粒度 32 张 + 扫过即清

> 状态：🟢 已完成（2026-09-10）

| 阶段 | 状态 |
|---|---|
| 1. 量 mark 的种子构成 | ✅ |
| 2. 32 张卡（免费）+ 扫过即清 | ✅ |
| 3. 试「不把年轻条目当根」→ **放弃**，有对照数据 | ✅ |
| 4. 测试 + GREEN + 实测 + 文档 + 归档 | ✅ |

## 阶段 1: 量种子构成

- [x] 给 `mark_phase_minor` 的播种与 BFS 分别计时，并按来源计根数
- [x] 结果：后期 minor **播 518 530 个脏卡根，只标记到 76 个年轻对象**；
      播种本身很便宜（0.2–3 ms），**BFS 9–21 ms 才是 mark 的大头**
- [x] 两个原因：一张卡盖 256 条；**卡在 major 之前从来不清**，而写屏障和晋升（#539）
      都在往里加 —— 脏集滚到活堆的约 65%

## 阶段 2: 实现

- [x] `CARDS_PER_CHUNK = 32` / `ENTRIES_PER_CARD = 8` / `card_of()`；
      **`card_dirty` 一直是 `Vec<u32>` 却只用 bit 0，所以是免费的**
- [x] `mark_card_dirty(ci)` → `mark_card_dirty(ci, ei)`（四处调用点：写屏障 ×2、
      晋升补卡 ×2、`rebuild_card_table` ×2）
- [x] `iterate_dirty_cards` 按位遍历（`trailing_zeros` + `bits &= bits-1`），回传卡号
- [x] `clean_card`；`seed_from_dirty_cards` 用单槽累加器结算每张卡
- [x] 老条目在播种处直接穿透（#537 之后 BFS 对它本来就只做这件事）

## 阶段 3: 试过并放弃的那一版

- [x] 「不把年轻的脏卡条目当根」：停顿没再多赚（18.2 vs 18.3 ms），RSS 也没变
- [x] 对照实验（保留旧播种、只改粒度与清扫）：24.5 ms / 572.2 MB
- [x] **两版 `gc_reclaimed_bytes` 逐字节相同**（449 084 216）→ 回收的是同一批对象，
      小 nursery 档 +6% RSS 是**标记顺序影响分配局部性**，不是保留策略变了
- [x] 结论：去掉年轻根是**语义**改动，单独立项

## 阶段 4: 测试 / GREEN / 文档

- [x] `a_write_dirties_only_its_own_card` / `clean_card_clears_only_that_card`
- [x] 既有卡测试跟随新签名
- [x] #537 / #539 的三个跨代正确性测试原样通过（它们正是「卡清早了会丢对象」的守卫）
- [x] `./xtask test` 全绿
- [x] `docs/book/src/runtime/gc-tuning-and-safepoint.md`
- [x] 归档

## 交给后续 change 的发现

1. **`purge_blocks` 的 retain 7–10 ms**：每元素已是 O(1) 判定，但要摸 270 万个块头
   （随机读）。要把 `all_blocks` 改成按 chunk 分桶，回收一个 chunk 就是 `clear()`。
2. **「不把年轻的脏卡条目当根」** 是一次语义改动，需单独立项 + 单独证据。
3. 🔴 **翻 `Z42_GC_MODE` 默认**：分代中位停顿现在 **22.2 ms vs STW 57.9 ms（−62%）**，
   代价 RSS +2.3%。前置仍是**补 CI 的分代覆盖**（CI 从来不跑这个模式，
   正是 #537/#539 三个丢对象缺陷能活几个月的原因）。

## 验收标准

- 一次置脏只让 `ENTRIES_PER_CARD` 条成为根
- 不再有跨代边的卡在被扫过后变干净，且跨代不变量不被破坏
- STW 模式的行为与用量不变
- 分代 minor 的中位停顿显著下降
