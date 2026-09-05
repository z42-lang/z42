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
      三个 hazard（注册窗口 / 仲裁死锁 / 新对象被 sweep）**均已确定性建模**；余 3.2 起的正式修复）

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
- [x] 3.1c **marking 期 allocate-black 建模（模型 C）**（2026-09-04）：新文件
      `src/runtime/tests/gc_alloc_black_loom.rs`，建模并发 cycle 全流程
      （snapshot → yield → handshake → sweep）。**穷举 2105 条交错**，三档策略把边界钉死：
      `Never`（今日生产）→ 可达新对象被 sweep；`ConcurrentOnly` → **仍然**被 sweep；
      `ConcurrentAndMarking`（design 的提法）→ 绿。
      ⇒ design 里 `phase ∈ {ConcurrentMarking, Marking}` 从**断言**变成了**已证必要**。
      顺带确认该 hazard **不依赖注册窗口 race、也不依赖任何候选 fix**（见 design.md
      「更新 2026-09-04（二）」）。
- [~] 3.2 在模型下设计修复（拆成 3.2a/3.2b，**三个模型直接当门禁**）。
- [x] 3.2a **marking 期 allocate-black —— 已落地**（2026-09-04）。`ArcMagrGC::alloc_black`
      在并发 cycle 的 `request_gc_pause` → `sweep_phase` 结束之间为真（这个跨度**必须**含
      `Marking`，由模型 C 证明），期间所有分配出生即 marked=1。
      落点是**全部 5 个分配 chokepoint**：`finish_alloc` / `alloc_array_obj` 各自的 TLAB
      快路径与 ambient 路径，加上 `acquire_var_block`（strings / closures / 数组载荷这些
      `region_var` 块同样被 `VarRegion::sweep` mark-sweep）。热路径代价 = 一次 relaxed load，
      且在默认 `StwMarkSweep` 下恒为 false。
      回归测试：`arc_heap_tests::concurrent_mark::allocate_black_keeps_an_object_that_becomes_a_root_after_the_snapshot`
      与 `..._covers_var_region_blocks` —— **单线程确定性**（这个 hazard 不是 race），
      手工驱动 snapshot→分配→drain→sweep。**已做变异验证**：把 `allocating_black()` 改成
      恒 false，两条测试都红（`left: 1`，新对象被 sweep 掉），确认不是空过。
- [ ] 3.2b **注册—首 safepoint 窗口封闭** —— 未做，比预想难，见 design.md
      「更新 2026-09-04（三）」记录的三条硬事实（barrier 是 post-write / born-parked 只是
      把窗口挪了个位置 / 2026-06-01 的 deadlock 是一个**先于本 fix 存在**的隐患的症状）。
      需要 User 裁决方向后再动。
- [ ] 3.3 cargo test —— 全绿（含本测试重新启用、稳定）
- [ ] 3.4 移除 cross_thread_smoke.rs 上的 `#[ignore]`（过渡撤销）
- [ ] 3.5 docs/design/runtime/vm-architecture.md 或 GC 专章追加"并发 mark bit 生命周期 + 注册/safepoint 协议"说明

## 备注
- 过渡（阶段 2.5）：`concurrent_gc_mode_stress_no_race_no_leak` 已 windows `#[ignore]`；在 linux/macOS 仍跑（守护 invariant）。
- 盲推已被证伪：park-on-registration fix deadlock 了 `second_collector_falls_back_to_mutator_park_returns_none`（见 design.md "尝试记录"）。修复必须在 loom 下验证。
- 该 deadlock 自 2026-09-04 起是**本地确定性测试**（模型 B），不再依赖跑真单测撞上它。
