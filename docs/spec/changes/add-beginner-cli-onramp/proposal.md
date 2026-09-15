# Proposal: 新手上手路径的 CLI 补齐（安装 / new / build·run / 单文件 / 版本）

> 状态：🟡 进行中（2026-09-16 User 批准开工）| 创建：2026-09-15
> 子系统：`toolchain`（launcher / z42b / 安装脚本 / 打包）+ `stdlib`（z42.project 清单定位）+ `compiler`（z42c 空参报错）
> 姊妹变更：[`add-learn-book-examples-gate`](../add-learn-book-examples-gate/proposal.md) —— 手册第 1–3 章与第 28 章依赖本变更

## Why

照着「安装 → Hello World → 工程」写教程时，用户第一步就会撞墙（2026-09-15 源码核实）：

| 用户操作 | 现状 |
|---------|------|
| 一行命令安装 | 不存在。`install-z42.sh` 必须在仓库 checkout 里跑（读 `versions.toml`），`curl \| sh` 时 `BASH_SOURCE` 为空；仓库名写成 `codesigner-ui/z42`（launcher 用 `z42-lang/z42`）；无 python3 时解析失败；`.bat` 读旧版清单格式、下载 URL 为空、按旧布局安装 |
| `z42 --version` | `unknown command`，退出码 2 |
| `z42 new hello` 后照提示 `z42 build && z42 run` | `z42 build` 打印 usage 且**退出码 0**；`z42 run` 退出码 2 |
| `z42 new --test` 生成的工程 | `kind="exe"` 却无 `Main` → 编译失败；依赖键 `z42.test` 未加引号被 TOML 解析成嵌套表 |
| `z42 new --path dir` | 帮助说是父目录，实现当作工程目录本身 |
| `z42 run hello.z42` | VM 报 `unrecognised artifact extension` |
| `z42 run .` 的输出 | 混入 z42c 的 `cached: N/M files`（stderr） |
| `z42 clean` | 删 `./dist`、`./cache`；而 z42c 实际写 `artifacts/<profile>/.cache`，清不干净 |
| 配置了 `[build] dist_dir` 的工程 `z42 run` | 写死 `<proj>/dist`，找不到产物 |
| `z42 self-update` | SDK 形态下 apphost 必设 `Z42_PORTABLE_VM` → 永远拒绝；且按旧布局替换 |

User 裁决：
- 2026-09-15：这些缺失功能**趁机补上**。
- 2026-09-16：**按正规的来写，趁机梳理命令行**——z42 需要的命令补上，暂时不需要的（如 `self-update`、`which`）去掉避免干扰；
  安装脚本要能让用户直接下载到，**默认 nightly（即最新版）**，**默认装到用户级系统目录 `~/.z42`**；下载安装后续单独一章详细介绍。

## What Changes

0. **命令面梳理**（design D11）：移除 `info` / `list` / `default` / `link` / `which` / `install` / `uninstall` / `self-update`、
   `run --runtime` 与 runtimeconfig `version` 键、隐藏的 `run --rid`、z42b 的 `run` 桩、`z42 new --test`；保留并整理 `new` / `build` / `run` / `test` / `bench` / `clean` / `repl` / `publish` / `export` / `workload`，新增 `version` / `help`。
1. **清单自动定位**：`z42 build / run / test / bench / clean / publish` 不带路径时，从当前目录向上查找工程；z42b 直接调用同一逻辑。
2. **`z42 new` 修正**：`--path` 语义、名字校验、下一步提示、生成的 README、删过期 PARKED 注释（`--test` 模板移除）。
3. **`z42 --version` / `-V` / `z42 help [命令]`**。
4. **输出与错误信息**：`z42 run` 不混入构建噪声；z42c `build` 缺清单时报错退出 2；`z42 run` 尊重 `[build]` 产物目录；`z42 clean` 与实际产物布局对齐。
5. **单文件运行**：`z42 run hello.z42 [-- args]`（撤销 `launcher-future-single-file-exe-zpkg` 延后项）。
6. **独立安装脚本**：`install.sh`（POSIX sh）+ `install.ps1`，可 `curl | sh` / `irm | iex`；仓库内的 `scripts/install-z42.*` 改为调用它的薄封装（安装逻辑只有一份）。
7. ~~`z42 self-update` 修复~~ → 移除（User 2026-09-16）；更新 = 重新运行安装脚本。
8. **测试**：上述每条在 `xtask test dist`（真实 SDK 形态）中有端到端用例；安装脚本在 CI 用刚打出的包离线验证。

## Scope（允许改动的文件）

| 文件 / 目录 | 变更 |
|------|------|
| `src/libraries/z42.project/src/ManifestLocator.z42` + tests | NEW |
| `src/toolchain/launcher/core/*.z42` | MODIFY（cli 路由 / run / publish / version / network self-update / 单文件） |
| `src/toolchain/builder/core/{builder_new,builder_cli,builder_commands,builder_test}.z42` | MODIFY |
| `src/compiler/z42c.driver/src/{Main,BuildCommand,BuildLog,BuildPaths,BuildCache,IndexedDist,RuntimeConfigSidecar}.z42` | MODIFY / NEW（`build` 参数解析与定位、`--quiet`、路径解析改用 BuildLayout） |
| `src/libraries/z42.project/src/BuildLayout.z42` + tests | NEW |
| `src/toolchain/workload/{ios,android,wasm}/appbuilder/export.z42` | MODIFY（安装提示改 `z42 workload install`） |
| `docs/book/src/toolchain/cli.md`、`docs/design/toolchain/*.md`、`docs/roadmap.md` | NEW / MODIFY |
| `scripts/install/install.sh`、`scripts/install/install.ps1` | NEW |
| `scripts/install-z42.{sh,bat,command}` | REWRITE（薄封装） |
| `scripts/test/xtask_test_dist.z42`（+ 拆分出的新文件） | MODIFY / NEW |
| `.github/workflows/{ci,release,deploy-book}.yml` | MODIFY（安装脚本随 release 上传 + Pages 托管 + CI 离线验证） |
| `docs/design/runtime/launcher.md`（或其迁入的 book 页）、`docs/book/src/compiler/tools.md`、`docs/book/src/compiler/project-build.md`、`docs/workflow/{quickstart,release}.md`、`README.md` | MODIFY |

## Out of Scope

- 统一 `z42 build` 的执行者（z42c vs z42b，产物目录两套约定）：本变更只让 launcher 解析好清单后照旧转发 z42c；统一是独立的构建系统变更（见 design「延后」）。
- `z42 init`（在当前目录初始化）。
- 自动修改 shell profile 以外的系统级 PATH（Windows 注册表除外，见 D7）。
- 多版本运行时管理：随命令面梳理整体移除（D11），不再作为后续工作。
