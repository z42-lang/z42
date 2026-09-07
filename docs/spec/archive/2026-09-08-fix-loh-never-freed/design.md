# Design: 大对象的 chunk 在死后归还分配器

## Architecture

变长区的 chunk 有两种形态，本 change 之后它们的**生命周期终点**第一次分开：

```
                       ┌─ 还有活块 ────────────────────────────► 留着，下一轮再看
   sweep 尾            │
   reclaim_dead_var ───┼─ 整块死 & cap == CHUNK_BYTES ─────────► var_free_chunk_pool
   _chunks             │   （bump chunk：内存留着，reuse_gen 抬高后重新 bump）
                       │
                       └─ 整块死 & cap != CHUNK_BYTES ─────────► dealloc + 墓碑槽位
                           （dedicated chunk：内存还给分配器）      → free_chunk_slots
```

「整块死」的判定**一行都不用新写**：`survey_chunks` 早就为每个 chunk 算了
`has_live` / `any_block`（它要为 bump chunk 算 `max_gen`，顺路把每个块归到它的 chunk）。
原来的代码是在选择循环里用 `cap != CHUNK_BYTES` 把 dedicated chunk **主动排除**掉的 ——
本 change 把这条 `continue` 换成了第二条出路。

### 槽位为什么不能删

`chunks` 的**下标就是 chunk 的身份**：`bump_chunk: Option<usize>`、`borrowed: Vec<bool>`、
`reuse_gen: Vec<u32>`、`var_free_chunk_pool: Vec<usize>` 全部按下标寻址。`Vec::remove`
会把它后面每一个 chunk 改号，于是 `bump_chunk` 指向别人、池子里的下标错位、
`borrowed` 的标记跑到隔壁 —— 一次静默的全面错乱。

所以释放是**原地**的：`Chunk::free_in_place` 把内存 `dealloc` 掉，把 `base` 置成
dangling、`cap` 置 0，槽位本身留在原位当墓碑。`cap == 0` 就是「已释放」的判据
（`is_freed`），三处消费它：

1. `VarRegion::drop` 跳过墓碑，否则 double free；
2. `ChunkIndex::build` 不给墓碑建地址区间 —— 它不拥有任何地址，任何块都不该落进去；
3. `partition_dead_chunks` 跳过墓碑，免得同一个槽位被释放两次。

墓碑槽位进 `free_chunk_slots`，`push_chunk` 优先复用它。没有这一步，
「大对象反复生灭」的负载会让三张 per-chunk 表按每个死块 ~29 B 只增不减 ——
RSS 依然单调增，只是慢了三千倍，而本 change 的验收标准恰恰是「不再单调增」。

## Decisions

### Decision 1: 直接 dealloc，不按 size 分桶入池（设计文档 D-3，选项 B）

**选项 A**：给 dedicated chunk 也做一个池子，按 size 分桶复用。
**选项 B**：死块在 sweep 尾直接 `dealloc`，把碎片管理交还给 mimalloc。

**选 B。** 实测这一路的块尺寸跨 80 KB–1.3 MB、离散得没有任何复用规律，按 size 分桶
命中率极低，**池子本身就会变成一笔不还的内存** —— 换句话说 A 是把今天的泄漏换个名字。
大对象件数少（百量级：一次 256M 构建里 127 块被释放），`malloc`/`free` 的开销可忽略；
mimalloc 本来就擅长这个尺寸段的复用。

### Decision 2: 释放会削弱 generation 守卫 —— 只在 oversized 上，接受（User 裁决 2026-09-08）

这是本 change 唯一真正的代价，写清楚免得以后有人踩了才发现。

**入池**的 chunk 内存还在：陈旧 `VarGcRef` 解引用读到的是一个仍然有效的块头，
只是 `reuse_gen` 已被抬高、`generation` 对不上 → `resolve` 干净地返回 `None`。
这就是 #533 那次分代崩溃呈现成 `__str_hash_code: arg 0 expected string, got Null`
的原因 —— 守卫把一个标记 bug 兜成了一条可读的错误。

**释放**掉的 chunk 没有这层网：内存已经不属于这个进程的这块用途了，陈旧句柄解引用
就是 use-after-free；更糟的是分配器可能把同一地址交给别的分配，`generation` 撞上
（新块从 0 起，而陈旧句柄拿的正是它 alloc 时的 0）就成了类型混淆。

**为什么仍然接受**：

- 块走到这里的前提是**刚刚那次 sweep 把它 tombstone 了**，即它从任何根都不可达 ——
  标记正确时，没有活结构还持有指向它的句柄；
- 影响面只有 oversized 块（> 64 KB），本仓一次完整构建里也就百量级；
- 三个 region 的 sweep 都在 mutator 停住时跑（STW 模式直接 STW；并发模式在
  Phase 6 的 handshake pause 里），不存在「一边释放一边有人解引用」；
- 释放前 `purge_blocks` 已把该块从 `all_blocks` / `free_lists` / `young_list` 三张表
  全部摘掉 —— GC 自己不会再碰它。

**代价的准确表述**：一个标记 bug 过去在 oversized 块上表现为 `None`（一个
`Null`），现在表现为内存损坏。不是「引入了 bug」，是「同一个 bug 的现场变难看了」。

### Decision 3: 不动 `used_bytes` 计账

`used_bytes` 记的是**逻辑**字节，收在 alloc、退在 sweep（`alloc_charge_bytes`）。
块被 tombstone 的那一刻账就已经退了；chunk 的 `dealloc` 发生在之后，还的是**物理**内存。
两者是两回事，本 change **一个字节的计账都不改** —— 这也是它不会重蹈 #522 覆辙的原因。
`VarChunkReclaim::freed_bytes` 只用于观测，不喂给预算闸门。

### Decision 4: `Z42_GC_LOH_BYTES` 推到 `add-bounded-nursery`

设计文档的旋钮总表把 `Z42_GC_LOH_BYTES`（默认 64 K，低延迟 profile 用 32 K）
挂在本 change 名下。实施时发现它的代价不在自己身上：

- 门槛今天是 `chunk.rs` 的 `const CHUNK_BYTES`，被 `class_for` 用；
- `class_for` 是**最热的分配路径**（每个字符串 / 闭包 / 数组元素块各一次），
  且被 `alloc.rs` 的 TLAB 快路径在**没有 region 引用**的地方调用 —— 所以门槛只能做成
  进程级全局原子，每次分配多一次 relaxed load；
- 今天没有任何消费者：第一个真要拧它的是 change 3（`add-bounded-nursery`）的 profile 实验。

**决定（User 裁决 2026-09-08）：推到 change 3**，和它的第一个真实消费者一起进、一起 A/B。
本 change 的门槛保持 `const CHUNK_BYTES`。

## Implementation Notes

- `reclaim_dead_var_chunks` 原本 90 行、越过 60 行函数硬限，本 change 又要往里加分支，
  因此拆成 `ChunkIndex`（地址 → chunk 下标的二分，#521 的产物，原本是函数内的闭包）、
  `survey_chunks`（每 chunk 的 `has_live` / `max_gen` / `any_block` 普查）、
  `partition_dead_chunks`（分成入池 / 释放两组）、`purge_blocks`（三张表的清扫）。
  行为等价，纯提取。
- `chunk_count()`（`#[cfg(test)]`）改成「仍持有内存的 chunk 数」而不是 `chunks.len()`
  —— 墓碑槽位是记账、不是内存占用，而所有用到它的测试问的都是后者。
  另加 `chunk_slot_count()` 给槽位复用的测试用。

## Testing Strategy

单测（`var_region_tests.rs`）：

- `dead_oversized_chunk_is_freed_and_its_slot_reused` —— 释放发生、`freed_bytes` 覆盖整块、
  `chunk_count` 归零而 `chunk_slot_count` 不变、三张表都不再持有那个指针、下次 alloc 复用槽位
- `live_oversized_chunk_is_never_freed` —— 只释放没被标记的那个；幸存者的 payload
  末字节仍读得回原值（证明它的 chunk 没被误释放）
- `oversized_churn_does_not_grow_the_per_chunk_tables` —— 8 轮生灭后 chunk 内存归零、
  槽位表不增长（「RSS 不再单调增」的区级形式）
- `freeing_a_dedicated_chunk_leaves_bump_chunk_indices_valid` —— dedicated 与 bump chunk
  交错，释放后幸存者仍可写，且每个仍被跟踪的指针都落在区还拥有的 chunk 里

端到端（`arc_heap_tests/tlab.rs`）：

- `oversized_var_blocks_are_freed_not_leaked` —— 6 轮 × 4 个 200 KB 字符串 + `force_collect`，
  chunk 数达到稳态。**已验证非空转**：把释放那一路去掉后该测试红，
  chunk 数为 `[4, 8, 12, 16, 20, 24]`

实测（`z42c.semantics --release --no-incremental`，同一次会话顺序单跑，RSS 复现性 ±0.01%）：

| 预算 | main 峰值 RSS | 本 change | Δ | main 墙钟 | 本 change |
|---|---|---|---|---|---|
| 128M | 604.1 MB | **596.1 MB** | **−8.0 MB (−1.3%)** | 7.27 s | 7.25 s |
| 256M | 743.1 MB | **727.2 MB** | **−16.0 MB (−2.1%)** | 6.46 s | 6.52 s |

探针实测的归还量：256M 一次构建 **12.0 MB / 127 块**，128M **19.3 MB / 161 块**。
墙钟差落在本机 ~2.9% 的跑间离散度内，视作持平（改动只在 sweep 尾多几百次 `dealloc`）。

**未武装（默认）的行为完全不变** —— 一次回收都不触发，这条路径根本不执行。
