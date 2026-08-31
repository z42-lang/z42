# toolchain/launcher — `z42` launcher

## 职责

用户一次性安装的唯一入口 `z42`：解析所需运行时版本 → 用对应 `z42vm` 跑
Exe-zpkg → 透传命令行参数；并管理已装运行时（`~/.z42/runtimes/<ver>/`）。
类比 `dotnet` muxer + `rustup`。

**z42 优先**：只有"找/给 VM 的最小核"必须原生（bootstrap 铁律：无 VM 跑不了
z42）；其余逻辑全部用 z42 写。

## 结构

| 路径 | 语言 | 职责 |
|------|------|------|
| `core/launcher.z42` | **z42** | launcher 命令实现：`~/.z42` 缓存 / 版本解析 / 起 app / 各子命令 handler（从 `Std.Cli.ParseResult` 读参）。编译为 `launcher.zpkg`（Exe-mode）。 |
| `core/launcher_cli.z42` | **z42** | CLI 层（migrate-xtask-launcher-to-std-cli）：`Std.Cli` 嵌套 `SubcommandRouter` 命令树 + `_runLauncher`（apphost 简写 / `run` 透传 / Resolve + dispatch）。每层 `-h` help 由库生成（手写 `_help()` 已删）。 |
| `core/apphost.z42` | **z42** | apphost stub-patch 库（apphost-as-config 2026-06-17）：拷贝 apphost stub 模板 + patch 内嵌占位符 + macOS ad-hoc 重签名。`Produce(app, outPath)` 由 `_cmdPublishDesktop`（`z42 publish`）调用——**无独立 `z42 apphost` 命令**。 |

> **unify-launcher-apphost（2026-06-21）**：原 Rust trampoline crate（`Cargo.toml` /
> `src/main.rs` / `src/lib.rs`）**已删**。`z42` 现在就是**通用 per-app apphost stub**
> （`src/toolchain/workload/desktop/platform/apphost`，z42-apphost crate），SDK 打包时
> patch 成 payload=`programs/launcher/launcher.zpkg`。运行时解析全部复用 apphost 桩的 `hostrun`
> 模块（`resolve_app_runtime` + `ensure_portable_vm`，most-local-wins；原独立 `z42-hostrun` crate
> 已并入桩，merge-hostrun-into-apphost）。本目录现只剩
> `core/`（launcher 核心 z42 源 → `launcher.zpkg`）。

## 命令（P1）

`run` / `link` / `list` / `default` / `which` / `info`（本地、无网络）。
`install` / `uninstall` / `self update` / 下载 = P2（后续 spec）。

`z42 publish <project.z42.toml>`（读 `[platform.desktop].publish_dir`）→ 产出 per-app 原生可执行文件 apphost（机制 / 本地优先解析 / 签名详见 [`docs/design/runtime/launcher.md`](../../../docs/design/runtime/launcher.md) 的 apphost 段）。apphost-as-config：apphost 是 desktop 平台的发布产物，不是独立命令。

## 状态

进行中 —— spec：`docs/spec/changes/add-z42-launcher/`。
Phase 0（z42vm `-- args` 透传）已落地（commit fe0e0273）。
