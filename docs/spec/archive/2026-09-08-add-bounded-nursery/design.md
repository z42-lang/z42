# Design: 两个闸门 + 会还 chunk 的 minor

## Architecture

分代堆有两个独立的容量，就该有两个独立的闸门：

```
   分配 ──► used - baseline ≥ NURSERY_BYTES ──────────────► minor
                （新生代装满了）

   晋升 ──► promoted_since_major ≥ MAX_BYTES ─────────────► major
                （老年代吃进了一个预算的量）
```

**为什么不能只用一个。** 原来的唯一闸门是 `used ≥ near_limit × MAX_BYTES`，而
`used_bytes` 记的是**活字节**。minor 回收年轻垃圾，把活字节压得很低 —— 于是这个闸门
在分代模式下既不是新生代的容量（它要等堆整体接近预算才开），也看不见老年代的增长
（死掉的老对象不在活字节里）。实测：`used` 稳定在 90 MB 上下，而 RSS 到 1 GB。

**晋升字节数是唯一看得见老年代增长的量**，而且它的维护点在 minor sweep 里 ——
那里本来就在算「这一轮谁跨过了阈值」（#539 为了补脏卡加的名单），所以**分配路径零成本**。

## Decisions

### Decision 1: minor 尾部必须做 chunk 级回收（RSS 的主修）

`reclaim_dead_chunks` / `reclaim_dead_var_chunks` 原本只在 `sweep_phase`（major）里调。
minor 把条目 tombstone 掉进 free list，但**一块 chunk 都不还给池子** —— 而 TLAB 是
**整块**发给 mutator 的，一阵短命对象通常整块死。也就是说 minor 最该收的那种形状，
它恰恰不收。

| 128M 预算 | 周期 | 峰值 RSS |
|---|---|---|
| 分代（不做） | 15 minor / 1 major | 986.3 MB |
| 分代（做） | 15 minor / 1 major | **607.9 MB** |

同样的回收次数，−38%。**这不是「major 跑得不够多」的问题** —— 那是最初的猜测，被这组
数字直接推翻。

⚠️ **代价写在这里**：这个 pass 是 **O(堆)** 而不是 O(young)，是 minor 停顿的大头
（中位停顿 ~30 ms → ~75 ms）。它给「nursery 越小停顿越低」压了一个地板 —— 见「留给下一步」。

### Decision 2: 老年代闸门用晋升字节数（User 裁决）

**选项 A**：晋升字节数 ≥ 老年代预算 → major。
**选项 B**：把 `MAX_BYTES` 的口径从「活字节」改成「已提交的 chunk 字节」。
**选项 C**：每 N 次 minor 补一次 major。

**选 A。** 老年代垃圾的上界就是流进去的量，这是标准做法；维护点在 sweep 里，**分配路径
零成本**。B 最直接对应 RSS，但会**同时改变 STW 模式下 `MAX_BYTES` 的含义**，把所有既有
实测表作废，波及面太大。C 的 N 与负载和预算都无关，等于把调参责任推给用户。

阈值取整个 `MAX_BYTES`：老年代吃进一个预算的量就整理一次。实测这个负载上约 1–3 次 major。

### Decision 3: `Z42_GC_NURSERY_BYTES` 默认 `MAX_BYTES / 4`（User 裁决）

比例而不是绝对值，同一个设置在任何预算下含义相同 —— 固定 32 MB 在 64 MB 预算下是半个堆、
在 256 MB 预算下是八分之一。128M 预算下正好得到设计文档建议的 32M。

### Decision 4: 存活率口径 —— 晋升不是死亡

升级启发式原本算 `young_after / young_before`，读的是 sweep 之后 young 表的长度。
可熬过 `PROMOTION_THRESHOLD` 的幸存者**会离开 young 表** —— 阈值是 2，于是任何一次
minor 的幸存者大多被算成「没活下来」，存活率永远低于 `Z42_GC_MINOR_THRESHOLD`，
**升级从没触发过**。改成 `1 - reclaimed_entries / young_before`：sweep 真正 tombstone
掉的那部分才叫死。

### Decision 5: 徒劳退避对着 throttle 比，不对着增长闸门比

退避的判据是「上一次回收有没有买到一个增长闸门的空间」。分代模式把增长闸门换成 nursery
（预算的四分之一）之后，一次收掉大半个 32M nursery 的**健康** minor 也会被判成
「不足 32M → 徒劳」，退避每次翻倍。实测 minor 从 18 次掉到 8 次、堆干脆不收了。

两个量回答的是不同的问题：**再试之前要有多少新分配** vs **上一次试值不值**。
后者固定用 `throttle_ratio × limit`。

## Implementation Notes

- `pending_major` 是策略层与执行层之间的唯一通道：`maybe_auto_collect` 决定要哪种回收，
  但真正的回收延迟到 mutator 的 safepoint 才跑，而那条路只知道「collect」。
- `promoted_size_of_objects` / `..._arrays` 复用 sweep 退账用的同一个估算器，
  保证「晋升进去多少」和「回收退回多少」是同一个单位。
- 分代判定用 `MagrGC::mode(self)`（`ArcMagrGC` 有个同名字段，直接 `self.mode()` 会撞上）。

## Testing Strategy

- `generational_trips_a_minor_on_the_nursery_gate_below_the_near_limit` ——
  `used` 全程低于 `0.9 × 预算`，仍然发生了 minor（旧的单闸门下这个负载一次都不收）
- `the_nursery_gate_is_generational_only` —— 同样的负载在 STW 模式下**零**周期
- `minor_reclaims_whole_dead_chunks` —— minor 之后 `free_chunk_pool` 非空
- `promotion_feeds_the_old_gen_budget_and_a_major_resets_it`
- `gc_nursery_bytes_parses_the_same_suffixes_as_gc_max_bytes` / `..._is_unset_by_default`

## 实测（同一 seed、逐条显式 `env`）

`z42c.semantics --release --no-incremental`：

| 配置 | 周期 | minor/major | 停顿中位/最大 | 墙钟 | 峰值 RSS |
|---|---|---|---|---|---|
| 未武装 | 0 | — | — | 8.31 s | 1013.8 MB |
| stw 128M | 14 | 0/14 | 78.7 / 113.5 ms | 7.47 s | 606.8 MB |
| stw 256M | 3 | 0/3 | 131.0 / 138.0 ms | 6.75 s | 763.2 MB |
| **gen 128M** | 15 | 15/1 | **76.7 / 84.1 ms** | **7.33 s** | **618.7 MB** |
| gen 256M | 7 | 7/1 | 90.0 / 128.7 ms | 6.96 s | **728.1 MB** |

**分代模式第一次值得打开**：128M 下与 STW 的 RSS 打平（+2%）、墙钟略快（−1.9%）、
**最大停顿 −26%**（84.1 vs 113.5 ms）；256M 下 RSS **−4.6%**（728.1 vs 763.2 MB）。
修前是 986.3 MB。

### nursery 旋钮确实在买停顿（128M 预算）

| `NURSERY_BYTES` | 周期 | 停顿中位/最大 | 墙钟 | 峰值 RSS |
|---|---|---|---|---|
| 8M | 27 | 67.6 / **78.5** ms | 7.93 s | 609.2 MB |
| 32M（默认 = 预算/4） | 15 | 76.7 / 84.1 ms | 7.33 s | 618.7 MB |
| 64M | 8 | 90.3 / **127.8** ms | 7.19 s | 679.8 MB |

最大停顿随 nursery 单调变化（78.5 → 127.8 ms），RSS 与墙钟朝相反方向走 ——
这正是这个旋钮该有的形状。

## 留给下一步：chunk 回收是停顿的地板

中位停顿只从 67.6 走到 90.3 ms，远不如最大停顿敏感，因为**chunk 级回收是 O(堆) 的**，
和 nursery 大小无关。设计文档里「p99 随 nursery 容量线性变化」要完全成立，
得先把 `reclaim_dead_chunks` 做成增量的（只看这一轮 minor 碰过的 chunk）。
在那之前，nursery 能买到的停顿有一个由堆大小决定的下界。
