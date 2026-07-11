# Spec: 删 TSIG/EXPT

## ADDED Requirements

### Requirement: 跨包解析改读 TYPE/SIGS/IMPL

#### Scenario: 跨包类型/方法解析
- **WHEN** z42c 编译一个依赖别包类型/方法的源
- **THEN** DepScan 经 `TsigReconcile.Rebuild`（读 TYPE/SIGS）重建导出签名，编译成功（不再读 TSIG）

#### Scenario: 跨包 impl 传播
- **WHEN** 包 B `impl Trait for Type`（Type 在包 A），主程序用该 impl 方法
- **THEN** Rebuild 经 IMPL 段（`ReadImplInto`）读进 Impls，跨包 impl 方法解析/传播/反射正常

#### Scenario: indexed 依赖
- **WHEN** 依赖包是 indexed（TYPE 在散装 zbc）
- **THEN** `ReadModuleTypes` 经 FILE 目录 + 包目录 load 散装 zbc 提取 TYPE，Rebuild 正常

## REMOVED Requirements

**Before:** zpkg 写 EXPT（导出符号清单）+ TSIG（跨包类型签名）两段；z42c 读 TSIG 做跨包解析。
**After:** 不写 EXPT（写而不读，冗余）+ TSIG（由 Rebuild 取代）；段面 packed 9→7/7→6、indexed 同步。
IMPL 保留。zbc 不变。

## IR Mapping
无 IR/opcode 变化。zpkg 格式 bump 0.30→0.31（删两段）。

## Pipeline Steps
- [x] IR/format — ZpkgWriter/Indexed 停写 EXPT/TSIG；ZpkgReader 删 ReadTsig + ReadModuleTypes 扩 indexed
- [x] 消费侧 — DepScan ReadTsig→Rebuild（world 一次解析复用）；Rebuild 读 IMPL
