# Spec: 有界新生代与老年代闸门

## ADDED Requirements

### Requirement: 分代模式有两个独立的回收闸门

在 `GcMode::GenerationalMarkSweep` 下，自动回收由两个独立条件触发：
新生代按**自上次回收结束以来的分配量**达到 `Z42_GC_NURSERY_BYTES` 触发 **minor**；
老年代按**自上次 major 以来晋升进老年代的字节数**达到 `Z42_GC_MAX_BYTES` 触发 **major**。
任一条件成立即回收。

#### Scenario: 新生代闸门先于预算闸门触发

- **GIVEN** 一个分代堆，用量全程低于 `gc_near_limit_ratio × Z42_GC_MAX_BYTES`
- **WHEN** 自上次回收以来分配量超过新生代容量
- **THEN** 发生一次 minor 回收

#### Scenario: 新生代闸门只在分代模式生效

- **GIVEN** 同样的负载，模式为 `StwMarkSweep`
- **THEN** 不发生任何自动回收（沿用原本的单一预算闸门）

#### Scenario: 晋升喂老年代预算，major 清零

- **WHEN** 存活条目跨过晋升阈值进入老年代
- **THEN** 其字节数累加进老年代预算计数
- **AND** 一次 major 回收之后该计数归零

#### Scenario: 新生代容量默认为预算的四分之一

- **WHEN** `Z42_GC_NURSERY_BYTES` 未设置
- **THEN** 新生代容量取 `Z42_GC_MAX_BYTES / 4`

### Requirement: minor 回收整块死亡的 chunk

minor 回收结束时，三个 region 中所有条目都已 tombstone 的 chunk 必须归还各自的
chunk 池，供后续分配复用。

#### Scenario: 整块死亡的 chunk 在 minor 后可复用

- **GIVEN** 若干未被引用的对象填满了数个 chunk
- **WHEN** 一次 minor 回收运行
- **THEN** chunk 池中出现可复用的 chunk

## MODIFIED Requirements

### Requirement: minor 升级为 major 的存活率判据

存活率定义为「本次 minor **未被 tombstone** 的年轻条目占比」，即
`1 - 回收条目数 / 回收前年轻条目数`。被晋升的幸存者算**存活**。

#### Scenario: 晋升不计为死亡

- **GIVEN** 一次 minor 中大部分年轻条目存活并被晋升
- **THEN** 存活率接近 1，而不是接近 0

## Pipeline Steps

无。只改 GC 的自动回收策略与 minor 的收尾动作。
