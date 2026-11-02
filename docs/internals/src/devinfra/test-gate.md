# 测试门禁（GREEN gate）

> 代码：`scripts/test/xtask_test.z42`（gate 编排 + stage 清单）、`scripts/test/xtask_test_*.z42`（各 stage）
>
> 怎么写 `[Test]`、`z42 test` 怎么用 → 参考手册；本页写 gate 的编排机制、各类检查的职责与 CI 分工。

裸 `xtask test` 就是提交门禁：串联全部必跑 stage，任一失败即停；全绿是 commit / push 的先决条件。

## 1. 查看完整 gate 的 stage 流水

```bash
xtask test --list
```

按执行顺序打印每个 stage 的名称和 `--skip` 键；`(required)` 表示不可跳过。
该命令只读 stage 元数据，不构建、不运行测试，也不要求构建树已有产物。
先备工具链与基线（build wave），再依序跑其余验证 stage；任一步失败立即终止。

## 2. 登记与执行共用一张表

`scripts/test/xtask_test.z42` 的 `_gateStages` 是唯一执行清单：每条 `GateStage` 同时携带
名称、skip 键与执行函数。完整测试与 `--list` 都消费这张表，文档只描述机制，不维护另一份完整名单。
加、删、改名或调整顺序只改这张表；若改变了某类检查的职责，同步本页对应说明。

`_runGateStages` 在任何构建或测试开始前校验 `--skip`，未知键直接报错；随后按表顺序执行，
成功的 stage 记录耗时，第一个非零退出码原样向上传播，后续 stage 不执行。

## 3. 顺序里的两条硬约束

**debug z42vm 必须先于 regen 构建。** golden regen 用 `_activeVm(root, "debug")` 解析 debug vm 去
编译各 golden；若 regen 先跑而 debug vm 陈旧（早于某个新增的 VM builtin），会在 regen 阶段
panic「unknown builtin」→ 全体 golden 假失败，且 regen 返回 1 早退，使 debug vm 那一步永远跑不到。
故 `_gateBuildWave` / `_testE2eCore` 把 `_buildDebugVmAndCompression()` 排在 `_regenForTest()` 之前。

**cross-zpkg 的 fixture 编译走 release z42vm、运行走 debug z42vm。** 该 stage 每个 fixture 要三阶段
z42c 编译（target → ext → main，全跑约 40 次），debug VM 编译慢 release 一个量级，会成为
`test-host(linux-x64)` 的 pole（约 18 min）。debug VM 的价值（overflow-checks + 内存布局
`debug_assert!`）在**执行期**触发，编译走 debug 零覆盖价值，故 `_runOneCrossCase` 用 release VM 编
fixture、debug VM 跑 `main.zpkg`——跨包 dispatch 的 debug 断言覆盖不丢，pole 消失（约 2–3 min）。
`--toolchain` 消费路径只带 release VM，编译与运行同为它。

## 4. 检查职责

| 类别 | 检查内容 |
|---|---|
| 语言与运行时 | golden（interp）、cross-zpkg、multi-exe；JIT 由 CI 专腿覆盖 |
| 库与工具链 | stdlib / toolchain 单元、benchmark 语料可运行、manifest targets、学习手册会话 |
| 编译器 | 自举不动点与单元 / CLI 夹具；真实编译负载下的 GC 模式验证 |
| 静态一致性 | VSCode grammar 与 Lexer、CI shell 变量顺序、子进程环境入口、测试布局 / 能力、版本投影 |
| 文档 | 相对链接与文档结构；stdlib 文档注释的 W08xx 检查 |
| 诊断码 | 登记唯一性、字面量与常量引用有效、码表集合与活性状态一致、活码有测试按码断言 |

静态检查实现归 `scripts/check/`，单跑用 `xtask check <name>`，完整测试按执行表调用相同处理函数。
诊断码直接读取登记表、源码、文档与测试面；允许已登记的字面量发码用于种子过渡，不维护站点白名单、
计数快照或到期日期。测试面中的代码注释不算断言，检查器自身也不算覆盖来源。

文档符号引用用 `xtask check doc-refs` 手动查看报告，不参与 CI 的通过判定。运行时的分层设计由
[运行时审计](../../../runtime-audit.md)描述，依赖方向通过代码审查维护。

完整 Rust 测试的 signal-crash helper 不进入本地常跑门禁；完整版本由 CI 独立步骤与 `xtask test runtime` 覆盖。
GREEN 中保留 `cargo test --lib` 与全部测试 target 的编译检查。

## 5. `--skip`：只改「在哪跑」，不改 gate 的组成

除 build wave 与 `e2e goldens` 外，其余 stage 都可经 `--skip <csv>` 下放到独立 CI job。
skip 键与 banner 名可能不同（如 `vscode` 对应 `vscode-syntax`）；完整键表用 `xtask test --list` 查询。
逗号或空格均可分隔，未知键在构建前报错。skip 只影响在哪跑，不改变完整 stage 表。

`--changed <base>` 是 `--skip` 的自动版：按 §7 映射表的「gate stage」列，把相对 `<base>` 的改动**用不到**的
重 stage（`rust-units` / `cross-zpkg` / `multi-exe` / `stdlib` / `toolchain` / `bench` / `targets` / `examples` /
`compiler` / `gcgen`）追加进 skip。build wave、`e2e goldens` 与全部静态检查恒跑（后者守全仓不变量，秒级）；
任一文件判成 full、或 base 不可解析 ⇒ 一个都不跳。判定打印在开头（`[test --changed] …`）。CI 只在 PR 的
`test-host` 上用它（`--changed <PR base sha>`），main push / schedule / dispatch 全跑。

`--no-build`（或 `--toolchain <sdk>`）跳过构建波、直接消费既有产物——CI 正是先集中构建一次、
再多 job `test --no-build` 消费的形态；本地缓存后反复迭代同理。
**这些都不构成 GREEN**：提交判定只认完整 `xtask test`。

## 6. stage 耗时归因（无条件输出）

每个 stage 结束时打印自身墙钟，gate 末尾再给一张**降序**表 + 占比：

```
── stage wall-clock (降序) ──
  build wave (debug vm + regen)                   2m47s  (47%)
  stdlib [Test]                                   1m23s  (23%)
  compiler                                        51.7s  (14%)
  ...
  TOTAL                                           5m52s
```

**为什么无条件打印**（而不是挂在 `-v diagnostic` 下）：`_procStart` / `_procEnd` 的耗时只在
verbosity ≥ 4 才输出，而 CI 跑的是默认 verbosity——于是 `xtask test` 在 CI 日志里是个黑盒，只有一个
总时长；想知道「哪个 stage 吃掉了墙钟」必须本地复现或调高 verbosity 重跑一次。stage 数量有限、
边界天然清晰，多打 N 行的成本远低于「排查 CI 变慢得先重跑一次」的成本。
**构建波也计入**——它常是最大的一块，不计的话各 stage 之和对不上 TOTAL，反而误导。

实现：`StageLogZ` + `_stageStart` / `_stageEnd` / `_stageSummary`，时长格式化 `_fmtDur`
（`scripts/common/xtask_common.z42`）。

## 7. `test changed`：按改动挑测试（命令计划 + gate 分流共用一张表）

对未提交改动（相对 `BASE`，默认 `HEAD`；含 untracked）逐文件分类。映射表（`_mapFile`，
`scripts/test/xtask_test_changed.z42`）每条规则给两列：本地 `test changed` 执行的**窄命令**（去重后的并集，
依序执行、首败短路；`--dry-run` 只打印计划，并顺带打印下一列的分流结果），与 `xtask test --changed <base>`
（§5）用的**受影响 gate stage**。「全集」= 所有编译 / 执行 z42 代码的重 stage（`rust-units` 除外）。

| 改动路径 | 窄命令 | gate stage |
|---|---|---|
| `src/libraries/<lib>/src/`、该库 `.toml` | `test stdlib <lib>`（`src/` 另加 `test e2e`）| 全集 |
| `src/libraries/<lib>/tests/` | `test stdlib <lib>` | `stdlib` |
| `src/libraries/<lib>/benches/` | `bench stdlib <lib>` | `bench` |
| `src/runtime/src/`、`Cargo.toml/lock`、`build.rs` | `test runtime` + `test e2e` | 全集 + `rust-units` |
| `src/runtime/tests/` | `test runtime` | `rust-units` |
| `src/compiler/z42c.pipeline/tests/fixtures/cross-zpkg/` | `test e2e --dir cross-zpkg` | `cross-zpkg` |
| `src/compiler/z42c.pipeline/tests/fixtures/multi-exe/` | `test e2e --dir multi-exe` | `multi-exe` |
| `src/toolchain/builder/tests/fixtures/` | `test toolchain builder` | `toolchain`、`targets` |
| 其余 `src/tests/` | `test e2e` + `check layout` | —（goldens 与 layout 恒跑）|
| `src/benches/` | `bench --quick` | —（gate 里没有它）|
| `src/compiler/` | `test compiler` + `test e2e` | 全集 |
| `src/toolchain/launcher/`、`src/toolchain/builder/` | `test stdlib` + `test toolchain` + `test docs examples`（命令行输出一变，手册里的会话脚本就失配）| `stdlib`、`toolchain`、`bench`、`targets`、`examples`（z42b 是这几种用例的执行器）|
| 其余 `src/toolchain/` | `test stdlib`（工具链影响 `[Test]` 的执行方式，全库扫）+ `test toolchain` | `stdlib`、`toolchain` |
| `examples/<part>/<chapter>/…` | `test docs examples <part>/<chapter>` | `examples` |
| `docs/learn/` | `test docs examples --book-only` | `examples` |
| `scripts/`、`*.workspace.toml`、其余 `src/`、未识别路径 | **full**（坍缩为 `test`）| **full**（不跳）|
| 其余文档 / artifacts | 跳过 | — |

设计取向是**宁可多跑不可漏跑**：任一未识别路径即保守坍缩为完整 `test`。
计划里的逻辑命令**在进程内重入 CLI 路由**（不 shell out），免去每命令一次进程启动；cargo 命令例外
走子进程。

## 8. 反射 runner 的输出格式（`z42b test` / `z42b bench`）

stdlib 与 compiler stage 驱动的 `Std.Harness.Runner` 支持两种输出，经 `--format` 选择（默认 `pretty`）：

- **pretty**：逐条 `PASS` / `FAIL` / `SKIP` + `Result:` 汇总，供人眼与 gate 退出码判定。
- **json**：单个 `TestReport` 对象 `{tool, module, summary{total,passed,failed,skipped}, results[]}`；
  每条 result 带 `is_benchmark`，benchmark 还带
  `bench_stats{label, min_ns, median_ns, max_ns, samples}`。

**bench 结构化数据流**：benchmark 经 `Bencher.printSummary` 打出一行
`bench[<label>] min=… median=… max=…` → json 模式下 `Runner` 用 `TestIO.captureStdout` 捕获该
benchmark 的 stdout → `BenchStats.parse` 解析为结构化字段（malformed → `null`，不降级 sentinel）
→ 汇入 `TestReport`（手写 JSON，因为 `z42.harness` 是基础库、刻意不依赖 `z42.json`）。
格式契约的两端——产出端 `Bencher.printSummary` ↔ 消费端 `BenchStats.parse`，同住
`src/libraries/z42.harness/`——改格式须同提交，`tests/bench_stats.z42` 兜底防漂移。
pretty 模式不捕获、逐字节不变。

## 9. 边界与限制

- 单 stage / `--no-build` / `test changed` 均**不构成 GREEN**——提交判定只认完整 `xtask test`。
- `test changed` 只看工作区相对 BASE 的 diff，不理解语义依赖（靠保守坍缩弥补）。
- JIT 一致性不在本地默认路径内，由 CI 专腿覆盖（本地可 `test e2e --mode jit` 手动跑）。
- stage 全串行；无依赖的 stage（compiler ∥ stdlib 等）并发执行尚未实施。
