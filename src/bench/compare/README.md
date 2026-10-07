# src/bench/compare/ — 跨语言对标基准

## 职责

`xtask bench compare` 的工作负载：同一个算法分别用 z42、Python 3、JavaScript（Node）、Ruby、
C#（.NET）、Java 写一遍，参数相同，每个程序打印 `RESULT <checksum>`，由 runner 逐一核对。
回答的问题是「z42 和其它脚本 / 托管语言比，在每个领域差多少」——不判红、不进 CI，结果写进
[跨语言对标](../../../docs/internals/src/devinfra/benchmark-compare.md)。
z42 自身的回归门禁在上一级的 `scenarios/`（`xtask bench`），两者互不复用。

## 功能索引

| 功能 | 入口 / 文件 |
|------|-----------|
| 工作负载清单（类别、度量方式、参数、期望 checksum、quick 参数） | `workloads.toml` |
| 一个工作负载的六种实现 | `<name>/main.z42` · `main.py` · `main.js` · `main.rb` · `Main.cs` · `Main.java` |
| C# 的共用工程（编一个 dll，按第一个参数分派到 `<name>.Bench.Run`） | `_dotnet/compare.csproj` + `_dotnet/Program.cs` |
| runner（构建、运行、核对、出报告） | [`scripts/xtask_bench_compare.z42`](../../../scripts/xtask_bench_compare.z42) + `xtask_bench_compare_report.z42` |

工作负载按类别：startup（`hello`）· cpu（`fib` `nbody` `spectral_norm` `mandelbrot` `fannkuch`）·
calls（`vcall_mono` `vcall_poly` `closures`）· strings（`str_builder` `str_split_join` `num_format`）·
collections（`list_ops` `dict_ops` `sort`）· json（`json`）· memory（`density_*` 五条）·
gc（`binary_trees` `large_heap`）· concurrency（`parallel_sum`）。

## 基础用法

```bash
xtask bench compare                       # 全部工作负载 × 本机找得到的全部 runtime，每格 3 次取中位
xtask bench compare --quick               # 冒烟：quick 参数、每格 1 次（约 20 s），checksum 照样核对
xtask bench compare --only fib,json       # 只跑这几条
xtask bench compare --runtimes z42,python # 只比这几个 runtime（另有 z42-stw / z42-interp）
```

外部 runtime 都是**可选**的：`python3` / `node` / `ruby` / `dotnet` / `java`+`javac` 在 PATH 上找不到就
跳过并打印原因，不是仓库构建的依赖。结果在 `artifacts/reports/bench/compare.{json,md}`。

## 如何测试验证

```bash
xtask bench compare --quick   # 每格打印数值；任何一格崩溃或 checksum 不符 ⇒ 退出码 1
```

加 / 改一条工作负载后，先让六种实现在 quick 和完整参数下打印同一个 checksum，再把两个值写进
`workloads.toml`。

## 关联文档

- 度量口径、当前结果与最大差距：[跨语言对标](../../../docs/internals/src/devinfra/benchmark-compare.md)
- z42 自身的回归门禁（另一套，判红）：[性能基准与回归门禁](../../../docs/internals/src/devinfra/benchmarking.md)

## 核心文件

| 文件 | 职责 |
|------|------|
| `workloads.toml` | 唯一的工作负载表：runner 只认这里列出的目录 |
| `<name>/main.z42` | z42 实现（`--opt-all` 编译，`--mode jit` 运行；`z42_stw = true` 的另跑 `gc-mode=stw`） |
| `_dotnet/compare.csproj` | 收录 `../*/Main.cs`；`--artifacts-path` 让 bin/obj 落在 `artifacts/build/bench/compare/dotnet/` |
| `_dotnet/Program.cs` | 分派器：`compare.dll <name> [args]` → `<name>.Bench.Run(args)` |

某个 runtime 缺文件 = 该语言没有对应的惯用标准设施（`workloads.toml` 的 `note` 写明，如 JDK 没有标准
JSON API），runner 记为 n/a 而不是失败。
