# toolchain/launcher — `z42` 命令

## 职责

SDK 的唯一命令入口 `z42`：新建 / 构建 / 运行 / 测试工程、发布应用、管理平台 workload。
`build` 转发 z42c，`new` / `test` / `bench` / `clean` 转发 z42b，`repl` 转发 z42i；其余在这里实现。
不管理多个运行时版本（SDK 单版本，运行应用用同址 `bin/z42vm`），不做自更新（由安装脚本负责）。

`z42` 本身是通用 apphost stub（`src/toolchain/workload/desktop/platform/apphost`），打包时 patch 成
payload=`programs/launcher/launcher.zpkg`；本目录只有 launcher 核心的 z42 源。

## 功能索引

| 功能 | 入口 / 文件 |
|------|-----------|
| 命令树、路由、转发 z42b / z42c / z42i、`help` | `core/launcher_cli.z42` 的 `_runLauncher` / `_launcherRoot` |
| SDK 布局解析（`_home` / `_sdkVm` / `_sdkLibs` / `_sdkBin`） | `core/launcher.z42` |
| `run`（定位工程 → `z42c build --quiet` → 按 `BuildLayout` 找产物 → z42vm） | `core/launcher.z42` 的 `_cmdRun` |
| `version` | `core/launcher.z42` 的 `_cmdVersion` |
| `publish` / `export`（含工程定位 `_resolveDeployManifest`） | `core/launcher_export.z42` |
| `workload install / list / uninstall` | `core/launcher_workload.z42` |
| release-index 下载 / 校验 / 解压（workload 网络安装用） | `core/launcher_network.z42` |

## 基础用法

```bash
z42 new hello && cd hello
z42 run                 # 构建并运行当前工程
z42 build --release
z42 test
z42 --version
z42 help run
```

完整命令参考见 book [z42 命令参考](../../../docs/book/src/toolchain/cli.md)。

## 如何测试验证

```bash
xtask package sdk --no-build            # 打出 SDK 包（含 launcher）
DIST_SMOKE_ONLY=launcher xtask test dist  # launcher 命令行冒烟（新手路径 + run/repl）
xtask test dist                          # 完整：再加 publish 冒烟与打包 goldens
```

冒烟用例在 `scripts/test/xtask_test_dist_cli.z42`（new → run → build → clean / 版本 / 帮助 / 错误路径）与 `scripts/test/xtask_test_dist.z42`。

## 关联文档

- 命令参考（用户面，唯一权威）：[docs/book/src/toolchain/cli.md](../../../docs/book/src/toolchain/cli.md)
- apphost 机制与运行时探测：[docs/design/runtime/launcher.md](../../../docs/design/runtime/launcher.md)
- 工程定位 / 产物布局实现：`src/libraries/z42.project/src/{ManifestLocator,BuildLayout}.z42`
- 引入/演进：change `add-beginner-cli-onramp`（simplify-z42-cli）

## 核心文件

| 文件 | 职责 |
|------|------|
| `core/launcher.z42` | Main、SDK 布局、`run`、`version` |
| `core/launcher_cli.z42` | 命令树、路由、转发 |
| `core/launcher_export.z42` | `publish` / `export` |
| `core/launcher_workload.z42` | `workload` 子命令 |
| `core/launcher_network.z42` | 下载辅助 |
| `core/z42.launcher.z42.toml` | launcher 工程清单（产出 `launcher.zpkg` + 根 `z42` apphost） |
