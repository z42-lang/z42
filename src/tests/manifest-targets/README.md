# manifest-targets

## 职责
清单声明式 test / bench / example 目标的端到端夹具（add-tests-bench-manifest-config P4）。
每个子目录是一个真实工程，其 `z42.toml` 声明 `[[test]]`/`[[bench]]`/`[[example]]` 目标 +
`[tests]`/`[benches]`/`[examples]` 段，用来验证 xtask 的清单驱动发现 / harness 分派 /
退出码判定 / 三层 dev-dep 合并 / 具名过滤 / 显式覆盖同名 auto。**不**参与 golden regen/run、
不进 stdlib dist、不打包（`_isNonRegenCat` / `_isNonRunnableCat` 已排除本目录）。

## 功能索引
| 覆盖点 | 承载 |
|--------|------|
| harness=true 反射（[Test]） | `basic/tests/unit_ok.z42`（显式）+ `basic/tests/auto_conv.z42`（约定） |
| harness=false 退出码 | `basic/tests/exit_ok.z42`（`entry=MtExit.Main`） |
| [[bench]] 反射（[Benchmark]） | `basic/bench/micro.z42` |
| [[example]] 编译门禁 + test=true 执行 | `basic/examples/hello.z42` |
| 显式覆盖同名 auto | `unit_ok` / `exit_ok` 目标名 == 文件 stem → 覆盖 auto 单元 |
| compile-then-test（`z42b test <toml>`） | `compile-then-test/`——纯 test 工程（**不**声明 `[[test]]`，故被清单引擎跳过），由 fixtures stage 的 `_smokeCompileThenTest` 单独驱动，验证 toml→build→反射跑（`add-z42b-compile-then-test`）|

## 如何测试验证
    xtask test targets            # 跑本目录所有 [[test]] 目标（harness 两态）
    xtask test targets exit_ok    # 只跑名为 exit_ok 的目标（具名精确）
    xtask bench targets           # 跑所有 [[bench]] 目标
    xtask example                 # 编译所有 example，跑 test=true 的
    xtask example hello           # 编译并运行单个 example
    xtask test                    # 全 gate（含 `manifest targets` + `examples` stage）

## 关联文档
- 设计：`docs/design/compiler/project.md` L5b（目标模型 / harness / exit-code / 约定发现）
- 引入：change `add-tests-bench-manifest-config`（`docs/spec/changes/`）

## 核心文件
| 文件 | 职责 |
|------|------|
| `basic/z42.toml` | 声明四类目标 + 依赖，驱动发现 |
| `basic/tests/*.z42` | harness 两态 + 约定 auto 单元 |
| `basic/bench/micro.z42` | [Benchmark] 反射目标 |
| `basic/examples/hello.z42` | example（编译门禁 + test=true 执行） |
| `compile-then-test/{z42.toml, src/Tests.z42}` | z42b compile-then-test 冒烟夹具（自由函数 `[Test]`；由 `_smokeCompileThenTest` 经 `z42b test <toml>` 驱动） |
