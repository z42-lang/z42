# Spec: 卡粒度与扫过即清

## MODIFIED Requirements

### Requirement: 卡覆盖 chunk 的一段，而非整个 chunk

每个 chunk 被划分为固定数量的卡，每张卡覆盖相同条数的条目。置脏一个条目只点亮它
所属的那一张卡；minor 只把脏卡覆盖的条目当作根。

#### Scenario: 一次写只点亮一张卡

- **WHEN** 把某个条目所在的卡置脏
- **THEN** 只有该卡覆盖的那一段条目成为 minor 的根
- **AND** 同一 chunk 内其它段的条目不成为根

#### Scenario: 整个 chunk 全脏时行为不变

- **WHEN** 一个 chunk 的每一张卡都被置脏
- **THEN** 该 chunk 的全部活条目都成为根

#### Scenario: 清一张卡不影响同 chunk 的其它卡

- **GIVEN** 同一 chunk 内两张卡都是脏的
- **WHEN** 清掉其中一张
- **THEN** 另一张仍然是脏的，且该 chunk 仍被视为「有脏卡」

#### Scenario: 每张卡都清掉后 chunk 变干净

- **WHEN** 一个 chunk 的所有脏卡都被清掉
- **THEN** 该 chunk 不再被视为「有脏卡」

## ADDED Requirements

### Requirement: minor 扫过的卡若不再指向年轻对象则被清掉

minor 在扫描一张脏卡时判断它覆盖的条目是否仍能到达任何年轻条目；若不能，则清掉该卡。
写屏障与晋升处的置脏负责在需要时重新点亮它。

#### Scenario: 不再有跨代边的卡被清掉

- **GIVEN** 一张脏卡，其覆盖的条目都不再引用任何年轻条目
- **WHEN** 一次 minor 扫过它
- **THEN** 该卡在此次 minor 之后是干净的

#### Scenario: 仍有跨代边的卡保持脏

- **GIVEN** 一张脏卡，其覆盖的某个老条目仍引用一个年轻条目
- **WHEN** 一次 minor 扫过它
- **THEN** 该卡仍然是脏的，且该年轻条目存活

#### Scenario: 清扫不破坏跨代不变量

- **WHEN** 连续多次 minor 运行
- **THEN** 只经由老条目可达的年轻对象始终存活

### Requirement: 非分代模式不受影响

卡表只服务于 minor。`StwMarkSweep` 下的回收行为与用量不因本变更改变。

#### Scenario: STW 模式的停顿与用量不变

- **WHEN** 同一负载在 `StwMarkSweep` 下运行
- **THEN** 峰值用量与回收次数与变更前一致

## Pipeline Steps

无。
