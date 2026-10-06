# toolchain/devtools — z42 开发者工具链（`z42d`）

## 职责

围绕**源码与开发体验**的工具集合，统一在单个 muxer apphost `z42d` 下：

| 子命令 | 职责 |
|--------|------|
| `symbolicate` | 离线还原剥离档崩溃栈（`at <fn> +0x<off>` + `.zsym` → `file:line:col`）|
| `install` | 把 SDK 自带的编辑器集成装进用户编辑器（`z42d install vscode`）|

待办：`fmt`（源码格式化）、`doc`（doc comment → 文档站点）、`dbg`（调试器 / DAP）、`prof`（性能剖析）、
`lint`（静态检查）五个子命令已登记命令面，运行时只打印 "planned" 并返回 1；时点见 `docs/roadmap.md`。

形态对照 z42b（builder）：**一个 exe + 一个 Std.Cli 嵌套 router**，launcher 命令分发
（`z42 fmt` → `z42d fmt`，同 `z42 test` → `z42b`）。

```
src/toolchain/devtools/core/*.z42  →  z42.devtools.zpkg  →  apphost z42d
（对照 builder/core/*.z42 → z42.builder.zpkg → z42b）
```

**不做**：
- **编译本身** —— 经编译器库（z42c）。`doc`/`lint` 需要语义信息时调编译器 API，不 fork 子进程。
- **VM 级钩子** —— `dbg` 的断点/单步、`prof` 的采样都对接 `runtime/` 的 VM 调试/profiling 钩子
  （读 zbc DBUG 源位置）；z42d 侧只做前端 + 协议适配（DAP），不在此实现 VM 钩子本身。

## 编辑器集成（`vscode/` + `z42d install`）

[`vscode/`](vscode/) 是 **VSCode 编辑器资产包**：声明式 TextMate 语法高亮 +
language-configuration，**无 `main`、无需编译**。

**两条安装路，服务不同的人，落点故意不同、互不覆盖：**

| 谁 | 命令 | 落点 | 特点 |
|---|------|------|------|
| **SDK 用户** | `z42d install vscode` | `~/.vscode/extensions/z42.z42-lang/`（用户级） | 从 `<sdk>/editors/vscode/` 拷贝；装了 SDK 就能用，不需要仓库 |
| **仓库开发者** | `xtask deps install vscode` | `<repo>/.vscode/extensions/`（工作区） | **symlink 回源码树**，且先经 `z42c --dump-keywords` 重新生成 grammar——改 Lexer 关键字即时生效 |

资产随 SDK 分发靠 `packages.toml` 的 `[component.editor-assets]`
（`*.tpl.json` 生成器模板**不进包**，由 `xtask package check` 的 staging 自检守着）。
grammar 防漂移 = `xtask check vscode-syntax`（GREEN gate）。

## 基础用法
release（剥符号）构建把行表剥到旁挂 `.zsym`；部署常不带 `.zsym`，故线上崩溃栈是
`at <fn> +0x<off>`（无行号）。归档好 `.zsym` 后离线还原：

```bash
z42d symbolicate crash.txt --syms path/to/app.zsym          # 单个 .zsym
z42d symbolicate crash.txt --syms symdir/ --syms other.zsym # 多个（目录递归找 *.zsym，参考 addr2line/Breakpad）
```

`--syms` 可重复，值为 `.zsym` 文件**或目录**（目录递归扫 `*.zsym`）。匹配 `at <fn> +0x<off>`
的帧按 frame-name（含签名，如 `Demo.Boom(int)`）查符号 → 还原 `(file:line:col)`；非帧行透传，
缺符号保留原行 + stderr 警告（尽力而为，退出码 0）。机制见
[`docs/internals/src/formats/zpkg.md`](../../../docs/internals/src/formats/zpkg.md)（`.zsym` MDBG within-minor 例外）。

## 如何测试验证

```bash
xtask build toolchain       # 构建并 publish z42d
xtask check vscode-syntax    # 编辑器 grammar 一致性
```

## 关联文档

- 编辑器集成：[editor-integration.md](../../../docs/internals/src/toolchain/editor-integration.md)；`.zsym` 格式：[zpkg.md](../../../docs/internals/src/formats/zpkg.md)

## 核心文件（`core/`）

| 文件 | 职责 |
|------|------|
| `core/devtools_cli.z42` | **CLI 路由**（对照 `builder_cli.z42`）：`Std.Cli` 嵌套 router 登记全部子命令（每层 `-h`）+ dispatch |
| `core/symbolicate.z42` | **离线符号化引擎**：读崩溃栈 + `.zsym`（多目录递归）→ `SidecarReader` 建 frame-name→行表 索引 → `+0x<off>` 解包(block<<16\|instr) → `file:line:col` |
| `core/editor_install.z42` | `z42d install <target>`：把 `<sdk>/editors/vscode/` 拷到 `~/.vscode/extensions/z42.z42-lang/` |
| `core/z42.devtools.z42.toml` | 包清单（exe / pack / apphost；依赖 z42.core/io/cli/**ir**）|
| `vscode/` | VSCode 编辑器资产包（见其 README）|

## 依赖关系

- 依赖 `z42.core` / `z42.io` / `z42.cli`（命令面）与 `z42.package`（`.zsym` 读取）。
- 被 launcher 命令分发调用。
