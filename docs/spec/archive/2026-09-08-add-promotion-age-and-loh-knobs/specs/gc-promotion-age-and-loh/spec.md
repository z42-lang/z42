# Spec: 晋升年龄与大对象门槛旋钮

## ADDED Requirements

### Requirement: 晋升年龄可配置，且在建堆时定死

`Z42_GC_PROMOTION_AGE` 决定一个条目要熬过几次 minor 才被晋升。它在**建堆时读取一次**并
缓存，此后在该堆的生命周期内不再改变；写屏障与 minor 的年龄判定都读缓存值。

#### Scenario: 按设定的年龄晋升

- **GIVEN** 一个以晋升年龄 N 构造的 region
- **WHEN** 一个存活条目连续熬过 N 次 minor
- **THEN** 它在第 N 次离开年轻集合，且在此之前不离开

#### Scenario: 越界设定被警告并 clamp

- **WHEN** `Z42_GC_PROMOTION_AGE` 设为 0 或大于可表示的最大年龄
- **THEN** 输出警告并 clamp 到合法范围内，而不是静默饱和

#### Scenario: 不设时保持既有默认

- **WHEN** `Z42_GC_PROMOTION_AGE` 未设置
- **THEN** 晋升年龄为 2

### Requirement: 大对象门槛可配置，进程级生效

`Z42_GC_LOH_BYTES` 决定变长块的总尺寸超过多少就走 dedicated chunk。它是进程级设置，
在 VM 构造时应用一次。

#### Scenario: 降低门槛让更多块走 dedicated chunk

- **GIVEN** 一个在默认门槛下属于 in-chunk size class 的 payload
- **WHEN** 门槛降到该 payload 的尺寸之下
- **THEN** 它被归类为 oversized，且其 footprint 足以容纳块头 + payload

#### Scenario: 门槛不超过 bump chunk 的容量

- **WHEN** 设定值大于 bump chunk 容量
- **THEN** 实际生效值为 bump chunk 容量

#### Scenario: 不设时保持既有默认

- **WHEN** `Z42_GC_LOH_BYTES` 未设置
- **THEN** 门槛为 bump chunk 容量（64 KB）

## MODIFIED Requirements

### Requirement: 写屏障的跨代判定读取闭包块自身的年龄

写屏障判断被写入的值是否年轻时，对闭包读取**闭包块自身**的 `gen_age`，
与 minor 标记阶段使用的判据一致。

#### Scenario: 年轻闭包写入老 owner 会置脏卡

- **GIVEN** 一个年轻的闭包块，其 `env` 数组已经是老的
- **WHEN** 它被写入一个老对象
- **THEN** owner 的卡被置脏

## Pipeline Steps

无。
