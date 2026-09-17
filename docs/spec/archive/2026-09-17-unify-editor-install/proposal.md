# Proposal: 编辑器集成统一到 `z42d install`

> 状态：🟡 进行中 | 创建：2026-09-17

## Why

写学习手册第 4 章「开发环境」时核实发现：**装了 SDK 的普通用户拿不到 VSCode 扩展。**

| 分发途径 | 现状 |
|---|---|
| 随 SDK 分发 | ❌ `.z42/` 下无任何扩展文件 |
| VSCode Marketplace | ❌ 只有 `package.json` 里一个 `"publisher": "z42"` 字段，无 vsce、无 CI 发布 |
| `.vsix` 离线包 | ❌ 全仓无打包步骤 |
| `xtask deps install vscode` | ⚠️ **仓库开发者专用**——装到 `<repo>/.vscode/extensions/`，且 SDK 用户没有 xtask |

⇒ 手册第 4 章没法老实地教读者装扩展，只能让他们去 GitHub 扒目录。

顺带两处**已存在的缺陷**一并修掉：

1. `scripts/install/xtask_install_vscode.z42` 的文件头注释写 symlink 到 `~/.vscode/extensions/`，
   而第 46 行实际是 `Path.Join(root, ".vscode/extensions")`（仓库本地）——**注释与代码相反**。
2. `z42d --help` 的命令面里没有任何「装开发环境」的入口，而 `z42d` 正是 developer toolchain 的 muxer。

## What Changes

1. **扩展资产进 SDK 包**：新增 `[component.editor-assets]`，把
   `src/toolchain/devtools/vscode/` 的 4 个文件装进 SDK 的 `editors/vscode/`。
2. **`z42d install <target>`**：新子命令，从 SDK 自身的 `editors/` 取资产，
   装到用户级编辑器目录（VSCode：`~/.vscode/extensions/z42.z42-lang/`）。
   预留多目标形态（以后可加 `zed` / `nvim`）。
3. **`xtask deps install vscode` 改为转发 z42d**——仓库开发者那条「用生成器重新生成
   tmLanguage 再装」的路径保留（它依赖 `z42c --dump-keywords`，SDK 里没有生成器）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `scripts/packages.toml` | MODIFY | 新增 `[component.editor-assets]` + sdk 的 include 加一项 |
| `scripts/package/xtask_stage_components.z42` | MODIFY | 新增 `_pkgStageEditorAssets`（仿 `_copyAbiHeaders`） |
| `src/toolchain/devtools/core/devtools_cli.z42` | MODIFY | 注册 `install` 子命令 + dispatch |
| `src/toolchain/devtools/core/editor_install.z42` | NEW | 安装逻辑（定位 SDK 的 `editors/`、拷贝、幂等） |
| `src/toolchain/devtools/core/z42.devtools.z42.toml` | MODIFY | `[sources]` 加新文件 |
| `scripts/install/xtask_install_vscode.z42` | MODIFY | 改为转发 z42d；修文件头过期注释 |
| `src/toolchain/devtools/README.md` | MODIFY | 六段：功能索引 + 核心文件 |
| `docs/reference/src/toolchain/cli-z42c-z42b.md` | MODIFY | z42d 命令面补 `install` |
| `docs/internals/src/toolchain/editor-integration.md` | MODIFY | 安装路径与分发链路改写 |
| `docs/spec/changes/unify-editor-install/**` | NEW | 本 change |

**只读引用**：`scripts/package/xtask_packages_config.z42`（组件表解析）、
`src/toolchain/devtools/core/symbolicate.z42`（同目录既有子命令写法参照）。

## Out of Scope

- **打 `.vsix` / 发 Marketplace**——那是另一条路（需要 vsce 依赖 + 发布凭据 + CI 密钥），
  本 change 只解决「SDK 用户能装上」。留 roadmap。
- 其它编辑器（zed / nvim）的实际资产——本 change 只把 `install <target>` 的形状留出来。
- LSP——完全没有，不在本 change。

## Open Questions

- [x] 装到用户级还是仓库本地？→ **用户级**（`~/.vscode/extensions/`）。
      SDK 用户没有「仓库」概念；仓库开发者继续用 `xtask deps install vscode`（走生成器）。
