# toolchain/devtools — z42 开发者工具链（`z42d`）

## 职责

围绕**源码与开发体验**的工具集合，统一在单个 muxer apphost `z42d` 下：

| 子命令 | 职责 | 状态 / roadmap |
| `symbolicate` | 离线还原剥离档崩溃栈（`at <fn> +0x<off>` + `.zsym` → `file:line:col`）| ✅ 已实现（add-offline-symbolication，2026-08-04；z42d 首个落地子命令）|
|--------|------|----------------|
| `fmt`  | z42 源码格式化 | planned（0.2.4 → 0.4.x；收编原独立 `z42-fmt`）|
| `doc`  | doc comment → HTML/markdown 文档站点 | planned（0.4.6；收编原独立 `z42-doc`）|
| `dbg`  | 调试器（断点 / 单步 / 变量）| planned（前端在本目录 + VM 断点/单步钩子住 `runtime/`，DAP 0.8.x）|
| `prof` | 运行期 / 编译期性能剖析 | planned（0.4.4 Pv4/Pc4 profiling 门控）|
| `lint` | 静态检查 | planned（0.7.x；收编原独立 `z42-lint`）|

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

## 编辑器集成（`vscode/`，非 z42d 子命令）

[`vscode/`](vscode/) 是 **VSCode 编辑器资产包**（add-vscode-syntax-ext，2026-07-07）：
声明式 TextMate 语法高亮 + language-configuration，安装走 `xtask deps install vscode`
（生成 grammar + symlink），防漂移检查 = `xtask test vscode-syntax`（GREEN gate）。
它不进 `z42d` muxer（不是 CLI 工具，是编辑器资产）；**B 期 LSP**（诊断/跳转/语义着色）
落地时：server = 本目录新增 `lsp` 子命令（调 z42c API，对照 `dbg`/DAP 的前端+协议模式），
client = `vscode/` 扩展升级为 LSP 宿主。详见 vscode/README + book 编辑器集成页。

## 核心文件（`core/`，scaffold）

| 文件 | 职责 |
|------|------|
| `core/devtools_cli.z42` | **CLI 路由**（对照 `builder_cli.z42`）：`Std.Cli` 嵌套 router 登记 symbolicate + fmt/doc/dbg/prof/lint（每层 `-h`）+ dispatch（symbolicate 已实现，其余 "planned"）|
| `core/symbolicate.z42` | **离线符号化引擎**（add-offline-symbolication）：读崩溃栈 + `.zsym`（多目录递归）→ `SidecarReader` 建 frame-name→行表 索引 → `+0x<off>` 解包(block<<16\|instr) → `file:line:col` |
| `core/z42.devtools.z42.toml` | 包清单（exe / pack / apphost；依赖 z42.core/io/cli/**ir**）|

## 基础用法（symbolicate）

release（剥符号）构建把行表剥到旁挂 `.zsym`；部署常不带 `.zsym`，故线上崩溃栈是
`at <fn> +0x<off>`（无行号）。归档好 `.zsym` 后离线还原：

```bash
z42d symbolicate crash.txt --syms path/to/app.zsym          # 单个 .zsym
z42d symbolicate crash.txt --syms symdir/ --syms other.zsym # 多个（目录递归找 *.zsym，参考 addr2line/Breakpad）
```

`--syms` 可重复，值为 `.zsym` 文件**或目录**（目录递归扫 `*.zsym`）。匹配 `at <fn> +0x<off>`
的帧按 frame-name（含签名，如 `Demo.Boom(int)`）查符号 → 还原 `(file:line:col)`；非帧行透传，
缺符号保留原行 + stderr 警告（尽力而为，退出码 0）。机制见
[`docs/design/runtime/zpkg.md`](../../../docs/design/runtime/zpkg.md)（`.zsym` MDBG within-minor 例外）。

## 与现有规划的关系（待收敛 —— 规范冲突，已记录）

> 当前 `docs/roadmap.md` 对这些工具有**三套并存且互相矛盾**的说法，需后续裁决统一：
>
> 1. [roadmap.md:258](../../../docs/roadmap.md) —— `z42c` 编译器驱动**自身**计划托管 `fmt/doc/...`
> 2. [roadmap.md:260-263](../../../docs/roadmap.md) —— 又规划**独立 binary** `z42-fmt`/`z42-lint`/`z42-doc`
> 3. 本目录 —— 统一收进 muxer `z42d`
>
> `fmt`/`doc` 同时出现在「z42c 动词集」与「独立 binary」两处。User 决策（2026-06-30）：
> **先立 z42d 骨架，暂不动 z42c 规划**；三处收敛留待各工具真正实现期裁决（倾向：z42d 统一承接，
> z42c 退为纯编译动词，launcher 转发）。届时同步改 roadmap 唯一真相。

## 依赖关系

- 依赖 `z42.core` / `z42.io` / `z42.cli`（命令面）；各工具实现期再按需加（编译器 API / `z42.io` 文件等）。
- 调用 `runtime/`（dbg/prof 的 VM 钩子）、`compiler/`（doc/lint/fmt 的语法·语义）。
- 被 launcher 命令分发调用。

## 状态

🟡 **骨架占位，已打包**。命令面 + apphost bin/payload 均已就位，`z42.devtools.z42.toml`
已登记进 [`scripts/packages.toml`](../../../scripts/packages.toml)（`[component.devtools]`，
2026-07-01 User 裁决），随 SDK 包一起发行——但每个子命令目前仍只打印 "planned" 并
`return 1`，尚无一个真正实现。

落地走 spec-first（架构性 + 多工具分期），各工具按 `docs/roadmap.md` 时点推进。
