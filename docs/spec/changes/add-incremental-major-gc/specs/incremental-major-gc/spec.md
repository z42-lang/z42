# Spec: 增量 major 标记

## ADDED Requirements

### Requirement: major 停顿有界且与堆大小无关

分代模式下，一次 major 周期被拆成若干 STW 切片，每个切片的停顿不超过 `Z42_GC_SLICE_MS`（默认 2 ms）加一个固定的
切片调度开销；root 快照切片的停顿与 root 数量成正比，与堆大小无关。

#### Scenario: 编译器自举负载
- **WHEN** `z42c.driver -- build src/compiler/z42c.semantics/z42c.semantics.z42.toml --release --no-incremental`（默认配置）
- **THEN** 全程最大 GC 停顿 ≤ 10 ms，且编译产物与改动前逐字节一致

#### Scenario: 大堆负载停顿不随堆增长
- **WHEN** 运行 `13_gc_large_heap` 的两档活对象规模（约 150 MB 与 450 MB）
- **THEN** 两档的最大 GC 停顿都 ≤ 10 ms，且大档不比小档高出 20% 以上

### Requirement: 标记期间的写不会造成漏标

#### Scenario: 从未扫描对象中取出引用后清空字段
- **WHEN** major 周期处于标记阶段，mutator 把对象 A 的字段 `f` 读入寄存器，随后把 `A.f` 写为 `null`，此后该对象只被寄存器持有
- **THEN** 周期结束后该对象仍存活，句柄可正常解引用

#### Scenario: 标记与清扫期间新分配的对象
- **WHEN** 对象在标记阶段或清扫阶段被分配，且只被寄存器持有
- **THEN** 本周期不会回收它；它在下一个周期若不可达则被回收

#### Scenario: 弱引用读取
- **WHEN** 标记阶段中，一个只剩弱引用的对象通过弱句柄被读回寄存器
- **THEN** 本周期不会回收它

### Requirement: 切片之间允许 minor

#### Scenario: SATB 记住的年轻对象
- **WHEN** 标记阶段中，一个 S 时刻可达的年轻对象的最后一条引用被覆盖，随后发生一次 minor
- **THEN** 该对象活过这次 minor；major 周期结束时若仍不可达，才被回收

### Requirement: 同步完成的退化路径

#### Scenario: 显式回收
- **WHEN** 周期进行中调用 `GC.Collect()` / `GC.ForceCollect()`，或堆触及软上限
- **THEN** 当前周期在本次调用内同步完成，返回时所有不可达对象已回收（与今天的语义一致）

## MODIFIED Requirements

### Requirement: major 不再有全堆 reset marks 遍历

**Before:** 每次 major 先遍历全部条目清除标记位，再标记、清扫、升龄（四趟）。
**After:** major 标记用周期 epoch 表示，换代即全白；清扫与升龄合为一趟。`Z42_GC_PHASES` 不再输出 `reset marks` / `age survivors` 行，
改为每切片一行 `slice <kind>` 与周期汇总行。

## Pipeline Steps

- [ ] Lexer / Parser / TypeChecker / IR Codegen —— 不涉及
- [x] VM runtime（GC）
