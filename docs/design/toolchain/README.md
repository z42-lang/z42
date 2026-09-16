# toolchain 设计

`z42` launcher 之上的开发者工具链设计：命令分发架构 + 跨平台工程脚手架/生命周期。

> ⚠️ **本目录文档为前瞻设计草案（未实施）**：为 0.4.x+ 的 `z42c new/init`、workload 机制、移动端打包线铺路（见 [roadmap.md](../../roadmap.md) `z42up` / workload / `z42c new` 行）。落地时各自开 `docs/spec/changes/<name>/` spec，按 workflow 实施；本目录沉淀"为什么这样设计"的底稿。

## 核心文件

| 文件 | 职责 |
|------|------|
| [launcher-command-dispatch.md](launcher-command-dispatch.md) | `z42` 如何分发命令：core / SDK / workload 三层 + 目录发现 + Std.Cli 统一注册 |
| [build-orchestrator.md](build-orchestrator.md) | `z42b` 构建编排器：驱动 `z42.build` 管线（in-process `ICompiler` 共享编译 + 标准/自定义两条 driver 路径 + `build/` 扩展约定）|
| [platform-export-lifecycle.md](platform-export-lifecycle.md) | z42 项目 → 导出平台工程（managed + eject）+ 全平台生命周期（build / export / publish / test） |
| [runtime-workload-distribution.md](runtime-workload-distribution.md) | GitHub Releases 为后端：host runtime / workload 的安装·更新·运行 + `release-index.json` manifest 契约 + 版本解析 |

## 与已有规范的关系

- [runtime/launcher.md](../runtime/launcher.md) — launcher 现状（run/link/list/install/apphost；Std.Cli router）。本目录是它的**扩展方向**。
- [runtime/embedding.md](../../internals/src/runtime/embedding.md) — VM 嵌入 API + 平台包布局（xcframework/AAR/wasm）。平台导出的"VM 嵌入"层即此。
- `Std.Cli`（archive/2026-06-10-add-cli-nested-subcommands + add-cli-optional-positional）— 命令树/解析的库基础。
