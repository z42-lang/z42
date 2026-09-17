# Tasks: 编辑器集成统一到 `z42d install`

> 状态：🟢 已完成 | 完成：2026-09-17

## 1. 资产随 SDK 分发
- [x] 1.1 `packages.toml` 新增 `[component.editor-assets]`（`kind="editor-assets"`, `dest="editors/"`）
- [x] 1.2 `[package.sdk].include` 加 `"editor-assets"`
- [x] 1.3 `_pkgStageEditorAssets`（仿 `_copyAbiHeaders`）——**`*.tpl.json` 不进包**
- [x] 1.4 `_packageDesktop` 调用它

## 2. `z42d install <target>`
- [x] 2.1 `editor_install.z42`：目标表 / SDK 根定位（与 launcher 同一套优先级）/ 安装
- [x] 2.2 `devtools_cli.z42` 注册 `install` + dispatch
- [x] 2.3 toml `[sources]` 加新文件
- [x] 2.4 复用 `Directory.Copy(src, dst, recursive)`（stdlib 已有，不自己写拷贝树）

## 3. 门禁
- [x] 3.1 `xtask test packages` 的 staging 自检加 4 条断言，含 **`generator template NOT packaged`**
- [x] 3.2 `packages-config` 自检的组件数 9→10、sdk.include 8→9 + 新组件的 name/kind/dest
- [x] 3.3 `xtask test vscode-syntax` 仍绿（未碰生成链）

## 4. 文档
- [x] 4.1 `devtools/README.md`：命令表加 `install`；「编辑器集成」节改成**两条安装路对照表**
- [x] 4.2 `reference/toolchain/cli-z42c-z42b.md`：标题 → `z42c` / `z42b` / `z42d`；新增
      `z42d install` 与 `z42d symbolicate` 两节（含退出码、只有语法高亮无 LSP 的边界）
- [x] 4.3 `internals/toolchain/editor-integration.md`：两个命令面 → **三个**；新增
      「资产怎么随 SDK 走」；原 §4 改为「开发者那条为什么装在项目目录」
- [x] 4.4 修 `xtask_install_vscode.z42` 文件头**与代码相反**的注释（写 `~/.vscode/`，实为仓库本地）

## 5. 验证
- [x] 5.1 z42d 四条路径实跑：未知目标(2) / 资产缺失(1) / 正常安装(0) / 重装幂等
      —— 用临时 HOME + 假 SDK，不碰真实 `~/.vscode`
- [x] 5.2 `xtask test packages` 三项 PASS
- [x] 5.3 `xtask test vscode-syntax` / `test docs` / `test examples` 绿

## 备注

**`xtask deps install vscode` 最终没有改成转发 z42d**——原计划如此，实施时看懂了它的机制后
推翻：它是 **symlink 回源码树** + 先经 `z42c --dump-keywords` 重新生成 grammar，
服务的是「改完 Lexer 立刻看到效果」。SDK 里没有生成器，转发会**丢掉**这条能力。
两条路落点不同（工作区 vs 用户级）、互不覆盖，是正确的分工，文档里写清即可。
