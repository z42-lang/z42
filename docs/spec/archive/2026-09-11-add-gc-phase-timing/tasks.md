# Tasks: add-gc-phase-timing

> 状态：🟢 已完成 | 创建：2026-09-11 | 完成：2026-09-11

**变更说明：** 新增 `Z42_GC_PHASES` 旋钮——把一次 GC 停顿拆成各阶段的耗时 + 条目数，
外加一行「这次回收是被哪个闸门触发的」，都打在 stderr。关掉时零成本。

**原因：** `Z42_GC_TRACE` 说得出「这次停了 64.6 ms」，说不出这 64.6 ms 花在哪、以及**为什么
这次要扫这么大一片年轻代**。整条 GC 停顿线（#565 / #566 / #569 / #570）每一次定位都靠同一个
手打补丁：给每个阶段套一个 env 门控的计时器。量完就删，下次重打——四次了。这次把它固定下来。

**文档影响：** `docs/book/src/runtime/gc-tuning-and-safepoint.md` 新增「诊断旋钮」一节。

## 任务
- [x] 1.1 `gc/phase_timer.rs`：`PhaseTimer` RAII 计时器 + `note()` 自由文本行 + `phases_enabled()`
- [x] 1.2 `gc/phase_timer_tests.rs`：行格式与对齐的单测
- [x] 1.3 `config.rs` / `config/knob_table.rs`：登记 `Z42_GC_PHASES`（Bool，`DEBUG_KNOB` 档）
- [x] 1.4 `gc/trace.rs`：`human()` 提升为 `pub(crate)`，两处字节数用同一种写法
- [x] 1.5 major 路径打点（`arc_heap/control.rs` + `arc_heap/collect.rs`）：
      reset marks / full mark / sweep 的四个半程 / sweep/var / sweep/chunk reclaim / age survivors
- [x] 1.6 minor 路径打点（`arc_heap/generational.rs`）：
      minor mark / scan·promote·tomb × {objects, arrays} / var sweep / chunk reclaim
- [x] 1.7 `arc_heap/auto_collect.rs`：trip 行（哪种回收、闸门多大、退避倍数、实际长了多少）
- [x] 1.8 文档同步：book 的「诊断旋钮」一节
- [x] 1.9 GREEN —— `xtask test` 全绿（13 stage，3m13s）

## 验证
关掉是默认。`z42c.semantics --release --no-incremental`，开关各两跑：

| | wall | 停顿合计 | 中位 | 最大 |
|---|---|---|---|---|
| off | 6.99 s / 7.16 s | 274.6 / 283.5 ms | 21.6 / 21.2 ms | 64.6 / 65.1 ms |
| on  | 7.13 s / 7.10 s | 278.4 / 274.7 ms | 21.8 / 21.7 ms | 64.8 / 64.1 ms |

噪声内——计时器关掉时不取时钟，开销就是每阶段一个 `None` 判断。

## 备注
这个工具第一次跑就交了一份答卷（**属下一个 change，不在本 PR 范围内**）：
`trip minor  gate 32.0M x4  grown 160.0M` —— 徒劳退避把 **minor** 的闸门乘到了 128M，
于是那两次 minor 要啃 160 MB 的年轻代，停顿 45.4 / 64.6 ms，而闸门正常的 minor 只要 6–22 ms。
