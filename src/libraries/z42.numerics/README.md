# z42.numerics — 数值库

## 职责

z42 任意精度 + 扩展数值类型：`BigInt`（任意精度整数）/ `Complex` / `Decimal`。

## src/ 核心文件

| 文件 | 类型 | 说明 |
|------|------|------|
| `BigInt.z42` | `BigInt` | 任意精度整数；construct / Parse(decimal+hex) / Add/Sub/Mul/Div/Mod / Pow / CompareTo / ToString (decimal) / ToHex / `ToBase(radix 2–36)` |
| `BigIntModular.z42` / `BigIntPrimality.z42` / `BigIntBpsw.z42` | `BigIntModular` / `BigIntPrimality` / `BigIntBpsw`（internal）| ModPow（Montgomery REDC）/ ModInverse；Miller-Rabin（随机 + A014233 确定性见证）/ NextPrime / 小素数试除；BPSW（强 Lucas + Jacobi）——`BigInt` 同名公开方法为薄委托 |
| `Complex.z42` | `Complex` | 复数（`Real` / `Imaginary` + 算术） |
| `Decimal.z42` | `Decimal` | 十进制定点数 |

## 入口点

- `Std.Numerics.BigInt` / `Complex` / `Decimal`

## 依赖关系

- `z42.core` — 基础类型 / 异常
- `z42.random` — `BigInt.IsProbablyPrime` 的 Miller-Rabin 随机见证

## 实现策略

纯脚本，无 VM 改动。Magnitude 用 `int[]` little-endian 31-bit limb（每 limb
存 0..2^31-1，留 1 bit 给 mul 中间结果 fit `long` i64）；sign 用 `int _sign`
(-1/0/+1)。详 [docs/reference/src/stdlib/numerics.md](../../../docs/reference/src/stdlib/numerics.md)。

## 如何测试验证

```bash
xtask test stdlib z42.numerics    # 本库全部 [Test]
```
