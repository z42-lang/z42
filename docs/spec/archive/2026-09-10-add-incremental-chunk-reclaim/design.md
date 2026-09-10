# Design: per-chunk 普查

## Architecture

chunk 回收要回答两个问题，对每个 chunk 各一次：

1. **它有过块吗？**（从没用过的 chunk 没有存储可回收）
2. **它还有活块吗？**

这两个问题原本都是**扫出来**的 —— 定长区逐槽扫（`O(chunk 数 × 256)`），变长区逐块扫
`all_blocks` 并对每块二分查找归属（`O(块数 × log chunk 数)`）。改成**增量维护的计数器**
之后，两个问题各是一次数组读。

```
                    分配 / retire                tombstone
                         │                           │
   blocks_per_chunk[ci] ++                           │
   live_per_chunk[ci]   ++              live_per_chunk[ci] --
                                        max_gen_per_chunk[ci] = max(…)
                         └───────────┬───────────────┘
                                     ▼
              「全死」判定 = blocks[ci] > 0 && live[ci] == 0   ← O(1)
```

## Decisions

### Decision 1: `chunk_idx` 进块头 —— 而且是**免费**的

变长块的归属查询是整件事的根：`tombstone` 只拿得到块指针，`purge_blocks` 也只有指针。
没有「地址 → 槽下标」的算术（块是变长的），所以原来只能二分查找。

`GcBlockHeader` 是 `#[repr(C, align(8))]`，六个字段共 **12 字节**，被对齐**填充到 16**。
也就是说末尾有 **4 字节纯 padding** —— `chunk_idx: u32` 正好落进去，**头仍然是 16 字节**。

这条很关键：头的 16 字节是三堆设计的**不可动摇约束之二**（涨到 24 会把 180 万个总长
恰好 32 字节的块推进下一个 size class，吐回 15 MB+）。这次是在既有的空隙里放东西，
不是在挤位（#533 挤 `type_tag` 的空闲位是另一回事）。

### Decision 2: 计数器维护点必须穷举，一个漏掉就是静默的错回收

「全死」判错的后果是**把还有活块的 chunk 回收掉**——比慢严重得多。维护点：

| 事件 | `blocks`/`init` | `live` |
|---|---|---|
| `alloc` bump / dedicated | ++ | ++ |
| `alloc` 复用自由链槽 | 不变（槽早就算过） | ++ |
| `retire_chunk`（TLAB） | += 新构造的数量 | += 填充数 |
| `tombstone` | 不变 | −− |
| chunk 入池 / 释放 | 归零 | 归零 |
| `push_chunk` 复用墓碑槽位 | 归零 | 归零 |

两个坑：

- **`retire_chunk` 对复用的 chunk 是幂等写**（`init_row[ei] = true` 可能本来就是 true），
  所以只能数**跃迁**（`std::mem::replace` 的返回值），不能数 `hw`。
- **入池的 chunk 保留 `init` 计数**：它的槽仍然是构造好的（`ChunkClaim::fill` 靠这个
  保留每槽的 tombstone generation —— ABA 守卫），只是 `live` 归零。所以
  「已在池中」那道 guard 不能删，否则下一轮会把它再回收一次。

### Decision 3: 哈希集合换成按下标的标志表

普查修完之后，定长区的 `reclaim_dead_chunks` 还是 5–9 ms —— 剩下的全是
`already_pooled: HashSet` 的构建 + `free_list.retain` 里**每条一次哈希查找**，
而 free_list 有几十万条。换成 `vec![false; chunks.len()]` 之后：**5–9 ms → 0.44 ms**。

**规律：GC 里凡是「每个元素查一次集合」的地方，集合的键如果是 chunk 下标，
就该是标志表而不是 HashSet。**（同一族的第三次：#519 是 `young_list` 线性扫、
#521 是 `chunks` 线性扫。）

## Implementation Notes

- `bump()` / `alloc_dedicated()` 改成返回 `(header_ptr, chunk_idx)` —— 调用方本来就要
  把它写进头，顺手也拿来更新计数器。
- `VarGcRef::alloc_leaked` / 测试用的 leak 块不属于任何 region chunk，`chunk_idx` 写
  `u32::MAX` 哨兵；`tombstone` 与 `purge_blocks` 都对越界下标做范围检查后跳过。
- `max_gen_per_chunk` 在 `tombstone` 里维护，因为**只有 tombstone 会抬高块的 generation**。
  它是入池 chunk 的 `reuse_gen` 必须越过的 ABA 下限。

## Testing Strategy

- 既有的 `assert_eq!(size_of::<GcBlockHeader>(), 16)`（两处测试 + 一处 `const` 断言）
  就是「免费」这件事的守卫 —— 加字段加错了立刻红。
- 既有的 chunk 回收测试（`reclaim_dead_var_chunks_pools_dead_bump_chunks_among_dedicated_ones`
  / `reclaimed_chunk_purges_young_list` / `minor_reclaims_whole_dead_chunks` /
  `oversized_churn_does_not_grow_the_per_chunk_tables` 等）直接覆盖新判定 ——
  普查算错就会多回收或少回收，它们会红。
- 新增 per-chunk 普查与真值对账的测试（见 tasks.md 阶段 3）。
- **miri**：本 change 动了块头的布局与每一处 raw 写入，属于「必跑 miri」那一类
  （见 [[z42-gc-three-heap-roadmap]] #2 的教训）。

## 实测（同一 seed、同一次会话，`z42c.semantics --release --no-incremental`）

分停顿（分代模式，后几次大堆 minor）：

| 段 | 修前 | 修后 |
|---|---|---|
| `reclaim_dead_var_chunks` | **45–59 ms** | **6–10 ms** |
| `reclaim_dead_chunks` ×2（定长） | ~6.5 ms | **0.44 ms** |
| `mark_phase_minor` | ~23 ms | ~25 ms（未动） |

端到端：

| 配置 | 周期 | 停顿中位 | 停顿最大 | 峰值 RSS |
|---|---|---|---|---|
| stw 默认（修前） | 4 | 98.7 ms | 144.4 ms | 741.9 MB |
| **stw 默认（修后）** | 4 | **58.8 ms (−40%)** | **90.9 ms (−37%)** | 742.1 MB |
| gen（修前） | 10 | 76.1 ms | 152.9 ms | 764.5 MB |
| **gen（修后）** | 10 | **32.5 ms (−57%)** | **88.7 ms (−42%)** | 763.7 MB |
| gen nursery=8M（修前） | 61 | 57.2 ms | 110.7 ms | 576.7 MB |
| **gen nursery=8M（修后）** | 61 | **27.3 ms (−52%)** | **74.1 ms (−33%)** | 580.7 MB |

RSS 一分不差（回收的**判定**没变，只是变快了），墙钟在 nursery=8M 那一档 **−17%**
（9.37 → 7.80 s）—— minor 便宜了，勤跑就不再是负担。

**nursery 终于开始买停顿了**（分代，中位停顿）：32M → 32.5 ms，8M → 27.3 ms，
4M → 24.3 ms。仍不成正比，因为地板换成了 `mark_phase_minor` 的 ~25 ms（见下）。

## 还剩什么（按大小排）

1. 🔴 **`mark_phase_minor` ~25 ms，现在是最大的一段**。它是 O(young + 脏卡条目)，
   而**卡是 chunk 粒度的** —— 一个脏 chunk 里 256 条活条目**全部**当根。
   #539 之后晋升也会置脏卡，脏集因此更大。细化卡粒度是下一个杠杆。
2. **`purge_blocks` 的 retain，7–10 ms**：每元素已经是 O(1) 判定，但要摸 270 万个
   块头（随机读）。要再往下，得把 `all_blocks` 改成按 chunk 分桶（`Vec<Vec<ptr>>`），
   这样回收一个 chunk 就是 `clear()`。
3. **翻 `Z42_GC_MODE` 默认**：本 change 之后它第一次成为**真实的权衡** ——
   分代的中位停顿 32.5 ms 已经明显低于 STW 的 58.8 ms，代价是 RSS +2.9%、
   CPU 时间 +2%。在这之前分代是两头都输。要翻还是得先补 CI 的分代覆盖。
