# Design: Math 迁入 z42.core（= System.Math；A2）

## Architecture

```
   z42.core   namespace Std;  class Math   (= System.Math，随 CoreLib)
      • libm externs: Pow/Sqrt/Floor/Ceiling/Round/Log/Log10/Sin/Cos/Tan/Atan2/Exp
      • 派生（脚本）: Abs/AbsInt/Max/MaxInt/Min/MinInt/Sign/SignInt/Clamp/ClampInt
      • 常量: Pi/E/Tau
        ▲ 直接调 Math.X（core 隐式 prelude；`using Std;`）
   z42.numerics / z42.random           ← 删 z42.math 依赖 + 删 `using Std.Math;`
   (z42.math 库删除)
```

## Decisions

### D1: 整个 Math 迁 core，namespace `Std`（= System.Math）
.NET `System.Math` 是 System 根命名空间下的**类**、且在 CoreLib。z42 对齐：`namespace Std; class Math`
→ FQN `Std.Math` 为类（不再有 `Std.Math` 子命名空间）。libm externs 现居唯一合法宿主 core（interop 规则）。
派生方法与常量随迁——`System.Math` 整类都在 CoreLib，不为「primitive vs feature」再切一刀（切了会重现
wrapper 自引用命名问题，且 .NET 不这么分）。与 W1（List/Dict → core）同一工程判断。

### D2: 删除 z42.math 库，消费方去依赖
z42.math 仅含 Math.z42 → 迁走后库空 → 删（避免空占位库）。z42.numerics/z42.random 已 `using Std;`，
删 `using Std.Math;` 即可，`Math.X` 经 Std 根解析；同时删 toml 的 z42.math 依赖。workspace default-members
去除 z42.math。**无兼容层**（pre-1.0）。

### D3: 无命名冲突
删掉 z42.math 后不再有 `Std.Math` 命名空间，故 core 的 `Std.Math`（类）无歧义。原修订前方案的
「wrapper 自引用 Math.Pow」问题随「不再有 wrapper」消失。

### D4: 无 bootstrap 处理（对比 A1）
math 不在 z42c 运行期自依赖链（grep 确认 z42c/xtask 源不用 Math），core-first 构建保证先有 Math。
不改 `_ensureBootstrapZ42Ir`。

### D5: tests 随类迁 core
`math_basics`（[Test]）+ `math`/`math_constants`（golden source+expected）迁 `z42.core/tests/`，
`using Std.Math;`→`using Std;`，保持 Math 的测试覆盖不丢。

## Testing Strategy
- workspace 全量编译（core 含 Math；numerics/random 去依赖后仍编过 = 解析走 Std 根 OK；z42.math 不在列）。
- 行为 smoke：Sqrt(2)=1.41421356…、Pow(2,10)=1024、Abs(-3)=3（fresh stdlib）。
- 完整 `xtask test`（core [Test] + 迁入的 math goldens + numerics/random dogfood）+ byte-identity → PR/CI。
- grep：无 `using Std.Math`、无 z42.math 依赖残留、`__math_*` 仅在 core Math.z42。
