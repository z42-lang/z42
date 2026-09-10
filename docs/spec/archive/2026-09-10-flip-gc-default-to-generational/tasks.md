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
- [x] 1.7 GREEN（`./xtask test` 全绿；`cargo test --lib` **debug** 1216 passed）
- [x] 1.8 修 CI 报出来的两个真缺陷（minor+major 同 pause / major 不升龄）+ 退避调优 + 两个回归测试
- [x] 1.9 修三个违反「调用方先过滤基元」契约的写屏障单测

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

## ✅ bench 回归：查清并修掉了（原 +85%，现 −11%）

第一版 PR 让三条 bench 翻红。查下来**不是分代的固有代价，是两个真缺陷**：

### 缺陷 1：一次 pause 里既跑 minor 又跑 major

`collect_cycles_with_context` 的升级启发式注释写着 `// Major in same pause window.` ——
minor 刚做完，紧接着在**同一个 pause** 里跑一次完整 major。major 是全堆标记 + 全区清扫，
minor 做的它全包含，所以那次 minor 是纯白干。`want_major` 那条路径也一样：想要 major 时仍然
先跑一遍 minor。

`09_alloc_ctorless` 上升级**每个周期都触发**（存活率 100%），实测每周期
`minor 156.7 ms + major 188.5 ms` = 370 ms 停顿、free 0 字节。

**改法**：想要 major 就只跑 major；升级把 major 排到**下一个**周期（`pending_major`），
而不是叠在刚做完的 minor 上。

### 缺陷 2：major 不给幸存者升龄

升龄是唯一能排空 young 表的东西。以前 major 前面永远有一个 minor 替它升龄；缺陷 1 修完
major 可以单独跑，于是**所有存活条目都留在 young 表里** —— 下一次 minor 直接重标全堆
（实测 1 498 866 条、192.6 ms）。

**改法**：major 尾部补一次升龄（`age_survivors_after_major` + `VarRegion::age_young_survivors`），
放在 `rebuild_card_table` **之前**（晋升产生 old→young 边，rebuild 负责记录它们）。
语义上也更对：熬过一次 major 和熬过一次 minor 是同样强的长寿证据。

### 缺陷 3（调优）：「白干一场」和「回收了不到一半」退避力度相同

两者性质不同：白干一场意味着活集根本不产生垃圾，而下一次回收要多标记整整一个闸门的对象、
回报仍是零。改成 `reclaimed < gate/16` → ×4，`< gate/2` → ×2。

### 实测（本地双二进制 A/B，base = `873455a2`，warmup 3 / runs 10）

| 场景 | 修之前 | 修之后 |
|---|---|---|
| **09_alloc_ctorless** | **1.93×** | **0.885×**（比 STW 还快） |
| 07_string_heavy | 1.03× | 1.008× |
| 08_dict_heavy | — | 1.020× |
| 05_polymorphic_dispatch | — | 1.005× |
| 01_fibonacci | — | 1.033× |
| 10_mono_vcall | — | 1.002× |

**编译器负载的收益一分没丢**：RSS 778.4 MB（stw 952.4，**−18.2%**）、
中位停顿 20.8 ms（stw 57.1，**−64%**）。

⚠️ **`07_string_heavy [interp]` 在 CI(linux-x64) 上曾报 1.36×，本地（macOS arm64）
用双二进制 A/B 只有 1.03×，且该场景两种模式下都是 0 次回收** —— 本地无法复现，
判断是代码布局效应。修完上面三条后本地 1.008×，等 CI 复验。

## 备注## 备注

- **CI**：`gc modes` stage 跑两条腿 —— 默认（**`Z42_GC_MODE` 不设**，nursery 1M，~440 次回收）
  + `stw`（budget 32M，~67 次）。默认那条**故意不写死模式**：它测的就是「默认是什么」。
  两条腿都断言回收次数下界，默认被改或策略让收集器不再触发时会红，而不是继续绿着什么都没测。
  stage 从 15.0 s 涨到 24.0 s，整个 gate 3m05s → 3m16s。
- ⚠️ **zsh 不对未加引号的参数展开分词**，量 GC 前用 `Z42_GC_TRACE=1` 数一下回收次数
  （这条路径的 kind 打的是 `Cycle`）。
