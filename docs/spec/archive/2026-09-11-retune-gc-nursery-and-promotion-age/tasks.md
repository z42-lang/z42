# Tasks: retune-gc-nursery-and-promotion-age

> 状态：🟢 已完成 | 创建：2026-09-11 | 完成：2026-09-11

**变更说明：** `DEFAULT_NURSERY_BYTES` 32M → **16M**，`PROMOTION_THRESHOLD` 2 → **3**，
**两个一起改**。附带新增 perf scenario `12_gc_churn` —— 全树唯一会让 GC 真正回收东西的负载，
也是这次定默认值的第二个证据来源。

**原因：** nursery 是「买停顿上界」的那个旋钮（minor 只扫年轻代）。诚实闸门下重测档位表，
16M 在真实负载上是白送的。但**单独**调小 nursery 会踩过早晋升：minor 会把熬过它的东西晋升，
nursery 减半 ⇒ 对象只要多活一半的分配量就被晋升 ⇒ 本该死在年轻代的被搬进老年代，而那里
只有 major 能回收。所以晋升年龄必须跟着上去。

**文档影响：** book「nursery 与晋升年龄是一对 —— 过早晋升」一节 + 旋钮表两行默认值；
`DEFAULT_NURSERY_BYTES` / `PROMOTION_THRESHOLD` / `var_region/block.rs` / `config.rs` /
`knob_table.rs` 的相关陈述。

## 任务
- [x] 1.1 `src/tests/perf/scenarios/12_gc_churn.z42`：按分代假说造流失形状
      （短命 ~87% / 中年进环后成垃圾 ~12% / 长寿 ~0.02%，含老→新写屏障两条路径）；
      期望输出**手算对账** 144 151 352，与实跑一致
- [x] 1.2 `DEFAULT_NURSERY_BYTES` 32M → 16M
- [x] 1.3 `PROMOTION_THRESHOLD` 2 → 3
- [x] 1.4 六个把「两次 promote = 老年代」写死的测试改成**与阈值无关**
      （`promote_to_old` 助手 + `for _ in 0..PROMOTION_THRESHOLD`）
- [x] 1.5 文档同步
- [x] 1.6 GREEN —— `xtask test` 全绿

## 验证

**三个负载、两个二进制（base = `e890d369` / 本 PR）、各三跑。** base 用真实默认值，不是环境变量。

| | `09_alloc_ctorless` 墙钟 | `12_gc_churn` RSS / p90 | `z42c.semantics` 墙钟 / RSS / p90 |
|---|---|---|---|
| 32M 年龄2（base） | 0.38 s | 184 MB / 29.7 ms | 7.00 s / 628 MB / 23.6 ms |
| 24M 年龄3 | 0.36 s | 186 MB / 21.0 ms | 6.91 s / 617 MB / 18.0 ms |
| **16M 年龄3（本 PR）** | 0.45 s | **142 MB / 16.4 ms** | 7.19 s / **595 MB / 14.1 ms** |

p90 停顿 **−40%～−45%**、峰值 RSS **−5%～−23%**，代价墙钟 +2.7%～3.3%。
`01_fibonacci` / `03_startup`（不分配）无变化。

### ⚠️ `09_alloc_ctorless` 回归约 18%，是**有意接受**的

它是 100% 存活的病理形状：分配的东西一个都不死，每次回收都白干，而 nursery 变小让它在
徒劳退避饱和之前多塞进一次回收（24M 下 2 次、16M 下 3 次；20M 实测也是 3 次，悬崖在
20M↔24M 之间）。24M 能躲开、且在每个负载上都小赚；16M 付掉它、换其余负载上约两倍的停顿收益。

**这个取舍已明确提交 User 裁决，User 选择 16M。** 这条线是停顿线，而回归发生在一个
「回收本来就不可能有用」的合成负载上。bench 门禁大概率会在这条上判红 —— 那是**真阳性**，
不要当噪声处理。

## 🔑 为什么两个默认值必须一起动

`12_gc_churn` 上单独把 nursery 调到 16M：晋升率从 **17.9% 升到 33.6%**（684 444 → 1 326 948 个，
扫描量同为约 390 万），峰值 RSS 从 198 MB **涨到 405 MB**。年龄提到 3 之后同一档是 **142 MB**,
比它取代的 32M 默认还低。

⚠️ **3 是上界**：`gen_age` 打包在 `GcBlockHeader::type_tag` 的两个空闲位里
（`MAX_GEN_AGE = 3`，`var_region.rs` 有静态断言）。所以 `Z42_GC_PROMOTION_AGE`
从此**只能调低、不能调高**；再往上要先给 age 找到那一位。

## 备注
新 benchmark 在 bench 门禁里标 `(new)`、不参与判红，直到有基线。
