# Spec: minor GC 的标记不变量

## MODIFIED Requirements

### Requirement: minor GC 的标记范围

minor 的标记阶段只给**年轻**条目置 mark 位。老条目（`gen_age >= PROMOTION_THRESHOLD`）
作为根出队时**不置位**，但仍然要追踪它的孩子，把其中年轻的入队。

#### Scenario: 老根在每一次 minor 都追踪它的孩子

- **GIVEN** 一个老对象持有唯一一条指向某个年轻对象的引用，且该年轻对象不落在任何脏 chunk 里
- **WHEN** 连续运行多次 minor
- **THEN** 该年轻对象每一次都存活，引用始终有效

#### Scenario: 一次 minor 结束后堆里没有被置位的 mark

- **WHEN** 一次 minor 结束
- **THEN** 老条目与年轻幸存者的 mark 位都是清的

#### Scenario: 老→年轻的图在多轮换新孩子后仍完整

- **GIVEN** 若干老对象，每轮 minor 之前各被写入一个**新**的年轻对象
- **WHEN** 连续运行多轮
- **THEN** 每一轮之后每个老对象持有的年轻对象都仍然存活

#### Scenario: 年轻条目的回收行为不变

- **WHEN** 一个年轻条目在 minor 中不可达
- **THEN** 它照旧被 tombstone

### Requirement: 全量清位覆盖三个 region

`reset_all_marks_in_regions`（STW / major 回收开头的防御性清位）必须覆盖对象区、
数组区**和变长区**。

#### Scenario: 变长区的残留 mark 位在 major 开始前被清

- **GIVEN** 变长区里有块带着上一轮遗留的 mark 位
- **WHEN** 一次 STW / major 回收开始
- **THEN** 该位已被清除，标记阶段能正常追踪该块的孩子

## Pipeline Steps

无。只改 GC 内部的标记阶段，不触及编译或执行管线。
