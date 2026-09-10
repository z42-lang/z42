# Spec: per-chunk 普查

## ADDED Requirements

### Requirement: 每个块知道自己所属的 chunk

变长区的每个块头携带其所属 chunk 的下标，且块头大小保持 16 字节不变。
不属于任何 region chunk 的块（leak / 测试用）用哨兵值标记。

#### Scenario: 块头大小不变

- **THEN** `GcBlockHeader` 仍为 16 字节、8 字节对齐

#### Scenario: 每个分配出的块携带有效的 chunk 下标

- **WHEN** 分配任意大小的块（含走 dedicated chunk 的 oversized 块）
- **THEN** 其块头记录的 chunk 下标指向一个该区确实拥有的 chunk

#### Scenario: oversized 块独占其 chunk

- **WHEN** 同时存在普通块与 oversized 块
- **THEN** oversized 块的 chunk 下标与任何普通块都不同

### Requirement: 两个 region 维护 per-chunk 普查

每个 region 为每个 chunk 维护「曾构造的槽/块数」与「仍存活的槽/块数」，
由分配、TLAB retire、tombstone 增量维护。chunk 回收的「是否全死」判定只读这份普查。

#### Scenario: 普查与全量扫描一致

- **GIVEN** 一段包含分配、tombstone、自由链复用与 oversized 分配的负载
- **THEN** 每个 chunk 的存活数与块数都与遍历全部块得到的真值相等

#### Scenario: 入池的 chunk 保留其「曾构造」计数

- **WHEN** 一个 chunk 因全死而入池
- **THEN** 其存活计数归零，但「曾构造」的事实仍被保留（槽仍是构造好的，
  以便复用时保留每槽的 tombstone generation）

#### Scenario: 回收判定与修改前一致

- **WHEN** 同一负载分别在修改前后运行
- **THEN** 被回收的 chunk 集合相同（本变更只改判定的**代价**，不改判定本身）

## Pipeline Steps

无。纯 GC 内部数据结构与算法复杂度的改动。
