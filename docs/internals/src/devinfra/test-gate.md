# 测试门禁（GREEN gate）

> 对齐：2026-09-17（change `restructure-docs-three-books`）｜ 代码：`scripts/test/xtask_test.z42`（gate 编排 + stage 清单）、`scripts/test/xtask_test_*.z42`（各 stage）
>
> 怎么写 `[Test]`、`z42 test` 怎么用 → 参考手册；本页写 **gate 由哪些 stage 组成、谁保证这份清单不漂移、加一个 stage 要改哪两处**。

裸 `xtask test` 就是提交门禁：串联全部必跑 stage，任一失败即停；全绿是 commit / push 的先决条件。
本页的 §1 是 **GREEN gate stage 组成的唯一权威清单**——其他文档一律「跑 `xtask test`，
组成见此」加链接，不再各自复列。

## 1. 完整 gate 的 stage 流水

```mermaid
graph LR
    B[stdlib + z42c 自建] --> D[debug z42vm<br/>+ compression cdylib]
    D --> R[regen 构建波<br/>cargo release z42vm<br/>+ golden .zbc]
    R --> S1[e2e goldens<br/>interp]
    S1 --> S2[e2e cross-zpkg<br/>编译=release·运行=debug]
    S2 --> S2b[e2e multi-exe<br/>一工程 → N 个 exe zpkg]
    S2b --> S3[stdlib Test 用例]
    S3 --> S3a2[stdlib Benchmark<br/>语料能跑 · 不判时间]
    S3a2 --> S3b[manifest targets<br/>&#91;&#91;test&#93;&#93; + &#91;&#91;example&#93;&#93; fixture]
    S3b --> S3c[examples<br/>书↔示例引用 + SDK 重放 .console]
    S3c --> S3d[docs<br/>相对链接可解析]
    S3d --> S4[compiler 自举<br/>后端三包 + 不动点 + units]
    S4 --> S4g[gc modes<br/>z42c.semantics 在各 GC 模式下重编]
    S4g --> S5[vscode-syntax<br/>grammar ↔ Lexer 防漂移]
    S5 --> S6[lines<br/>文件行数硬上限棘轮]
    S6 --> S7[walkers<br/>AST walker 完备性]
    S7 --> G((GREEN))
```

**机器可读清单**（`_checkGateStageDoc` 解析此区；条目文本 = `_stageStart` 打的 banner 名，
顺序 = 实际执行顺序。仅改这里而不改 `_gateStageNames()` 会让 gate 变红，反之亦然）：

<!-- gate-stages:begin -->
- `build wave (debug vm + regen)`
- `e2e goldens (interp; jit → vm-jit-consistency)`
- `e2e cross-zpkg`
- `e2e multi-exe`
- `stdlib [Test]`
- `stdlib [Benchmark]`
- `manifest targets ([[test]] + [[example]])`
- `examples (learn book transcripts)`
- `docs (relative links)`
- `compiler`
- `gc modes (z42c.semantics build)`
- `vscode-syntax`
- `lines`
- `walkers`
<!-- gate-stages:end -->

先备工具链与基线（build wave），再依序跑其余验证 stage；任一步失败立即终止。

## 2. 这份清单为什么有一道门守着

本节此前自称 SoT，却仍然烂了：multi-exe、manifest targets、examples 三次加 stage 都没同步本页，
文档停在 6 个而 gate 实跑 9 个。**纯纪律守不住一份没有测试盯着的清单**，所以把「gate 跑哪些 stage」
变成数据 + 开跑前对账。两个环扣在一起：

| 环 | 谁 | 保证什么 |
|---|---|---|
| ① | `_stageStart` 的注册校验 | banner 名必须已在 `_gateStageNames()` 里登记，否则当场抛异常 —— 挡住「加了 stage 却没进代码侧清单」 |
| ② | `_checkGateStageDoc` | 逐条对账 `_gateStageNames()` 与本页 `gate-stages` 区，不一致即红并打印两侧差异 |

**加 / 删 / 改名一个 stage，要同时改 `_gateStageNames()`（`scripts/test/xtask_test.z42`）与本页的
`gate-stages` 区**，两处一致才绿。对账跑在构建波**之前**（几毫秒）——文档漂移不该让人等十几分钟
的 gate 跑完才发现。`gate-stages` 区缺失标记时报的是「标记不见了」而不是「清单为空」：
两种失败的修法不同，混在一起会让人去改清单而不是去找被删掉的标记。

形态照 `vscode-syntax`（生成产物 ↔ 源表防漂移）。

## 3. 顺序里的两条硬约束

**debug z42vm 必须先于 regen 构建。** golden regen 用 `_activeVm(root, "debug")` 解析 debug vm 去
编译各 golden；若 regen 先跑而 debug vm 陈旧（早于某个新增的 VM builtin），会在 regen 阶段
panic「unknown builtin」→ 全体 golden 假失败，且 regen 返回 1 早退，使 debug vm 那一步永远跑不到。
故 `_testAll` / `_testE2eCore` 把 `_buildDebugVmAndCompression()` 排在 `_regenForTest()` 之前。

**cross-zpkg 的 fixture 编译走 release z42vm、运行走 debug z42vm。** 该 stage 每个 fixture 要三阶段
z42c 编译（target → ext → main，全跑约 40 次），debug VM 编译慢 release 一个量级，曾是
`test-host(linux-x64)` 的 pole（约 18 min）。debug VM 的价值（overflow-checks + 内存布局
`debug_assert!`）在**执行期**触发，编译走 debug 零覆盖价值，故 `_runOneCrossCase` 用 release VM 编
fixture、debug VM 跑 `main.zpkg`——跨包 dispatch 的 debug 断言覆盖不丢，pole 消失（约 2–3 min）。
`--toolchain` 消费路径只带 release VM，编译与运行同为它。

## 4. 各 stage 守的是哪一类回归

| stage | 守什么 | 成本 / 性质 |
|---|---|---|
| `e2e goldens` | 端到端语义。**只跑 interp**——cranelift JIT 行为 host-independent，jit 由专门的 CI 腿覆盖，在每个 OS 腿都跑一遍是 5× 冗余 | 主力 stage |
| `e2e cross-zpkg` / `multi-exe` | 跨包行为；一工程产 N 个 exe zpkg | 见 §3 |
| `stdlib [Test]` | 库正确性 | 最大的一块之一 |
| `stdlib [Benchmark]` | **只验语料能不能跑**，不看快慢 | 全量约 14.6 s、零噪声、host-independent |
| `manifest targets` | 清单驱动的 `[[test]]` / `[[example]]` target 契约 | — |
| `examples` | 学习手册示例逐条可运行且与书一致（书 ↔ 示例引用 + 用 SDK 里真实的 `z42` 重放每个 `.console`） | 完整 gate 先 `build sdk`；`--no-build` 缺 SDK 即红 |
| `docs` | 相对 markdown 链接可解析（棘轮：存量死链在 `scripts/test/doc-link-baseline.txt` 里只 warn，基线之外的新死链判红） | 秒级，不需要 SDK |
| `compiler` | 编译器自举不动点 gen1 == gen2 + units（见[构建编排](build.md)）| host-independent |
| `gc modes` | 在每种 GC 模式下、把收集器调到触发上百次，重编一个真实包（`z42c.semantics`），每模式带一个收集次数下限断言 | 三个提前回收缺陷都是这么现形的：编译器在悬垂引用上崩在半途 |
| `vscode-syntax` | 生成产物一致性：`z42.tmLanguage.json` 必须等于「当前 Lexer 关键字表 + 模板」的重渲染 | 约一次 z42c fork |
| `lines` | 文件行数上限棘轮，见下 | 纯文本扫描 < 1 s |
| `walkers` | z42c 里**手写穷举** AST walker 的完备性，见下 | 纯文本扫描 < 1 s |

**`stdlib [Benchmark]` 为什么必须在 gate 里**：bench 语料此前唯一的看门人是 `bench-pr.yml`，
而那个 job **不在分支保护的 required 列表里**。一次把 `Failure.z42` 搬出 `z42.test` 的改动让
14/19 个 bench 文件当场跑不起来，门红了、PR 照合，此后连红 3 个 PR 无人过问——**会红但不挡人的门
等价于没有门**。把它提进 required 不可行：`bench-pr.yml` 是 path-filtered，纯文档 PR 上根本不触发
⇒ required check 恒 pending ⇒ PR 永远合不了。正确的收口是拆两层：「语料能不能跑」是确定性事实，
归本 stage；「跑多快」留给[性能门禁](benchmarking.md)那套 A/B 判红（噪声治理是另一条线）。

**`lines` 是两档**（`_lineLimitHard()` / `_lineLimitSoft()`，`scripts/test/xtask_test_lines.z42`）：

- **硬限 886 行**——不在基线的新越界文件、或比基线更长的已知越界文件 → **红**；
- **软限 500 行**——只打一行 advisory 计数，**不进棘轮、不阻断**。

扫 `src/` 下的非测试 `.z42` / `.rs`，对照 `scripts/test/line-limit-baseline.txt`（首行即注明硬限值）。
拆分后降到硬限以下的文件用 `xtask test lines --update` 从基线剔除（只降不升）。
注意软限从来不变红：写个 600 行的新文件 gate 并不会拦。

**`walkers` 是活体对账**（`scripts/test/xtask_test_walkers.z42`）：扫 `src/libraries/z42c.syntax/src`
里节点类的全集（`Expr` / `Stmt` / `Pattern` / `TypeExpr` 的子类），逐个登记的 walker 文件里找
`is <类名>`；全集里既不被匹配、又不在该 walker 白名单里的类 → **红**。登记表是 `_walkerRegistry()`
（当前四个：`MethodTypeParamUse.Consumes` / `ExprTyper._bindExpr` / `StmtBinder._bindStmt` /
`PatternBinder.Bind`），加新 walker 加一行。**不硬编码计数**——那种计数本身在漂。

**Rust VM 单测（`test runtime` = `cargo test`）不在 gate 内**：它的 `signal_handler_e2e` 会 spawn
信号崩溃 helper，在信号受限的沙箱里挂住，会让这个「永远要跑」的 gate 不可用。改由每条 CI 腿单独
一步 + 本地按需 `xtask test runtime` + `test changed` 覆盖。

## 5. `--skip`：只改「在哪跑」，不改 gate 的组成

除 build wave 与 `e2e goldens` 外，其余 stage 都可经 `--skip <csv>` 下放到独立 CI job
（`_skipHas`）。skip 名是短名，**不等于 banner 全名**：`cross-zpkg` / `multi-exe` / `stdlib` /
`bench` / `targets` / `examples` / `docs` / `compiler` / `gcgen` / `vscode` / `lines` / `walkers`。

skip 只影响**在哪跑**，不改变 gate 的 stage 组成，所以 §1 的清单不随 `--skip` 变化，
`_checkGateStageDoc` 也照常对全量清单对账。

`--no-build`（或 `--toolchain <sdk>`）跳过构建波、直接消费既有产物——CI 正是先集中构建一次、
再多 job `test all --no-build` 消费的形态；本地缓存后反复迭代同理。
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
总时长；想知道「哪个 stage 吃掉了墙钟」必须本地复现或调高 verbosity 重跑一次。stage 数是个位数、
边界天然清晰，多打 N 行的成本远低于「排查 CI 变慢得先重跑一次」的成本。
**构建波也计入**——它常是最大的一块，不计的话各 stage 之和对不上 TOTAL，反而误导。

实现：`StageLogZ` + `_stageStart` / `_stageEnd` / `_stageSummary`，时长格式化 `_fmtDur`
（`scripts/common/xtask_common.z42`）。

## 7. `test changed`：命令级按需计划

对未提交改动（相对 `BASE`，默认 `HEAD`；含 untracked）逐文件分类，产出**去重后的命令并集**，
依序执行、首败短路。`--dry-run` 只打印计划。映射表（`_mapFile`，
`scripts/test/xtask_test_changed.z42`）：

| 改动路径 | 映射命令 |
|---|---|
| `src/libraries/<lib>/src/` | `test stdlib <lib>` + `test e2e` |
| `src/libraries/<lib>/tests/` 或该库 `.toml` | `test stdlib <lib>` |
| `src/libraries/<lib>/bench/` | `bench stdlib <lib>` |
| `src/runtime/src/`、`Cargo.toml/lock`、`build.rs` | `test runtime` + `test e2e` |
| `src/runtime/tests/` | `test runtime` |
| `src/tests/cross-zpkg/` | `test e2e --dir cross-zpkg` |
| 其余 `src/tests/` | `test e2e` |
| `src/compiler/` | `test compiler` + `test e2e` |
| `src/toolchain/` | `test stdlib`（工具链影响 `[Test]` 的执行方式，全库扫）|
| `examples/<part>/<chapter>/…` | `test examples <part>/<chapter>` |
| `docs/learn/` | `test examples --book-only` |
| `src/toolchain/launcher/`、`src/toolchain/builder/` | 追加 `test examples`（命令行输出一变，手册里的会话脚本就失配）|
| `scripts/xtask*`、`*.workspace.toml`、未识别路径 | **full**（坍缩为 `test all`）|
| 其余文档 / `.claude/` / artifacts | 跳过 |

设计取向是**宁可多跑不可漏跑**：任一未识别路径即保守坍缩为完整 `test all`。
计划里的逻辑命令**在进程内重入 CLI 路由**（不 shell out），免去每命令一次进程启动；cargo 命令例外
走子进程。

## 8. 反射 runner 的输出格式（`z42b test` / `z42b bench`）

stdlib 与 compiler stage 驱动的 `Std.Test.Runner` 支持两种输出，经 `--format` 选择（默认 `pretty`）：

- **pretty**：逐条 `PASS` / `FAIL` / `SKIP` + `Result:` 汇总，供人眼与 gate 退出码判定。
- **json**：单个 `TestReport` 对象 `{tool, module, summary{total,passed,failed,skipped}, results[]}`；
  每条 result 带 `is_benchmark`，benchmark 还带
  `bench_stats{label, min_ns, median_ns, max_ns, samples}`。

**bench 结构化数据流**：benchmark 经 `Bencher.printSummary` 打出一行
`bench[<label>] min=… median=… max=…` → json 模式下 `Runner` 用 `TestIO.captureStdout` 捕获该
benchmark 的 stdout → `BenchStats.parse` 解析为结构化字段（malformed → `null`，不降级 sentinel）
→ 汇入 `TestReport`（手写 JSON，因为 `z42.test` 是基础库、刻意不依赖 `z42.json`）。
格式契约的两端——产出端 `Bencher.printSummary` ↔ 消费端 `BenchStats.parse`，同住
`src/libraries/z42.test/`——改格式须同提交，`tests/bench_stats.z42` 兜底防漂移。
pretty 模式不捕获、逐字节不变。

## 9. 边界与限制

- 单 stage / `--no-build` / `test changed` 均**不构成 GREEN**——提交判定只认完整 `xtask test`。
- `test changed` 只看工作区相对 BASE 的 diff，不理解语义依赖（靠保守坍缩弥补）。
- JIT 一致性不在本地默认路径内，由 CI 专腿覆盖（本地可 `test e2e --mode jit` 手动跑）。
- stage 全串行；无依赖的 stage（compiler ∥ stdlib 等）并发执行尚未实施。
