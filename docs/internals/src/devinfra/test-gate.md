# 测试门禁（GREEN gate）

> 代码：`scripts/src/test/gate.z42`（gate 编排 + stage 清单）、`scripts/src/test/` 下按被测对象分的子目录（各 stage）
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

`scripts/src/test/gate.z42` 的 `_gateStages` 是唯一执行清单：每条 `GateStage` 同时携带
名称、skip 键、改动分流类别与执行函数。完整测试、`--list` 与 `--changed` 都消费这张表，文档只描述机制，
不维护另一份完整名单。加、删、改名或调整顺序只改这张表；若改变了某类检查的职责，同步本页对应说明。

改动分流类别决定 `--changed` 能否跳过该 stage：不标 = 恒跑（构建波、goldens、秒级的全仓静态检查）；
`rust` = 只随 Rust 运行时改动；`z42` = 执行 z42 代码，编译器 / stdlib / 运行时的改动都会选中它。

`_runGateStages` 在任何构建或测试开始前校验 `--skip`，未知键直接报错；随后按表顺序执行，
成功的 stage 记录耗时，第一个非零退出码原样向上传播，后续 stage 不执行。

## 3. 顺序里的两条硬约束

**debug z42vm 必须先于 regen 构建。** golden regen 用 `_activeVm(root, "debug")` 解析 debug vm 去
编译各 golden；若 regen 先跑而 debug vm 陈旧（早于某个新增的 VM builtin），会在 regen 阶段
panic「unknown builtin」→ 全体 golden 假失败，且 regen 返回 1 早退，使 debug vm 那一步永远跑不到。
故 `_gateBuildWave` / `_testE2eCore` 把 `_buildDebugVmAndCompression()` 排在 `_regenForTest()` 之前。

**cross-zpkg 的 fixture 编译走 release z42vm、运行走 debug z42vm。** 包间有依赖，按
`target` → 可选 `oldtarget` → 可选 `ext` → `main` 分波；各波并发独立夹具。
`_crossBuildWave` 使用共用的 fixture 构建助手，`_crossRunWave` 为每例保存进程句柄与隔离 libs，
按夹具顺序回收结果。debug VM 的 overflow-checks 与 debug assertion 覆盖保留在实际执行阶段。
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

静态检查实现归 `scripts/src/test/check/`，单跑用 `xtask check <name>`，完整测试按执行表调用相同处理函数。
诊断码直接读取登记表、源码、文档与测试面；允许已登记的字面量发码用于种子过渡，不维护站点白名单、
计数快照或到期日期。测试面中的代码注释不算断言，检查器自身也不算覆盖来源。

文档符号引用用 `z42 doc --page <页>` 逐页查看（原 `check doc-refs` 报告已在引用清零后退役——#1749）。运行时的分层设计由
[运行时审计](../../../runtime-audit.md)描述，依赖方向通过代码审查维护。

完整 Rust 测试的 signal-crash helper 不进入本地常跑门禁；完整版本由 CI 独立步骤与 `xtask test runtime` 覆盖。
GREEN 中保留 lib 单测与全部测试 target 的编译检查。两种入口及 GC 模式回归归属
`scripts/src/test/runtime/runner.z42`，共用同一执行器：一次 Cargo 图编出 target，把共享 Rust std 拷到组件旁，
再带组件 / deps / std 的库搜索路径运行 harness（组件测试会 dlopen 现编的探针库，直接 `cargo test` 在 Linux 上加载失败）；
Cargo workspace、feature 与 manifest 配置由同一 helper 提供。
CI 的 `vm` 过滤也包含此执行器文件，执行器改动由完整 Runtime 测试覆盖。

## 5. `--skip`：只改「在哪跑」，不改 gate 的组成

除 build wave 与 `e2e goldens` 外，其余 stage 都可经 `--skip <csv>` 下放到独立 CI job。
skip 键与 banner 名可能不同（如 `vscode` 对应 `vscode-syntax`）；完整键表用 `xtask test --list` 查询。
逗号或空格均可分隔，未知键在构建前报错。skip 只影响在哪跑，不改变完整 stage 表。

`--changed <base>` 是 `--skip` 的自动版：按 §7 映射表的「gate stage」列，把相对 `<base>` 的改动**用不到**的
重 stage（§2 里标了改动分流类别的那些）追加进 skip。build wave、`e2e goldens` 与全部静态检查恒跑（后者守全仓不变量，秒级）；
任一文件判成 full、base 不可解析或 Git 读取失败 ⇒ 一个都不跳。判定打印在开头（`[test --changed] …`）。CI 的
`test-host` 在 PR 上使用 PR base SHA，在 main push 上使用 `github.event.before`；首次 push 或无法取得基线时全跑，
schedule / dispatch 保留全量。测试差异基线不替代 nightly 发布的累计 SDK 差异判定。

`--no-build` 复用已有 z42c / stdlib；完整 `test` 仍确保本地 debug VM 并重生 golden。
专项 e2e 在指定 `--toolchain <sdk>` 时直接使用该工具链的 VM。examples stage 实际执行时仍负责构建 apphost、
组装当前开发 SDK（已有 z42c / stdlib 不重建）；`--toolchain <sdk>` 则直接在指定 SDK 上重放，不准备开发 SDK。
显式 skip、改动分流跳过 examples 或只列 stage 时，均不准备 SDK。
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
（`scripts/src/common/logging.z42`）。

## 7. 按改动挑测试

`test changed [BASE]` 对工作区差异与未跟踪文件生成窄命令；`test --changed <base>` 消费同一配置中的
stage 元数据，保留必跑的构建波、golden 与静态检查。目录归属、影响传播和执行前提的唯一配置是
`scripts/flows.toml`，完整用法见[按改动规划流程](flows.md)。

重命名计算旧、新路径。workspace 成员通过实际清单发现，使用声明的工程名；被删除、排除或未知的归属回退全量。
`test changed` 读取 Git 失败时返回 2；`test --changed` 保留全部 stage 并打印回退原因。
完整 `test` 未覆盖的 SDK 验证、完整 Rust harness 和整程序 benchmark 义务仍保留，不能被 GREEN 吞掉。
本地命令以参数数组重入 CLI，首败短路。普通文档选择真实的文档校验，学习手册与示例保留会话重放。

CI 用 `xtask test --plan artifacts/ci-plan.json` 消费 detect-changes 的同一份、经过提交与配置校验的计划。
`flows` 是完整 GREEN 的必跑 stage，执行配置、Git、计划及 CI 控制回归。

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
- `test changed` 使用显式目录与契约图，不代替编译器语义依赖分析；未知输入保守全量。
- JIT 一致性不在本地默认路径内，由 CI 专腿覆盖（本地可 `test e2e --mode jit` 手动跑）。
- stage 全串行；无依赖的 stage（compiler ∥ stdlib 等）并发执行尚未实施。
