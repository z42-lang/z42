# Tasks: 周期中的 minor 只把年轻的灰条目当根

> 状态：🔵 DRAFT，待 User 过 gate | 创建：2026-09-18
> 变更类型：`vm`（GC 根集语义；不改语言 / IR / zbc 格式）
> 前置：`add-incremental-major-gc` M0~M2b + `add-pause-budget-nursery`（M3）已合
> 本 change = M3 挖出、当时明确不在 M3 修的**两笔账之一**（另一笔「切片把 minor 闸门推远」是独立 change）

## 进度概览
- [ ] 0: DRAFT 与 gate
- [ ] 1: 实现（根集按年龄分流 + 诊断）
- [ ] 2: 回归测试（正向 + 阴性对照）
- [ ] 3: 验收（停顿 / 墙钟 / RSS / 产物一致）+ 文档

## 0: DRAFT 与 gate
- [x] 0.1 proposal / design / spec / tasks
- [x] 0.2 论证的三个前提逐条在代码中坐实（D1 前提 A、B；D2 var 区逐变体；D3 与 doomed 正交）
- [x] 0.3 实测这笔账的规模（D5 表：`--large` 上 68.3% 的灰根是老条目、单次峰值 118 690）
- [ ] 0.4 **User gate**：① SATB 是否一并改（D4 建议改，理由是规则一致性而非数字）；② 验收门槛

## 1: 实现
- [ ] 1.1 `generational.rs::mark_phase_minor`：灰/SATB 播种按 `gen_age_of < threshold` 过滤
- [ ] 1.2 `minor roots` 诊断行区分播种数与跳过数

## 2: 回归测试
- [ ] 2.1 `arc_heap_tests/incremental.rs`：老灰条目的年轻子节点存活（D1 的脏卡路径）
- [ ] 2.2 年轻灰条目不被回收 + **阴性对照**（把年轻的也跳过 ⇒ 必须红）
- [ ] 2.3 `cargo test --lib`（**debug**，不能用 `--release`，见 [[z42-cargo-test-signal-helper-wedge]] 的教训）

## 3: 验收与文档
- [ ] 3.1 A/B（同机交错 ×3，两个二进制）：`13_gc_large_heap` 两档 + `09` + `12` + `z42c.semantics`
      —— 预期收益集中在大堆两档；`semantics` 预期持平（D5）
- [ ] 3.2 产物逐字节一致（`z42c.semantics.zpkg`）
- [ ] 3.3 GREEN（`xtask test`，注意 `Z42_PORTABLE_VM` 指向本树 z42vm）
- [ ] 3.4 `docs/internals/src/runtime/gc-incremental-major.md`：「灰/SATB 作 minor 根」一节改写 + 卡表论证
- [ ] 3.5 `docs/internals/src/runtime/gc.md`：卡表不变量一节补交叉引用
