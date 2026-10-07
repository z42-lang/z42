# src/toolchain — z42 配套工具链

## 职责

围绕 `compiler/` 与 `runtime/` 的配套工具集合：`z42` 命令入口、构建编排、开发者工具、REPL、平台 workload（宿主集成与应用打包）。不包含语言核心（编译器、VM）与标准库源码（`libraries/`）。

## 子目录

| 目录 | 职责 |
|------|------|
| [launcher/](launcher/) | `z42` launcher（muxer）：原生 trampoline + `launcher.zpkg`（run / publish / export / workload / 转发 z42b·z42c·z42i）+ per-app 原生 apphost |
| [builder/](builder/) | `z42b` 构建编排器：读 `z42.toml` / `--rid` 驱动 `z42.build` 管线（compile → trim → assets → workload），launcher 分发调用（`new` / `build` / `publish` / `export` / `test` / `bench`）；兼作 stdlib / 工程 `[Test]` / `[Benchmark]` 的反射运行器 |
| [devtools/](devtools/) | `z42d` 开发者工具（muxer apphost）：已有 `symbolicate`、`install`；`fmt` / `doc` / `dbg` / `prof` / `lint` 为已登记命令面的待办 |
| [devtools/vscode/](devtools/vscode/) | VSCode 语法高亮资产包 |
| [interactive/](interactive/) | `z42i` 交互式 REPL（apphost，非 muxer）：片段 → 编译 → VM 求值 → 打印 |
| [interactive/repl/](interactive/repl/) | `z42.repl`（`Std.Repl`）：REPL 终端交互层（rustyline 行编辑 + 缩进感知键位） |
| [workload/](workload/) | 平台 / 能力 workload：`{ios,android,wasm,desktop}/`（appbuilder · platform · template）+ `test/`（on-device test-agent）+ `platform-contract.md`；按需 `z42 workload install` |

> 语言核心**编译器在 [`../compiler/`](../compiler/)**、VM 在 [`../runtime/`](../runtime/)、标准库在 [`../libraries/`](../libraries/)，不在本目录。

## 构建

对称于 `xtask build compiler|stdlib`：

| 命令 | 产出 |
|------|------|
| `xtask build workload` | `workload/*` 各平台库 → stdlib libs dir（launcher 的依赖） |
| `xtask build toolchain` | launcher/z42b/z42d/z42i（外加 z42c 驱动）各 `publish <toml>` → **其 `[platform.desktop].publish_dir`**（native apphost + payload）；自动先 `build workload` |
| `xtask package dev-sdk` | 完整可运行 `.z42` SDK —— 只组装：把上述 apphost 从各 `publish_dir` 与已编好的 z42c / stdlib / z42vm 合进 SDK |

**路径 SoT**：所有输出/publish 路径从各组件 `z42.toml` 读（`[build].dist_dir`/`output_dir`、`[platform.desktop].publish_dir`，级联默认 `${output_dir}/{dist,publish}`），xtask 不硬编码——改路径只动 toml。实现见 [`scripts/build/xtask_toolchain.z42`](../../scripts/build/xtask_toolchain.z42)。

本目录无 `host/`——Tier 1 C ABI + 头在 [`../runtime/src/host/`](../runtime/src/host/) + [`../runtime/include/`](../runtime/include/)，Tier 2 `z42-host` crate 在 `src/runtime/crates/z42-host`，Tier 3 平台绑定在 `workload/`。

启动器的命令分发、平台工程导出、runtime / workload 分发等机制见 [`docs/internals/src/toolchain/`](../../docs/internals/src/toolchain/)。

## 如何测试验证

```bash
xtask build toolchain      # 各组件 publish 成功即编译通过
xtask package sdk --verify            # 打包后 launcher / apphost 冒烟
xtask test toolchain builder         # z42b 夹具
```

各组件专项验证见其 README。

## 依赖关系

- 消费：`compiler/`（调用 CLI 或 API）、`runtime/`（嵌入或调用 VM）
- 被消费：`scripts/`（xtask 打包与测试调用 z42b / workload）
