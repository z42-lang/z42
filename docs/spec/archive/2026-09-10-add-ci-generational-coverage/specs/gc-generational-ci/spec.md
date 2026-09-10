# Spec: 分代模式的 CI 覆盖与数组写屏障

## ADDED Requirements

### Requirement: GREEN gate 覆盖分代收集器

完整 gate 必须包含一个用 `GcMode::GenerationalMarkSweep` 运行真实编译负载的 stage，
其新生代容量小到足以在一次运行中发生数十次 minor 回收。

#### Scenario: 分代下的真实负载编译成功

- **WHEN** gate 运行到该 stage
- **THEN** 在分代模式下重编一个真实包成功，且该 stage 可经 `--skip` 下放到独立 CI job

#### Scenario: 引入跨代缺陷时该 stage 变红

- **GIVEN** 晋升处不再补置脏卡（即 old→young 边失去记录）
- **WHEN** 该 stage 运行
- **THEN** 编译失败，stage 变红

### Requirement: 所有写入数组元素的路径都要发写屏障

任何把**堆引用**写进数组元素的执行路径，都必须触发数组写屏障，使得老数组收到年轻元素时
其卡被置脏。

#### Scenario: 单元素反射写

- **WHEN** 经 `Array.SetValue` 把一个年轻堆引用写进一个老数组
- **THEN** 该数组的卡变脏

#### Scenario: 批量拷贝

- **WHEN** 经 `Array.Copy` 把含年轻堆引用的区间拷进一个老数组
- **THEN** 该数组的卡变脏

#### Scenario: 只含基元的拷贝不置脏

- **WHEN** 拷贝的区间不含任何堆引用
- **THEN** 不置脏任何卡（屏障必须精确，不能退化为「这个数组被写过」）

#### Scenario: 经 ref 写数组元素

- **WHEN** 通过 `ref` 形参把一个年轻堆引用写进一个老数组的元素
- **THEN** 该数组的卡变脏

## Pipeline Steps

无。
