# relocate-xtask-handlers — `xtask.z42` 的错位 handler 归位

> 类型：`refactor`（最小化模式，无需 DRAFT 规范）。
> 属 scripts/ 结构优化程序**第 3 批**（#472 文档漂移门 → #474 golden 枚举合一 → #480 拆 CLI → 本次）。

## 问题

`scripts/xtask.z42`（214 行）的**自述**是：

> 命令树…都在 xtask_cli.z42——**Main 只做 argv 取用与转发**。
> 各命令的 handler 分散在 build/ test/ package/ install/ 子目录。

但它自己就装着 7 个 handler，与这段自述直接矛盾：

| handler | 实际归属 | 说明 |
|---|---|---|
| `_buildRuntime` | build 族 | cargo build z42vm |
| `_featureMatrix` + `_cargoFeature` | build 族 | 逐 cargo feature 组合编译 |
| `_buildStdlib` | build 族 | 3 行 shim → `_buildStdlibCore`（在 `build/xtask_stdlib.z42`） |
| `_regenCore` | build 族 | golden 资产重建，与 `_regenGolden`（`build/xtask_test_assets.z42`）成对 |
| `_testChanged` | test 族 | 纯 ParseResult 适配器 → `_testChangedRun`（`test/xtask_test_changed.z42`） |
| `_depsCheck` | deps | 纯 ParseResult 适配器 → `_depsCheckRun`（`xtask_deps.z42`） |
| `_clean` + `_rmIfExists` | build 族 | 删的全是 `artifacts/build/` 下的产物 |

最刺眼的是那几个**纯适配器**：`_testChanged` / `_depsCheck` 只做「从 ParseResult 取参数 →
调真正的 `_xxxRun`」，而 `_xxxRun` 就在另一个文件里——一个逻辑被切成两半放在两个目录。

## 方案

按族归位，`xtask.z42` **214 → 64 行**，只剩入口：

| 去向 | 内容 |
|---|---|
| `scripts/build/xtask_runtime.z42`（新） | `_buildRuntime` + `_featureMatrix` + `_cargoFeature` |
| `scripts/build/xtask_clean.z42`（新） | `_clean` + `_rmIfExists` |
| `scripts/build/xtask_stdlib.z42` | += `_buildStdlib`（与 `_buildStdlibCore` 并排） |
| `scripts/build/xtask_test_assets.z42` | += `_regenCore`（与 `_regenGolden` 并排） |
| `scripts/test/xtask_test_changed.z42` | += `_testChanged`（与 `_testChangedRun` 并排） |
| `scripts/xtask_deps.z42` | += `_depsCheck`（与 `_depsCheckRun` 并排） |

`xtask.z42` 保留 `Main` 与 **`run` 直通**——后者是 `_runCli` 在 `Resolve` **之前**就拦截的
裸直通（launcher 自己处理 `--` 尾巴，严格 ArgParser 表达不了），属**入口层**而非某个命令族。

namespace 扁平（`Z42Xtask`）+ `include = ["**/*.z42"]` → 搬家不改任何调用点。

### 刻意没做的事

**没有**顺手删掉 `_buildStdlib` 这个 3 行 shim（直接让 dispatch 调 `_buildStdlibCore`）。
理由：那要改 `_dispatchBuild`，而它正被 in-flight 的 **PR #480** 从 `xtask_cli.z42` 搬进
`scripts/cli/xtask_cli_build.z42` —— 两个 PR 会在同一函数上打架。搬完之后 shim 与
`_buildStdlibCore` 并排放着，冗余一眼可见，等 #480 落地后一行即可清掉。

## 验证

1. **CLI 面不变**：沿用 #480 的 harness dump 整棵命令树 help（49 条命令）→ 与基线
   `diff` **0 行**（这次顺带也证明了 #473 的 bench 迁移没动命令面）。
2. **纯搬家的编译期保证**：namespace 扁平 + 裸名互调，函数少一个就是 `E0401 undefined`，
   编译通过即证明调用点全部仍解析得到。
3. `xtask test` 全绿 10/10 stage（GREEN gate 本身就跑遍 `build stdlib` / `build runtime` /
   `build test` / `test changed` 这几条被搬动的路径）。

> 搬动时 `E0436` 提示了两处漏掉的 `using Std.Cli;`（`ParseResult` 在 `Std.Cli`）。
> 顺手核了一遍：`xtask_stdlib.z42` / `xtask_test_assets.z42` 收到的两个函数**不吃**
> ParseResult，所以那两个文件不加这条 using（先加后删，别留无用 import）。

## 状态

🟢 完成
