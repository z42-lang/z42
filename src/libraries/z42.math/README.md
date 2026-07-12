# z42.math — 数学库

## 职责

`Std.Math.Math` 静态类提供基础数学常量 + 标量函数。
**纯脚本 + libm 桥接**：`Abs / Max / Min` 用脚本（自带 int / double overload），
其他超越函数（`Pow / Sqrt / Sin / Cos / ...`）走 `[Native(...)] extern` 桥到 VM
内 libm（`__math_*` builtin），保证精度与平台 libm 一致。

## src/ 核心文件

| 文件 | 类型 | 说明 |
|------|------|------|
| `Math.z42` | `static class Math` | 常量 + 标量函数（脚本：Abs/Max/Min/Sign/Clamp 含 int 变体 + 8 个 libm bridge） |

## 入口点

### 常量
- `Math.Pi` (3.141592653589793)
- `Math.E` (2.718281828459045)
- `Math.Tau` (6.283185307179586)

### 脚本实现（int / double 分名变体，`*Int` 后缀）
- `Abs(x)` / `AbsInt(x)` — 绝对值
- `Max(a, b)` / `MaxInt` · `Min(a, b)` / `MinInt` — 取大 / 小
- `Sign(x)` / `SignInt(x)` — 符号 -1 / 0 / +1
- `Clamp(x, lo, hi)` / `ClampInt(x, lo, hi)` — 夹到 [lo, hi]

### libm 桥接（仅 double 签名）
- 幂 / 根：`Pow(base, exp)` / `Sqrt(x)` / `Exp(x)`
- 取整：`Floor(x)` / `Ceiling(x)` / `Round(x)`
- 对数：`Log(x)` (自然对数) / `Log10(x)`
- 三角：`Sin(x)` / `Cos(x)` / `Tan(x)` / `Atan2(y, x)`

## 依赖关系

依赖 `z42.core`；通过 `__math_*` builtin 走 VM 调 libm，无其他 stdlib 依赖。

## Deferred / Future Work

详见 [`docs/design/stdlib/`](../../../docs/design/stdlib/) 对应主题文档。当前 v0
覆盖 BCL `System.Math` 的标量子集；未引入的内容：

### BigInteger / Decimal / Complex

- **来源**：stdlib roadmap；阻塞 z42.crypto 高级算法（RSA / ECDSA 需 BigInteger）
- **触发原因**：BigInteger 需要 dynamic-length 整数 + Karatsuba / 内嵌 GMP 桥接的设计决策；Decimal 需要 IEEE 754-2008 decimal128 ABI；都是单独 spec 工作
- **前置依赖**：spec 决策 + ABI 桥接选型

### Vector / SIMD

- **来源**：performance roadmap
- **触发原因**：需要 VM 层 vectorized opcode 支持；当前 IR 全标量
- **前置依赖**：L3 性能阶段 + Cranelift SIMD intrinsic 暴露

### Hyperbolic / Special functions

- `Sinh` / `Cosh` / `Tanh` / `Asin` / `Acos` / `Atan` / `Erf` / `Gamma` 等
- **来源**：科学计算用户后期会问；当前不阻塞
- **修复成本**：每个 ≤ 30 LOC（再加一行 `[Native("__math_*")]` extern + Rust 侧 builtin）

## 测试

`tests/`：3 个 `.z42` 测试文件覆盖标量 / overload / libm 边界情况
（NaN / Inf / 大数 / 负零 / 整数 overflow）。运行：

```bash
z42 xtask.zpkg test lib         # 完整 stdlib 测试套
```
