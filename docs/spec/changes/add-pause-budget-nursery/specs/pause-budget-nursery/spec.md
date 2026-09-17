# Spec: 按停顿预算自适应 nursery

## ADDED Requirements

### Requirement: minor 的停顿有可配置的上界

分代模式下，年轻代大小由**实测的每字节 minor 代价**与目标停顿共同决定，而不是一个静态常量。

#### Scenario: 编译器自举负载
- **WHEN** `z42c.driver -- build src/compiler/z42c.semantics/... --release --no-incremental`（默认配置）
- **THEN** 全程最大 GC 停顿 ≤ 10 ms，且编译产物与改动前逐字节一致

#### Scenario: 大堆负载
- **WHEN** 运行 `13_gc_large_heap` 的两档活对象规模
- **THEN** 两档的最大 GC 停顿都 ≤ 10 ms

#### Scenario: 100% 存活的分配循环
- **WHEN** 运行 `09_alloc_ctorless`（每个分配都存活，minor 扫的就是整个活堆）
- **THEN** 最大停顿显著低于改动前，且墙钟回归不超过 5%

### Requirement: 徒劳退避不得放大一次 minor 的规模

#### Scenario: 退避已经放大到上限
- **WHEN** 连续多次回收几乎不回收任何东西，退避乘数升到上限
- **THEN** 下一次 minor 扫过的年轻代不超过停顿预算算出的 nursery（退避只把回收推得更稀疏，不让单次更大）

### Requirement: 手动挡优先

#### Scenario: 显式设置了 nursery
- **WHEN** 设置 `Z42_GC_NURSERY_BYTES`
- **THEN** 自适应关闭、该值即最终 nursery，并在 `Z42_GC_TRACE` 中说明自适应已被关闭

#### Scenario: 关闭自适应
- **WHEN** 设置 `Z42_GC_PAUSE_TARGET_MS=0`
- **THEN** 行为与本变更前一致（固定 nursery）

## MODIFIED Requirements

### Requirement: `Z42_GC_NURSERY_BYTES` 的语义

**Before:** 年轻代大小的唯一来源；未设置时用内置默认 16M。
**After:** **手动挡**。未设置时年轻代由停顿预算自适应决定（默认目标 10 ms，夹在 2M~64M）；设置了就完全按设置值走。

## Pipeline Steps

- [ ] Lexer / Parser / TypeChecker / IR Codegen —— 不涉及
- [x] VM runtime（GC）
