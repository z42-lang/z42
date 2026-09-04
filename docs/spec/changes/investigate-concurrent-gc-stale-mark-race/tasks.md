# Tasks: 调查 ConcurrentMarkSweep 残留 mark bit race

> 状态：🟡 进行中（根因已定位；race 与 deadlock 均已 loom 确定性建模，修复待做）| 创建：2026-05-26 | 更新：2026-09-04

## 进度概览
- [x] 阶段 1: ~~二分定位退化 commit~~ → 改为代码级根因分析（见 design.md）
- [x] 阶段 2: 根因分析（design.md：注册→首safepoint 窗口；本地不可复现）
- [x] 阶段 2.5: 过渡解封 CI —— windows `#[ignore]`（User 2026-06-01 豁免 philosophy 禁 skip）
- [x] 阶段 2.6: **平台不对称前提失效（2026-07-08）** —— `test runtime` 全腿放开后，
      `macos-arm64` CI 首次真跑即复现同一 stale-mark race（design 原判"ARM/macOS 通过、
      windows-only"不成立）。ignore 扩到 `any(windows, macos)`（User 2026-07-08 豁免）。
      linux（x64/arm64）本轮仍过，暂留观察——若后续 flaky 再扩。
- [ ] 阶段 3: loom/shuttle 确定性验证 + 正确修复 + 回归（**开工中**；3.1/3.1b 已完成，两个 hazard 中的
      「注册窗口」与「仲裁死锁」均已确定性建模；余 3.1c allocate-black 建模 + 3.2 起的正式修复）

## 阶段 1: 二分定位

- [ ] 1.1 `git bisect start HEAD 9f461ebc` 起点
- [ ] 1.2 每个 candidate commit 跑 `cargo test --test cross_thread_smoke concurrent_gc_mode_stress_no_race_no_leak`（连跑 3 次确认稳定 pass/fail）
- [ ] 1.3 记录 first-bad commit，写入 design.md

## 阶段 2: 根因分析

- [ ] 2.1 读 first-bad 的 diff，对照 arc_heap.rs sweep + concurrent.rs collect_cycle 状态机
- [ ] 2.2 写 design.md 提两个候选根因 + 推荐
- [ ] 2.3 User 确认根因后进阶段 3

## 阶段 3: loom 验证 + 正确修复

- [x] 3.1 引入 loom + 对 register→handshake→barrier→sweep→validate 交错建模，**确定性复现 race**。
      **已落地**（2026-07-08）：`src/runtime/tests/gc_registration_race_loom.rs`（`#![cfg(loom)]`，
      `[target.'cfg(loom)'.dev-dependencies] loom`）。`race_reproduces_without_registration_close`
      在 0.01s 本地必现 "stale mark bit … after sweep"——把 design 判为"硬件不可本地复现"的 race
      变成**确定性本地测试**。
- [x] 3.1b **collector 仲裁建模（模型 B）**（2026-09-04）。同一文件加了第二个模型，复刻
      `second_collector_falls_back_to_mutator_park_returns_none`：`registration_close_reintroduces_2026_06_01_deadlock`
      **确定性复现** 2026-06-01 的 deadlock，对照组 `arbitration_baseline_has_no_deadlock`
      在**穷举**搜索（34 条交错）下绿——同一模型翻一个开关即分开两侧，模型有判别力。
      产出的硬约束：**注册窗口封闭不得把任何 context 的 park 移到 collector 仲裁 CAS 之前**。
      细节 + loom 0.7.2 的 abort 陷阱见 design.md「更新 2026-09-04」。
- [ ] 3.1c **marking 期 allocate-black 建模**（下一增量）：`alloc_object` 出生 marked=0，
      只关注册窗口的 fix 仍会让可达新对象被进行中的 cycle sweep 掉。模型 A/B 都不覆盖这一族。
- [ ] 3.2 在模型下设计修复：注册—首safepoint 窗口封闭 + marking 期 allocate-black + 不破坏 collector 仲裁时序（绝不放宽 invariant）——**先补 deadlock 模型**再验证
- [ ] 3.3 cargo test —— 全绿（含本测试重新启用、稳定）
- [ ] 3.4 移除 cross_thread_smoke.rs 上的 `#[ignore]`（过渡撤销）
- [ ] 3.5 docs/design/runtime/vm-architecture.md 或 GC 专章追加"并发 mark bit 生命周期 + 注册/safepoint 协议"说明

## 备注
- 过渡（阶段 2.5）：`concurrent_gc_mode_stress_no_race_no_leak` 已 windows `#[ignore]`；在 linux/macOS 仍跑（守护 invariant）。
- 盲推已被证伪：park-on-registration fix deadlock 了 `second_collector_falls_back_to_mutator_park_returns_none`（见 design.md "尝试记录"）。修复必须在 loom 下验证。
- 该 deadlock 自 2026-09-04 起是**本地确定性测试**（模型 B），不再依赖跑真单测撞上它。
