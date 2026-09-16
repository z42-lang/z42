# Proposal: 调查 ConcurrentMarkSweep 残留 mark bit race

## Why

`concurrent_gc_mode_stress_no_race_no_leak` 在本地 baseline (any branch, no
local mods) 上 100% 失败，panic 信息：

```
thread 'concurrent_gc_mode_stress_no_race_no_leak' panicked at src/gc/arc_heap.rs:444:17:
stale mark bit in region_object after sweep: chunk=0, entry=<varies>
```

变体（来自 3 次连续重跑）：

1. `chunk=0, entry=16`
2. `chunk=0, entry=26`
3. `mark_queue stale post-collect: 2 entries remaining` (line 433)

这条 invariant 由 `ArcMagrGC::debug_validate_invariants` ([`arc_heap.rs:439-460`](../../../src/runtime/src/gc/arc_heap.rs#L439))
强制：sweep 跑完后，**任何 alive entry 都不能再有 marked=1**，且 mark_queue
必须空。当前观察：有 region_object alive entry 在 sweep 后仍带 mark bit，
或 mark_queue 有 leftover 条目。

引入该测试的 commit：[`9f461ebc`](https://github.com/...) test(gc):
add-concurrent-gc P5 — multi-mutator stress under concurrent mode (2026-05-22)。
该 commit 描述 "Runtime-scope test-all GREEN. 10/10"，说明加入时跑得过。
之间到 2026-05-26 之间某个 GC 改动让它退化。

不修：runtime-scope test-all 永远红，blocks 任何要求 GREEN 的迭代；用户每次
都得手动豁免这一项 pre-existing failure。

## What Changes

- 二分定位 9f461ebc 之后导致此测试退化的具体 commit
- 根据该 commit 的语义分析根因（mark bit 重置遗漏 / handshake 顺序 bug / 
  region iterator 漏 alive entry / 等）
- 在 `gc/` 内修正（不在 invariant 上"放宽"绕过）

## Scope（待 design 阶段定）

待二分定位后再确定具体改动文件。**只读引用**：

- [`src/runtime/src/gc/arc_heap.rs`](../../../src/runtime/src/gc/arc_heap.rs) — debug invariant + sweep 主路径
- [`src/runtime/src/gc/concurrent.rs`](../../../src/runtime/src/gc/concurrent.rs) — 并发 collect cycle 状态机
- [`src/runtime/src/gc/mark.rs`](../../../src/runtime/src/gc/mark.rs) — mark phase
- [`src/runtime/tests/cross_thread_smoke.rs`](../../../src/runtime/tests/cross_thread_smoke.rs) — 触发测试
- `add-concurrent-gc` archive 下的 spec/design — 上下文

## Out of Scope

- 重新设计并发 GC 算法（本 spec 只修 race，不做架构改造）
- 关闭 / 标记 `#[ignore]` 测试绕过（违反 [`philosophy.md`](../../../agent/rules/philosophy.md) "修复必须从根因出发"）

## Open Questions

- [ ] 二分定位：9f461ebc..HEAD 之间，哪个 commit 让此测试退化？
- [ ] 根因是 mark bit reset 漏 / handshake 顺序 / region iterator 边界 / 其他？
- [ ] 修完后是否需要把 `debug_validate_invariants` 在每次 collect 后强制开（而非 test-only）？
