# Tasks: 调查 ConcurrentMarkSweep 残留 mark bit race

> 状态：🟡 进行中（根因已定位；正确修复需 loom，阶段 3 开工）| 创建：2026-05-26 | 更新：2026-07-08

## 进度概览
- [x] 阶段 1: ~~二分定位退化 commit~~ → 改为代码级根因分析（见 design.md）
- [x] 阶段 2: 根因分析（design.md：注册→首safepoint 窗口；本地不可复现）
- [x] 阶段 2.5: 过渡解封 CI —— windows `#[ignore]`（User 2026-06-01 豁免 philosophy 禁 skip）
- [x] 阶段 2.6: **平台不对称前提失效（2026-07-08）** —— `test runtime` 全腿放开后，
      `macos-arm64` CI 首次真跑即复现同一 stale-mark race（design 原判"ARM/macOS 通过、
      windows-only"不成立）。ignore 扩到 `any(windows, macos)`（User 2026-07-08 豁免）。
      linux（x64/arm64）本轮仍过，暂留观察——若后续 flaky 再扩。
- [ ] 阶段 3: loom/shuttle 确定性验证 + 正确修复 + 回归（**开工中**）

## 阶段 1: 二分定位

- [ ] 1.1 `git bisect start HEAD 9f461ebc` 起点
- [ ] 1.2 每个 candidate commit 跑 `cargo test --test cross_thread_smoke concurrent_gc_mode_stress_no_race_no_leak`（连跑 3 次确认稳定 pass/fail）
- [ ] 1.3 记录 first-bad commit，写入 design.md

## 阶段 2: 根因分析

- [ ] 2.1 读 first-bad 的 diff，对照 arc_heap.rs sweep + concurrent.rs collect_cycle 状态机
- [ ] 2.2 写 design.md 提两个候选根因 + 推荐
- [ ] 2.3 User 确认根因后进阶段 3

## 阶段 3: loom 验证 + 正确修复

- [ ] 3.1 引入 loom/shuttle，对 alloc / write_barrier / handshake / new_with_core 注册的线程交错建模型测试，确定性复现 race（windows）+ deadlock（注册封闭尝试）
- [ ] 3.2 在模型下设计修复：注册—首safepoint 窗口封闭 + marking 期 allocate-black + 不破坏 collector 仲裁时序（绝不放宽 invariant）
- [ ] 3.3 cargo test —— 全绿（含本测试在 windows 重新启用、稳定 10/10）
- [ ] 3.4 移除 cross_thread_smoke.rs 上的 windows `#[ignore]`（过渡撤销）
- [ ] 3.5 docs/design/runtime/vm-architecture.md 或 GC 专章追加"并发 mark bit 生命周期 + 注册/safepoint 协议"说明

## 备注
- 过渡（阶段 2.5）：`concurrent_gc_mode_stress_no_race_no_leak` 已 windows `#[ignore]`；在 linux/macOS 仍跑（守护 invariant）。
- 盲推已被证伪：park-on-registration fix deadlock 了 `second_collector_falls_back_to_mutator_park_returns_none`（见 design.md "尝试记录"）。修复必须在 loom 下验证。
