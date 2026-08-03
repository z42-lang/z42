# Proposal: Math 迁入 z42.core（= System.Math；A2 修订）

> 总纲：improve-stdlib-org-perf 相位 A2。类型：refactor（删一个 lib + 移动类 + 改调用方）。
> **修订**：原方案「core 加 libm 门面 + z42.math 保留作 wrapper」被否——.NET 的 `Math` 是
> **`System.Math`（class Math 直接在 System 命名空间、且在 CoreLib）**，非子命名空间。故按 .NET +
> 本仓两层 interop 规则（libm=core cross-cutting 原语），把**整个 `Math` 类迁入 core**，
> 命名空间 `Std.Math`（class Math in Std 根，= System.Math），删除 `z42.math` 库。

## Why

`z42.math` 用命名空间 `Std.Math`（→ `Std.Math.Math`），既不符 .NET（`System.Math` 是 System 根下的
类），又让 libm intrinsic（全平台通用基础原语，规则要求归 core）滞留在共享库里。迁入 core 后：
- 命名与 .NET `System.Math` 对齐（`Std.Math` = class in Std 根）；
- libm intrinsic 落在唯一合法宿主 core（符合 interop 两层规则）；
- 无 wrapper、无跨 zpkg 间接开销（修订前方案的代价消失）；
- 与 W1（List/Dict 上提 core）同构——"每个程序都可能用" 的基础工具进 CoreLib。

## What Changes

- **`Math` 类迁入 core**：`z42.math/src/Math.z42` → `z42.core/src/Math.z42`，`namespace Std.Math;` →
  `namespace Std;`（class `Math` 不变；libm externs 现合法居 core；派生 Abs/Min/Max/Clamp/Sign + 常量随迁）。
- **删除 `z42.math` 库**（src/toml/README/bench；tests 迁 core）。
- **消费方去依赖**：z42.numerics / z42.random 删 `z42.math` toml 依赖 + 删 `using Std.Math;`（Math 现在
  `Std` 根，其已有 `using Std;` 覆盖；`Math.X` 调用不变）。
- **workspace**：default-members 移除 `z42.math`。
- **tests 迁移**：z42.math 的 `math_basics`（[Test]）+ `math` / `math_constants`（golden）迁 `z42.core/tests/`。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.core/src/Math.z42` | NEW | 迁自 z42.math，`namespace Std`（= System.Math）|
| `src/libraries/z42.math/` | DELETE | 整库删除（src/toml/README/bench；tests 迁走）|
| `src/libraries/z42.core/tests/math_basics.z42` | NEW | 迁自 z42.math（`using Std.Math`→`using Std`）|
| `src/libraries/z42.core/tests/math/` | NEW | golden 迁自 z42.math（source + expected）|
| `src/libraries/z42.core/tests/math_constants/` | NEW | golden 迁自 z42.math |
| `src/libraries/z42.workspace.toml` | MODIFY | default-members 删 `z42.math` |
| `src/libraries/z42.numerics/z42.numerics.z42.toml` | MODIFY | 删 z42.math 依赖 |
| `src/libraries/z42.numerics/src/Complex.z42` | MODIFY | 删 `using Std.Math;` |
| `src/libraries/z42.numerics/tests/complex.z42` | MODIFY | 删 `using Std.Math;` |
| `src/libraries/z42.random/z42.random.z42.toml` | MODIFY | 删 z42.math 依赖 |
| `src/libraries/z42.random/src/Random.z42` | MODIFY | 删 `using Std.Math;` |
| `src/libraries/z42.random/tests/random_extensions.z42` | MODIFY | 删 `using Std.Math;` |
| `src/libraries/z42.test/tests/dogfood.z42` | MODIFY | `using Std.Math;`→删（Math 在 Std）|
| `src/libraries/z42.core/src/README.md` | MODIFY | 功能索引加 Math |
| `src/libraries/README.md` | MODIFY | 库列表删 z42.math；libm 已在 core |
| `docs/design/stdlib/organization.md` | MODIFY | 现状表删 z42.math 行；R2 映射注：libm 归 core |
| `docs/spec/changes/move-math-to-core/*` | NEW | 本提案 + design + tasks |

## Out of Scope
- 派生方法是否再拆出 core（`System.Math` 都在 CoreLib，保持整类在 core）。
- A3 / A4 / B 轴。
- 脚本注释里的 `z42.math` 示例串（cosmetic，非功能；可选顺手）。

## Open Questions
- [ ] 无 bootstrap 处理（math 非 z42c 运行期自依赖，已确认）——单暖 change。
