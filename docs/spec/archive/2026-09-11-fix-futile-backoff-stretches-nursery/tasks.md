# Tasks: fix-futile-backoff-stretches-nursery

> 状态：🟢 已完成 | 创建：2026-09-11 | 完成：2026-09-11

**变更说明：** 自动回收的徒劳退避倍数改为**只在设了软上限（`Z42_GC_MAX_BYTES`）时生效**。

**原因：** 没有软上限时，倍数唯一够得着的闸门是 **nursery**——而 nursery 不是内存闸门，是
「一次 minor 要啃多大一片年轻代」的上界，也就是停顿上界。实测 `z42c.semantics`：两次分别
只回收了 8.4 MB / 10.1 MB 的回收把倍数推到 4，紧随其后的两次 minor 扫了 96 MB / 160 MB 的
年轻代、停了 **45.4 / 64.6 ms**，占整次构建停顿的 39%，而同一跑里闸门正常的 minor 只要
6–22 ms。且那两次回收本就不算徒劳——那是**高存活率**的样子，对它的正确回应是升级成 major
（`minor_escalation_threshold` 早就在做），不是把 nursery 养大。

没有软上限时也没有失控可拦：余量是 `live × 0.33`，随活集合一起长，回收次数对堆增长已经是
对数的——这正是模块文档自己写的「退避如今只是软上限把余量压到下界那一档的兜底」。

**文档影响：** `docs/book/src/runtime/gc-tuning-and-safepoint.md` 新增「徒劳退避只在有软上限
时生效」一节；`DEFAULT_NURSERY_BYTES` 的 nursery 档位表加一条「该表测于本修复之前」的更正。

## 任务
- [x] 1.1 `effective_backoff(backoff, soft_limit)`：无软上限 → 1
- [x] 1.2 在 **两个消费点**（`decide_trip` / `arm_next_collect`）各自应用，而不是在调用方中和
      ——调用方中和会在出现第三个消费者时静默失效
- [x] 1.3 `next_backoff`：无软上限时不再累加倍数（不往状态里存一个没人读的谎）
- [x] 1.4 回归测试三条（`auto_collect_tests.rs`）+ 两头反证
- [x] 1.5 文档同步（book 一节 + nursery 档位表更正 + 模块文档）
- [x] 1.7 顺带修好 `gc-tuning-and-safepoint.md` 的页头「对齐」块 —— 某次合并把两条 `> 对齐：`
      串成了一段，前一条在「修软上限在分代下不被」处被截断、后一条把它的下半句又说了一遍。
      本 change 本来就要改这个页头（加自己的对齐条目），顺手理成一条倒序清单。**经 User 明确
      授权**（否则按规矩不顺手修 Scope 外问题）。
- [x] 1.6 GREEN —— `xtask test` 全绿。顺带一条：`gc modes` stage 的默认腿（nursery 1M）
      从约 440 次回收变成 **633 次**——那条腿正是用回收次数换「过早回收」缺陷检出率的，闸门
      变诚实等于它的覆盖变密，且仍 `clean`（无悬垂引用）。

## 验证
`z42c.semantics --release --no-incremental`，**两个二进制**各三跑（base = `add-gc-phase-timing`
那个提交编出来的）：

| | 墙钟 | 峰值 RSS | 回收次数 | 停顿合计 | 中位 | p90 | 最大 |
|---|---|---|---|---|---|---|---|
| base | 6.78 / 6.79 / 6.79 s | 767 MB | 11 | 266 ms | 20.7 ms | 45.8 ms | 60.8 ms |
| fix  | 6.87 / 6.78 / 6.82 s | **583 MB** | 17 | 275 ms | **16.0 ms** | **26.4 ms** | **44.1 ms** |

p90 **−42%**、最大 **−27%**、中位 **−23%**、峰值 RSS **−24%**（−184 MB）；
代价墙钟 +0.4%、停顿合计 +3.4%（多 6 次回收的固定开销）。

两头反证：把 `effective_backoff` 的判据去掉 → 新增的两条 uncapped 测试立刻变红，
`a_capped_gate_is_still_stretched_by_the_futility_multiplier` 与既有的
`an_over_budget_live_set_does_not_re_collect_forever` 保持绿（belt 没被动到）。

## 备注
**后续（不在本 change 内）**：`DEFAULT_NURSERY_BYTES` 的 32M 默认值，它当初的两条理由如今
都已过期——「minor 还有一趟 O(堆) 的 chunk 回收」在 add-incremental-chunk-reclaim 之后已是
`O(chunks)`，「退避会撑大它」被本 change 修掉。档位表自那之后没人重测过。
