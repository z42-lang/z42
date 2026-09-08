# Design: 两个旋钮，两种不同的「不能放热路径」

两个旋钮都被同一个约束挡了很久 —— **它们的消费者在最热的路径上** —— 但挡法不同，
所以解法也不同。

| 旋钮 | 消费者 | 为什么不能运行时读 | 解法 |
|---|---|---|---|
| `Z42_GC_PROMOTION_AGE` | 写屏障（每次堆引用写） | 全局查找注入热路径 | **构造期读一次**，缓存进 `ArcMagrGC` / `Region` / `VarRegion` 的字段 |
| `Z42_GC_LOH_BYTES` | `class_for`（每次变长块分配） | 同上 **且**调用点在无锁 TLAB 快路径里，**手里没有堆引用** | **进程级 `static`**，VM 构造时写一次，热路径一次 relaxed load |

## Decisions

### Decision 1: `PROMOTION_AGE` 走构造期读取（User 裁决 2026-09-08）

书里那节「刻意不做」的两条理由都成立，而书自己就给了出路：
「真出现需求时应做成**构造期**（per-heap 一次读取、缓存进 heap 字段）」。照做：

- `gc::promotion_age_from_config()` 在 `ArcMagrGC::default()` 里调**一次**；
- 值分发给三处各存一份 `promotion_age: u8`（堆、两个定长 region、变长 region）；
- 写屏障读 `self.promotion_age` —— **普通字段，不是原子**。
  能这么写是因为**值在堆的生命周期内不可变**：只在构造时定，此后没有 setter。
  这也是三份副本能安全存在的理由（对比 `generational` 标志，它有 `set_mode` 这条改法，
  所以必须有 `set_generational` 把三处同步）。
- `PROMOTION_THRESHOLD` 常量**保留**，语义从「值」变成「默认值」——
  约 20 处 `for _ in 0..PROMOTION_THRESHOLD` 的测试一行不用改，它们问的本来就是
  「默认年龄下的行为」。

**范围 `1..=MAX_GEN_AGE`，越界 clamp 而不是饱和。** 上界是硬的：年龄打包在
`GcBlockHeader::type_tag` 的两个空闲位里（#533），3 是能表示的最大值。
0 会让一切在第一次 minor 就晋升（等于没有年轻代）。
**饱和是错的**：`bump_gen_age` 本来就在 `MAX_GEN_AGE` 处饱和，如果阈值 > 上界，
年龄永远「到不了」阈值 —— 什么都不会被晋升，而且是静默的。

### Decision 2: `LOH_BYTES` 只能是进程级

`class_for` 的调用点有三处，其中 `arc_heap/alloc.rs::tlab_alloc_var` 是**无锁的 TLAB
快路径**：它在拿到 region 锁之前就要判断「这块是不是 oversized」（oversized 直接返回
`None` 走 locked 路径）。那里**既没有 region 也没有堆引用**，per-heap 字段根本够不着。

所以是一个 `static AtomicUsize`，`vm_context/construct.rs` 写一次。多 VM 进程共享同一个
设置 —— 和它还是 `const` 的时候完全一样，不是新引入的限制。

**实测代价**：`z42c.semantics --release --no-incremental`，未武装默认路径，各三次取中位：

| | 指令 | 峰值 RSS |
|---|---|---|
| 基线 | 78.504 G | 902.96 MB |
| 本 change | 78.522 G (**+0.023%**) | 902.56 MB |

落在噪声里。#534 当时推后这个旋钮的顾虑（「给最热的分配路径加一次全局原子读」）
量出来是不成立的 —— 但当时也确实没有消费者，推后没错。

### Decision 3: `class_for` 拆出 `class_for_with_limit`

门槛是进程级的，一个直接 `set_loh_bytes` 再断言的测试会**和所有并发运行的、分配变长块的
测试竞争**（`cargo test` 默认多线程）。把纯逻辑抽成 `class_for_with_limit(payload, loh)`，
测试只调它；`clamp_loh_bytes` 同理。全局只在 VM 构造时被写一次。

### Decision 4（顺带）: 写屏障的闭包年龄口径

`maybe_mark_cross_gen_card` 读的是闭包 `env` **数组**的年龄，而 minor 标记阶段的判据
`gen_age_of(Value::Closure)` 读的是**闭包块自己**的年龄（#533 给它加的）。两者能不一致：
`env` 必然先于闭包块分配，因而永不更年轻 —— 于是一个「看起来老」的闭包写进老 owner
会跳过脏卡，而块本身还年轻。改成读同一个年龄。

（`fix-promotion-creates-uncarded-old-to-young` 论证过「闭包块永远不会比 env 老」，
那条论证保证的是**晋升**不会造出这个方向的边；这里修的是**写**的那一侧。）

## Testing Strategy

- `a_region_promotes_at_the_age_it_was_built_with` —— age 1/2/3 各自在第 age 次
  `promote` 才离开 young 表
- `a_lower_loh_threshold_makes_more_blocks_oversized` —— 40 KB payload 在 64K 门槛下
  是 in-chunk、在 32K 门槛下是 `OVERSIZED_CLASS`
- `the_loh_threshold_is_clamped_to_the_chunk_size` —— 纯函数，不碰全局
- `gc_promotion_age_parses_a_small_integer` / `gc_loh_bytes_parses_the_same_suffixes_as_gc_max_bytes`

**端到端**（`z42c.semantics`，gen + 128M）：

| 设置 | minor/major | 峰值 RSS |
|---|---|---|
| age=2（默认） | 15/1 | 608.2 MB |
| age=1 | 15/1 | 621.7 MB |
| age=3 | 15/2 | 618.5 MB |
| age=9 → 警告并 clamp 到 3 | 15/2 | 618.5 MB |
| LOH=32K | 15/1 | 604.4 MB |

旋钮都真的在动，且这个负载上差异都不大 —— 这也是本 change **不动任何默认值**的理由。
