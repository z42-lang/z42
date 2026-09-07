# Spec: 大对象堆的内存归还

## MODIFIED Requirements

### Requirement: 整块死掉的变长 chunk 在 sweep 尾被处理

变长区在每次 major sweep 的末尾扫一遍 chunk，把**所有块都已 tombstone** 的 chunk
按种类处理：bump chunk（`cap == CHUNK_BYTES`）进 `var_free_chunk_pool` 等待重新 bump；
dedicated chunk（为 oversized 块单开的精确尺寸 chunk）**把内存还给全局分配器**。

两种处理都必须先把该 chunk 的所有块从 `all_blocks` / `free_lists` / `young_list`
三张表中摘除。

#### Scenario: 死掉的 dedicated chunk 归还内存

- **WHEN** 一个 oversized 块（payload 使总尺寸超过 `CHUNK_BYTES`）在 sweep 中被 tombstone
- **AND** 其后 `reclaim_dead_var_chunks` 运行
- **THEN** 该块的 dedicated chunk 被 `dealloc`，区不再持有这块内存
- **AND** 归还的字节数不少于块头 + payload

#### Scenario: 仍有活块的 dedicated chunk 不被释放

- **WHEN** 一个 oversized 块被标记因而在 sweep 中存活
- **THEN** `reclaim_dead_var_chunks` 不释放它的 chunk
- **AND** 该块的 payload 在 reclaim 之后仍可完整读回

#### Scenario: 释放后没有任何一张表还指向那块内存

- **WHEN** 一个 dedicated chunk 被释放
- **THEN** `all_blocks`、`free_lists`、`young_list` 中都不再有指向它的指针

#### Scenario: bump chunk 的入池行为不变

- **WHEN** 一个整块死掉的 bump chunk 被回收
- **THEN** 它进入 `var_free_chunk_pool`（内存保留），`reuse_gen` 被抬到该 chunk 中
  任何块曾达到的 generation 之上

## ADDED Requirements

### Requirement: chunk 槽位在释放后作为墓碑保留并被复用

`chunks` 的下标是 chunk 的身份（`bump_chunk` / `borrowed` / `reuse_gen` /
`var_free_chunk_pool` 均按下标寻址），因此释放内存**不得**从 `chunks` 中移除元素。
被释放的槽位以 `cap == 0` 的形式保留，并可被后续的 chunk 分配复用。

#### Scenario: 释放不改变其它 chunk 的下标

- **WHEN** 一个夹在若干 bump chunk 之间的 dedicated chunk 被释放
- **THEN** 其余 chunk 的下标不变，仍存活的块仍可正常 resolve 与读写

#### Scenario: 墓碑槽位被下一次 chunk 分配复用

- **WHEN** 一个 dedicated chunk 被释放后，区再分配一个新 chunk
- **THEN** 新 chunk 占用那个墓碑槽位，`chunks` 的长度不增长

#### Scenario: 大对象反复生灭达到稳态

- **WHEN** 反复「分配若干 oversized 块 → 全部死亡 → 回收」多轮
- **THEN** 每轮结束后区持有的 chunk 内存回到零
- **AND** `chunks` 槽位表在第一轮之后不再增长

#### Scenario: 墓碑槽位不被重复释放

- **WHEN** 区被 drop，或 `reclaim_dead_var_chunks` 再次运行
- **THEN** 已释放的槽位不会被再次 `dealloc`

### Requirement: 回收结果可观测

`reclaim_dead_var_chunks` 返回本次入池的 chunk 数、释放的 chunk 数与释放的字节数，
以区分「内存被保留复用」和「内存真的还给了分配器」。

#### Scenario: 无事可做时返回全零

- **WHEN** 没有任何整块死掉的 chunk
- **THEN** 返回值的三个字段全为 0

## Pipeline Steps

无。本 change 不改变编译或执行管线的任何阶段，只改变 GC sweep 尾的 chunk 处置。
