# Tasks: Math 迁入 z42.core（= System.Math；A2）

> 状态：🟡 待确认（6.5，修订版）| 创建：2026-08-03 | 类型：refactor（删 lib + 移类），无 bootstrap 改动

> 实施完成，warm 验证通过；full gate + byte-identity 待 PR/CI。

## 进度概览
- [x] 阶段 1: Math 迁 core（namespace Std）
- [x] 阶段 2: 删 z42.math 库 + 消费方去依赖 + workspace
- [x] 阶段 3: tests 迁 core
- [x] 阶段 4: 文档
- [x] 阶段 5: 验证（warm）—— 24/24 编译 + Math 行为 + 消费方 + 迁移 tests 编译/运行；full gate → PR/CI

## 阶段 1: 迁类
- [ ] 1.1 `git mv z42.math/src/Math.z42 z42.core/src/Math.z42`；`namespace Std.Math;` → `namespace Std;`

## 阶段 2: 删库 + 去依赖
- [ ] 2.1 z42.numerics：toml 删 z42.math 依赖；Complex.z42 删 `using Std.Math;`
- [ ] 2.2 z42.random：toml 删 z42.math 依赖；Random.z42 删 `using Std.Math;`
- [ ] 2.3 z42.test/tests/dogfood.z42：删 `using Std.Math;`（注释同步）
- [ ] 2.4 z42.workspace.toml：default-members 移除 `z42.math`
- [ ] 2.5 删除 `src/libraries/z42.math/`（src/toml/README/bench；tests 已迁走）

## 阶段 3: tests 迁 core
- [ ] 3.1 `git mv z42.math/tests/math_basics.z42 z42.core/tests/`；`using Std.Math`→`using Std`
- [ ] 3.2 `git mv z42.math/tests/math z42.core/tests/math`（source+expected）；source `using Std.Math`→`using Std`
- [ ] 3.3 `git mv z42.math/tests/math_constants z42.core/tests/math_constants`；source `using Std.Math`→`using Std`
- [ ] 3.4 （z42.math/bench/math_bench.z42：迁 z42.core/bench/ 或删——按 core 是否有 bench 目录定）

## 阶段 4: 文档
- [ ] 4.1 z42.core/src/README.md 功能索引 + 依赖表加 Math
- [ ] 4.2 src/libraries/README.md 库列表删 z42.math 行；标 libm 在 core
- [ ] 4.3 organization.md 现状表删 z42.math 行；R2 domain 映射注：libm→core（= System.Math）

## 阶段 5: 验证
- [ ] 5.1 stdlib workspace 全量编译（core 含 Math；numerics/random 去依赖后编过；z42.math 不在列）
- [ ] 5.2 行为 smoke：Sqrt(2)/Pow(2,10)/Abs(-3)/Sin 已知值（fresh stdlib）
- [ ] 5.3 grep：无 `using Std.Math` 残留、无 z42.math 依赖残留、`__math_*` 仅 core Math.z42
- [ ] 5.4 完整 `xtask test` + byte-identity → PR/CI（同 A1 策略）
- [ ] 5.5 commit（branch 续栈）

## 备注
- 无 bootstrap 改动（math 非 z42c 运行期自依赖）。
- 无兼容层（pre-1.0）；消费方调用点 `Math.X` 不变，仅去 `using Std.Math` + toml 依赖。
