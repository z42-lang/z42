# Tasks: flip-gc-default-to-generational

> 状态：🟢 已完成 | 创建：2026-09-10 | 完成：2026-09-10

**变更说明：** `GcMode::default()` 由 `StwMarkSweep` 改为 `GenerationalMarkSweep`
（`Z42_GC_MODE` 不设时的默认）。顺带修一个只有在分代当默认时才暴露的缺陷：软上限在分代下不被执行。

**原因：** User 裁决（口令『翻 GC 默认』）。前置由 #552 / #553 / #555 / #557 清空，
翻默认这件事第一次是净赢。

**文档影响：** `docs/book/src/runtime/gc-tuning-and-safepoint.md`（旋钮表默认值 + 新增
「为什么分代成了默认」+ CI 覆盖一节改写）、`docs/book/src/dev/test-gate.md`（stage 改名）、
`config/knob_table.rs` 的 `default_hint`。

- [x] 1.1 `gc/mode.rs`：`Default` → `GenerationalMarkSweep`；模块文档写清依据
- [x] 1.2 `config/parse.rs`：无法识别的值回退到 `GcMode::default()`，不再硬编码 `stw`
- [x] 1.3 **修软上限在分代下不被执行**（见下）+ 回归测试（两头反证过）
- [x] 1.4 16 个假设「默认是 STW」的单测：该测 STW 的显式 `set_mode`，该测默认的改断言
- [x] 1.5 CI stage `gc generational` → `gc modes`：跑两条腿 + 断言回收次数下界
- [x] 1.6 文档同步（book 两页 + knob_table）
- [x] 1.7 GREEN（`./xtask test` 全绿；`cargo test --lib` 1164 passed）

## 实测（`z42c.semantics --release --no-incremental`，各 3 跑）

| | 墙钟 | 峰值 RSS | 回收次数 | 中位停顿 | 最大停顿 |
|---|---|---|---|---|---|
| `stw`（旧默认） | 6.52 s | 949 MB | 3 | 59.4 ms | 84.8 ms |
| **`generational`（新默认）** | 6.74 s（**+3.4%**） | **777 MB（−18.1%）** | 16 | **20.5 ms（−65%）** | 85.5 ms |

🔑 **和 memory 里记的「+2.3% RSS」差很多，差的那一栏不是分代变强了，是 STW 变差了**：
`arm-gc-by-default` 之后不设预算的 STW 只跑 3 次 full collection（余量 `live × 0.33` 随活集
一起长），而分代的 minor 闸门是**绝对**的一个 nursery。**「谁更省内存」在武装策略换成相对
余量之后才倒过来 —— 别拿更早的数据推论。**

## 🔴 顺带发现：软上限在分代下根本不被执行（本 change 一并修了）

分代的 minor 闸门是 nursery（默认 32M 绝对值，刻意与预算无关），于是一个远小于 nursery 的
`Z42_GC_MAX_BYTES` **一次都不会触发回收** —— `next_collect_at` 停在 `live + 32M`，
策略在堆冲过上限之前没被问过（`decide_trip` 本来会因 `near_cap` 要一次 major，只是没人问它）。

修法：minor 闸门改成 `min(nursery, allowance)`，allowance 正是软上限压的那个量。
**没设上限时 allowance ≥ 4 个 nursery，`min` 恒等于 nursery，默认路径逐字节不变。**
STW 那侧的闸门本来就是 allowance，所以这个洞在它当默认时看不见。
回归测试 `generational_enforces_a_soft_cap_far_below_one_nursery`（两头反证：去掉修复即红）。

## ⚠️ 一个必须让 User 看见的回归：`09_alloc_ctorless` +85%

| 场景（interp，hyperfine warmup=2 runs=8） | stw（旧默认） | gen（新默认） | Δ |
|---|---|---|---|
| 07_string_heavy | 50.3 ms | 49.9 ms | −0.8% |
| 08_dict_heavy | 56.7 ms | 55.3 ms | −2.6% |
| 05_polymorphic_dispatch | 1093.7 ms | 1104.7 ms | +1.0% |
| **09_alloc_ctorless** | **457.5 ms** | **848.4 ms** | **+85.4%** |

**成因（已查清，不是缺陷）**：这个场景**存活率 100%** —— 每次回收 `freed` 都是 0～384 B。
STW 的闸门 `live × 0.33` 随活集一起长，所以只跑 1 次（150 ms）；分代的 minor 闸门是绝对的
一个 nursery，跑了 3 次（45 + 122 + 375 ms，每次都是 O(活堆) 且活堆在长）。
徒劳退避确实在起作用（闸门 32M→64M→128M，否则次数会多得多），但消不掉。

**排除项**：`Z42_GC_MINOR_THRESHOLD=1.0`（禁掉升级 major）只从 0.87 s 到 0.82 s —— **不是升级启发式**。

**能不能靠调 nursery 救**：能，但要拿默认的好处去换 ——

| nursery | 09_alloc_ctorless | z42c.semantics RSS | z42c.semantics 中位停顿 |
|---|---|---|---|
| **32M（默认）** | 0.87 s | **766 MB** | **20.7 ms** |
| 64M | 0.76 s | 801 MB | 30.1 ms |
| 128M | 0.62 s | 984 MB（≈ 退回 STW） | 80.8 ms |

所以 32M 保持不变，`09` 的回归**建议接受**：它是「分配一大堆全都活着」的合成最坏情况，
对任何按 nursery 触发的 minor 都成立；单个程序想要旧行为设 `Z42_GC_NURSERY_BYTES` 或
`Z42_GC_MODE=stw` 即可。**bench-PR 门禁大概率会标红这一条，需要 User 裁决是接受还是另作处理。**

## 备注

- **CI**：`gc modes` stage 跑两条腿 —— 默认（**`Z42_GC_MODE` 不设**，nursery 1M，~440 次回收）
  + `stw`（budget 32M，~67 次）。默认那条**故意不写死模式**：它测的就是「默认是什么」。
  两条腿都断言回收次数下界，默认被改或策略让收集器不再触发时会红，而不是继续绿着什么都没测。
  stage 从 15.0 s 涨到 24.0 s，整个 gate 3m05s → 3m16s。
- ⚠️ **zsh 不对未加引号的参数展开分词**，量 GC 前用 `Z42_GC_TRACE=1` 数一下回收次数
  （这条路径的 kind 打的是 `Cycle`）。
