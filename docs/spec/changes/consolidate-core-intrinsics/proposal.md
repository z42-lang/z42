# Proposal: 去重 cross-cutting intrinsic 到 z42.core（A1）

> 总纲：`improve-stdlib-org-perf`。本 change 落实其相位 A1 + 纪律「单一声明点」。
> 类型：refactor（产出 byte-identical，无新对外行为）+ 一处 bootstrap 构建脚本修正。

## Why

同一个 VM intrinsic 被多个库重复声明为 `extern`，违反新确立的「单一声明点」纪律
（[organization.md](../../../design/stdlib/organization.md)）：

- **位转换** `__double_to_bits` / `__double_from_bits` / `__single_to_bits` / `__single_from_bits`：
  在 `z42.io.binary`（BinaryWriter/Reader）**和** `z42.ir`（ZbcInstr/ZbcReaderInstr）双声明。
- **时钟** `__time_now_ms`：`z42.time` / `z42.io` / `z42.net` 三声明；
  `__time_now_mono_ns`：`z42.time` **和** `z42.test` 双声明。

把每个 intrinsic 收敛到 `z42.core` 声明一次、公开薄封装，其余库调用它。

## What Changes

- **core 新增两个最小原语门面**：`Std.BitConverter`（4 个位转换）+ `Std.Runtime.Clock`（wall/mono 时钟）。
- **各库删除自带 extern，改调 core**：io.binary / ir（位转换）、time / io / net / test（时钟）。
- **z42.ir 保留 `ZbcInstr.DoubleToBits` / `ZbcReaderInstr.BitsToDouble` 的公开签名**（改为委托 core 的薄
  wrapper，仅删 extern）——因为 **z42c 源（IrGenFacts）在用这两个方法**，改其 API 会踩 seed-API 两-nightly
  轴。保留签名 = z42c 源零改动 = 不触发该轴。
- **bootstrap 脚本修正**：`_ensureBootstrapZ42Ir` 在单包重建 `z42.ir` **前**先把当前源 `z42.core` 建进 flat
  libs——否则冷/首暖构建时 `z42.ir` 对着缺 `BitConverter` 的旧 flat core 编译 → `undefined function`（详见 design 决策 3）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.core/src/BitConverter.z42` | NEW | `Std.BitConverter`：4 个位转换 extern（唯一声明点）|
| `src/libraries/z42.core/src/Clock.z42` | NEW | `Std.Runtime.Clock`：`WallMillis()` / `MonoNanos()` extern |
| `src/libraries/z42.io.binary/src/BinaryWriter.z42` | MODIFY | 删 `_SingleToBits`/`_DoubleToBits` extern，改调 `Std.BitConverter` |
| `src/libraries/z42.io.binary/src/BinaryReader.z42` | MODIFY | 删 `_SingleFromBits`/`_DoubleFromBits` extern，改调 core |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcInstr.z42` | MODIFY | `DoubleToBits` 保签名、删 extern、委托 core |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcReaderInstr.z42` | MODIFY | `BitsToDouble` 保签名、删 extern、委托 core |
| `src/libraries/z42.time/src/DateTime.z42` | MODIFY | 删 `NowMs` extern，改调 `Std.Runtime.Clock.WallMillis` |
| `src/libraries/z42.time/src/Stopwatch.z42` | MODIFY | 删 `MonoNs` extern，改调 `Std.Runtime.Clock.MonoNanos` |
| `src/libraries/z42.io/src/Environment.z42` | MODIFY | `GetCurrentTimeMs` 保签名、删 extern、委托 core |
| `src/libraries/z42.net/src/Http/HttpClient.z42` | MODIFY | 删 `_timeNowMs` extern，改调 core |
| `src/libraries/z42.test/src/Bencher.z42` | MODIFY | 删 `__time_now_mono_ns` extern，改调 core |
| `scripts/build/xtask_compiler.z42` | MODIFY | `_ensureBootstrapZ42Ir`：z42.ir 前先建当前源 z42.core |
| `src/libraries/z42.core/src/README.md` | MODIFY | 功能索引 + 核心文件登记 BitConverter/Clock |
| `src/libraries/README.md` | MODIFY | Extern 现状审计表：io.binary/ir/time 位转换·时钟已归 core |
| `docs/design/stdlib/organization.md` | MODIFY | 「现状」表 extern 列刷新 |
| `docs/design/compiler/self-hosting.md` | MODIFY | 轴 ④：`_ensureBootstrapZ42Ir` 现也预建 core |
| `docs/spec/changes/consolidate-core-intrinsics/*` | NEW | 本提案 + design + spec + tasks |

**只读引用**：
- `src/compiler/z42c.semantics/src/IrGenFacts.z42` — 确认 z42c 源只用 `ZbcInstr.DoubleToBits`/`BitsToDouble`（保签名故不改）
- `.claude/rules/bootstrap-seed.md` — 轴 ②/④ 判据

## Out of Scope
- math/time 的 `__math_*` 归 core（相位 A2，另 change——工作量与 seed 评估更大）。
- 能力库 interop 单 sink 收敛（A3）。
- 任何性能下沉（B 轴）。

## Open Questions
- [ ] `Std.BitConverter` vs 折进 `Std.Convert`：倾向独立类（见 design 决策 1）。
- [ ] Clock 命名空间 `Std.Runtime` vs `Std`：倾向 `Std.Runtime`（低层原语，与 `Runtime` 同域）。
