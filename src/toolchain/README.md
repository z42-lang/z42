# src/toolchain — z42 配套工具链

## 职责

围绕 `compiler/` 与 `runtime/` 的配套工具集合：宿主集成、调试器、应用打包、端到端工作流。不包含语言核心（编译器、VM）与标准库源码（`libraries/`）。

## 子目录

| 目录 | 职责 | 状态 |
|------|------|:----:|
| [launcher/](launcher/) | `z42` launcher（muxer）：原生 trampoline + `launcher.zpkg`（run/link/list/install/export…）+ per-app 原生 apphost（`apphost.z42` patch 库，经 `z42 publish`）。类比 `dotnet` muxer + `rustup` | ✅ 已实装 |
| [builder/](builder/) | `z42b` 构建编排器：读 `z42.toml`/`--rid` 驱动 `z42.build` 管线（compile→trim→assets→workload），launcher 分发调用（`build`/`publish`/`export`）；**兼跑 stdlib/工程的 `[Test]`/`[Benchmark]` 用例（取代原 Rust `z42-test-runner`，`xtask test` 内嵌调用）**。取代原 `packager` 占位 | 占位 |
| [devtools/](devtools/) | `z42d` 开发者工具链（muxer apphost）：`fmt`/`doc`/`dbg`/`prof`/`lint` 统一在单 exe + Std.Cli router 下，launcher 分发（`z42 fmt` → `z42d fmt`）。收编原独立 `z42-fmt`/`z42-doc`/`z42-lint` 规划 | 占位 |
| [interactive/](interactive/) | `z42i` 交互式 REPL（apphost，非 muxer）：源码片段 → 编译 → VM 求值 → 打印；0.3.x capstone，前置 `extract-compile-pipeline-api` | 占位 |
| [repl/](repl/) | `z42.repl`（lib，`Std.Repl`）：REPL **终端交互层（tier1）**——rustyline 行编辑 + 缩进感知键位。依赖 `z42.scripting`（Completeness）。真 tty + native 行编辑 builtin、平台绑定重 → 留 toolchain（非 stdlib 料）。拆自 scripting（`split-z42-repl`） | ✅ 已实装 |
| [workload/](workload/) | 平台相关能力束（consolidate-platform-into-workload）：`host-api/`（Tier 2 `z42-host` crate）+ `platforms/{ios,android,wasm,desktop}/`（facade + 测试）；按需 `z42 workload install`。host/ 解散后承接 | 🚧 实装中 |

> 命名说明：`toolchain` 取"围绕 compiler/runtime 的整套配套工具"之广义；语言核心**编译器在 [`../compiler/`](../compiler/) + [`../z42c/`](../z42c/)**、VM 在 [`../runtime/`](../runtime/)，不在本目录。

## 构建（add-build-toolchain, 2026-07-05）

对称于 `xtask build compiler|stdlib`：

| 命令 | 产出 |
|------|------|
| `xtask build workload` | `workload/*`（4 个 lib）→ stdlib libs dir（launcher 的依赖） |
| `xtask build toolchain` | launcher/z42b/z42d/z42i 各 `publish <toml>` → **其 `[platform.desktop].publish_dir`**（native apphost + payload）；自动先 `build workload` |
| `xtask build sdk` | 完整可运行 `.z42` SDK —— 把上述 apphost + z42c 从各 `publish_dir` 合并进 SDK |

**路径 SoT**：所有输出/publish 路径从各组件 `z42.toml` 读（`[build].dist_dir`/`output_dir`、`[platform.desktop].publish_dir`，级联默认 `${output_dir}/{dist,publish}`），xtask 不硬编码——改路径只动 toml。实现见 [`scripts/build/xtask_toolchain.z42`](../../scripts/build/xtask_toolchain.z42)。

## 状态

launcher / test-runner 已实装并在 CI / xtask 中使用；workload 实装中（承接 host 解散迁入的 host-api + 平台 facade，consolidate-platform-into-workload）；builder / devtools / interactive 为占位，具体设计与落地时机见 `docs/roadmap.md`。`host/` 顶层已移除——Tier 1 C ABI + 头在 [`../runtime/src/host/`](../runtime/src/host/) + [`../runtime/include/`](../runtime/include/)，Tier 2/Tier 3 在 `workload/`。

> launcher 的演进方向（命令分发三层、平台工程导出、runtime/workload 分发）见 [`docs/design/toolchain/`](../../docs/design/toolchain/)。

## 依赖关系

- 消费：`compiler/`（调用 CLI 或 API）、`runtime/`（嵌入或调用 VM）
- 被消费：`scripts/`（发行与测试脚本可能调用 builder / workload）
