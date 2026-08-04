# Spec: cross-cutting intrinsic 单一声明点

## ADDED Requirements

### Requirement: 位转换 intrinsic 唯一声明在 core

#### Scenario: 只有 core 声明位转换 extern
- **WHEN** `grep -rn '__double_to_bits\|__double_from_bits\|__single_to_bits\|__single_from_bits' src/libraries --include=*.z42`
- **THEN** 命中仅出现在 `z42.core/src/BitConverter.z42`（每符号一次），io.binary / ir 内无 `extern` 声明

#### Scenario: io.binary Single/Double round-trip 行为不变
- **WHEN** BinaryWriter 写 Single/Double(LE/BE) 后 BinaryReader 读回
- **THEN** 值与改前一致（既有 z42.io.binary [Test] 全绿）

### Requirement: 时钟 intrinsic 唯一声明在 core

#### Scenario: 只有 core 声明时钟 extern
- **WHEN** `grep -rn '__time_now_ms\|__time_now_mono_ns' src/libraries --include=*.z42`
- **THEN** 命中仅出现在 `z42.core/src/Clock.z42`；time / io / net / test 内无 `extern` 声明

## MODIFIED Requirements

### Requirement: z42.ir zbc 编码 API 签名保持

**Before:** `ZbcInstr.DoubleToBits(double)->long` / `ZbcReaderInstr.BitsToDouble(long)->double` 以 `extern` 实现。
**After:** 同签名，body 委托 `Std.BitConverter`；**公开签名不变**（z42c 源 IrGenFacts 调用点零改动）。

#### Scenario: z42c 自举字节不动点
- **WHEN** 用改后源自建 z42c（gen1 → gen2）
- **THEN** 产物与改前 byte-identical（intrinsic 语义 + z42c 源均未变）

#### Scenario: 冷/首暖构建 z42.ir 单包重建成功
- **WHEN** `_ensureBootstrapZ42Ir` 运行（flat core 为 seed/旧版）
- **THEN** 先预建当前源 z42.core（含 BitConverter）→ z42.ir 单包编译无 `undefined function`

## Pipeline Steps
- [ ] （无 lexer/parser/typechecker/codegen 改动——纯 stdlib 重定位 + 构建脚本）
