# Tasks: 扩展运行时计数（P1a）

> 状态：🟢 已完成 | 创建：2026-08-22 | 完成：2026-08-23

## 进度概览
- [x] 阶段 1: HeapStats 分代字段 + 收集器埋点
- [ ] 阶段 2: 修正 counters.rs 陈旧注释（native/exceptions 埋点已在 2026-05-26 生效）
- [ ] 阶段 3: ProfileSnapshot 合并快照 + app.rs 输出
- [ ] 阶段 4: xtask profile 报告显示
- [ ] 阶段 5: 测试
- [x] 阶段 6: 文档同步 + 验证

> **scope 收窄（2026-08-23）**：核查发现 native/exceptions 三计数早已埋点生效（`exec_native.rs:40`、
> `interp/mod.rs:1007/1024`），仅 counters.rs 注释陈旧。原"补零埋点"→"修正注释"。核心交付（GC 维度
> 进 profile）不变。

## 阶段 1: HeapStats 分代字段 + 收集器埋点
- [x] 1.1 `gc/types.rs` HeapStats 加 `minor_collections`/`major_collections`/`reclaimed_bytes`（u64）+ 注释
- [x] 1.2 `gc/arc_heap.rs` 4 处收集路径：generational minor(+escalate major) / concurrent / collect_cycles / force_collect 各归类 bump + reclaimed
- [x] 1.3 `gc/heap_tests.rs` 补新字段默认 0 断言

## 阶段 2: 修正 counters.rs 陈旧注释
- [ ] 2.1 `counters.rs` 模块 doc + native_calls/exceptions_thrown/exceptions_caught 字段注释：改"Phase 2 待埋点"为"已埋点（位置 + 日期）"

## 阶段 3: ProfileSnapshot 合并快照
- [ ] 3.1 `counters.rs` 定义 `ProfileSnapshot`（counters + allocations + minor/major/reclaimed）+ `to_json()`（超集，`z42vm_counters` sentinel）+ `Display`
- [ ] 3.2 `counters.rs` tests 拆到 `counters_tests.rs`（`#[cfg(test)] mod counters_tests;`）—— 独立 refactor 小步
- [ ] 3.3 `app.rs` 快照输出点：`ProfileSnapshot::from(counters snapshot, ctx.heap().stats())`，text/json 两路

## 阶段 4: xtask profile 报告
- [x] 4.1 `scripts/xtask_profile.z42` `_counterSummary` 扩展：展示 exc_caught/alloc/gc_minor/gc_major/reclaimed
- [ ] 4.2 用种子 z42c 重建 xtask.zpkg，`xtask profile -h` 接线核对

## 阶段 5: 测试
- [x] 5.1 `counters_tests.rs` ProfileSnapshot::to_json/Display/default 单测（全键 + sentinel + 单行 + 超集不漂移）
- [x] 5.2 收集器计数集成测试：`config_stats.rs` major_collections（非分代）+ `safepoint_tests.rs` generational minor
  （原计划 e2e golden 放弃——golden harness 只 diff stdout、不设 --print-stats，无法断言 stderr 计数 JSON）
- [x] 5.3 `heap_tests.rs` 新字段默认 0（阶段 1.3 已含）
- [x] 5.4 经验验证（开发期）：seed 编脚本 → 新 VM --print-stats-on-exit，确认 exc=5/5 + allocations=200046 + 新键齐全

## 阶段 6: 验证 + 文档
- [x] 6.1 `cargo build --manifest-path src/runtime/Cargo.toml --release` —— 通过（仅 pre-existing warning）
- [x] 6.2 `cargo test --lib` 全量 —— 943 + 21 passed, 0 failed（含新增 counters/gc 测试）
- [x] 6.3 完整 `xtask test` —— ✅ GREEN all stages + z42c self-host 5/5 gen1==gen2（种子=源=0.41，未撞格式墙）
- [x] 6.4 spec scenarios 逐条覆盖确认（分代拆分/合并 JSON/native·exc 回归护栏均有测试或经验证据）
- [x] 6.5 文档同步：diagnostics.md §5（gc/README 无需改——未加文件、不枚举 stat 字段；profile 报告为 xtask_profile.z42 自文档）
- [x] 6.6 diagnostics.md 无「对齐」页头（design 文档非 book 页）→ N/A

## 备注
- allocations 不新增计数、复用 HeapStats.allocations（Decision 1）。
- 精确次数 gate 不在本 change（P1c informational）。
