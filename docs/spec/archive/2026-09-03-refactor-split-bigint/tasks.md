# Tasks: 拆分 BigInt.z42（refactor-split-bigint）

> 状态：🟢 已完成 | 完成：2026-09-03 | 创建：2026-09-03 | 类型：refactor（stdlib z42.numerics；三面评审 L-10 文本结构阶段 ④）
**变更说明：** `BigInt.z42` 2234 行（200 行类型硬限的 11×）：把模幂 / 模逆（Montgomery REDC）、Miller-Rabin（随机 + OEIS A014233
确定性见证）、NextPrime 轮转、BPSW（强 Lucas + Jacobi）三块算法提到 `BigIntModular` / `BigIntPrimality` / `BigIntBpsw` 三个
`internal static class`；`BigInt` 保留同名公开方法作薄委托（**API 不变**）。算法体逐字搬移，仅 `this` → 显式 `self` 参数、
跨类 helper 调用改前缀；`_trim` / `_magMul` 由默认私有改 `internal`。
**原因：** code-organization.md 文件 500 行 / 类型 200 行硬限；三块算法与核心算术（limb 运算 / 进制转换 / 位运算）职责正交。
**文档影响：** `src/libraries/z42.numerics/README.md`（核心文件表）。

- [x] 1.1 `BigIntModular.z42` / `BigIntPrimality.z42` / `BigIntBpsw.z42` + BigInt 薄委托；BigInt.z42 2234 → 1498 行（仍超 500，后续继续拆 limb 算术）
- [x] 2. `xtask test stdlib z42.numerics`（bigint_* 12 个测试文件）+ `xtask test` GREEN
- [x] 3. 文档同步 + 归档
