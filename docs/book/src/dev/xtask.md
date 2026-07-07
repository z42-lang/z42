# xtask：自举 dev CLI

> **页型**: 机制页 ｜ **状态**: ✅ 已实现 ｜ **代码**: `scripts/`
> **相关**: [构建编排](build.md) · 编译器·自举与种子（待写）｜ **对齐**: 2026-07-07

## 概述

xtask 是纯 z42 写的仓库开发 CLI（`xtask <cmd>`），统一承载构建、测试、打包、
发行等全部开发动作——它自己就是被 z42c 编译、跑在 z42vm 上的 z42 程序（dogfooding）。
用法与命令清单见 `scripts/README.md`（基础层），本页讲设计与机制。

## 设计目标与约束

- **单一入口**：所有开发动作收敛到一个命令树，杜绝散落 shell 脚本各自为政
- **dogfooding**：用 z42 写开发工具，工具链本身就是语言最大的真实用例
- **自足自举**：冷启动仅需下载上一版 nightly 种子，无其他外部工具链依赖（见"机制"）
- **可重复**：同一命令在本地与 CI 行为一致；产物全部落 `artifacts/`

## 方案与决策

| 决策 | 选择 | 理由 |
|------|------|------|
| 实现语言 | z42（而非 bash / Rust cargo-xtask） | dogfooding 压力测试语言与 stdlib（Cli/IO/Toml/Regex 都被迫成熟）；跨平台无 shell 差异 |
| 命令路由 | `Std.Cli` SubcommandRouter 树 | 每层自动生成 `-h` 帮助；命令面即数据结构，可静态审阅 |
| 工具链选择 | 全局 `--toolchain <dir>` → `Z42_HOME` 环境变量 | 一处剥离、处处生效，命令实现无需逐个透传参数 |
| 运行形态 | 原生 apphost 可执行（仓库根 `./xtask`，内嵌 launcher + zpkg） | 单文件、直接运行，不依赖 PATH 上的 launcher；与用户应用同一套 apphost 机制 |

## 机制

### 自举链路（冷启动 → 日常）

```mermaid
graph TD
    A[install-z42 脚本<br/>无 z42 依赖] -->|下载上一版 nightly| B[种子: z42c.driver.zpkg<br/>+ stdlib dist + z42vm]
    B -->|种子 z42c 编 scripts/xtask.z42.toml| C[artifacts/xtask/xtask.zpkg]
    C -->|z42 publish → ./xtask| D[xtask 运行<br/>原生 apphost]
    D -->|build compiler / stdlib| E[自建 z42c + stdlib<br/>替换种子产物]
    E -.->|下一次构建用自建产物<br/>warm 路径| D
```

冷启动只发生一次：`install-z42.{sh,bat,command}`（唯一的非 z42 环节）下载 nightly 种子；
此后进入 warm 循环——xtask 编排 z42c 自建自己和 stdlib，产物就地替换，越滚越新。
为什么种子必须存在、新语法为何要晚一个 nightly 才能用——见编译器部分「自举与种子」章（待写）。

### CLI 分发

```mermaid
graph LR
    M[Main] --> R[_runCli]
    R --> T[--toolchain 剥离<br/>→ Z42_HOME]
    T --> I[拦截层<br/>test/bench/run 裸命令默认行为]
    I --> RT[SubcommandRouter 树<br/>Resolve]
    RT --> D[_dispatch 按 Path 分层分发<br/>→ 各 _dispatchXxx]
```

三段式：① 全局标志剥离（`--toolchain` 两 token 形式从 argv 摘除，last-wins，写入
`Z42_HOME`——单一「SDK 根」变量；simplify-compiler-build 把原 `Z42_TOOLCHAIN` 折进 `Z42_HOME`）；
② 拦截层给裸命令定默认语义（裸 `test` = 完整 GREEN gate、裸 `bench` = e2e、
`run` 原样透传 launcher）；③ 路由树解析后按路径首段分发到各子模块的 handler。

### `--toolchain` / `Z42_HOME` 的作用范围

设了 toolchain 后，**所有**解析"用哪套 z42c / stdlib / z42vm"的路径函数
（`_toolchainDir` → `_toolchainDriverHome` / `_toolchainLibs`）都优先取该 `Z42_HOME` 目录，
否则回落 build-tree（`artifacts/build/`）。也就是说它不是某几个命令的参数，而是全局的
"工具链根切换"——例如 `regen --toolchain <dir>` 会用该工具链的 z42c 编 golden。
`Z42_HOME` 跨两种布局：SDK-toolchain（`programs/`+`libs/`+`bin/z42vm`，`--toolchain` 或便携 SDK）
与 managed 安装（`runtimes/`+`config.toml`）；消费端按 `programs/` / `bin/z42vm` 是否存在守卫，
managed 布局的 `Z42_HOME` 不符 SDK-toolchain 布局时自动回落 build-tree，不会错位。

## 实现

代码全在 `scripts/`，按职责分子目录；逐文件职责表见 `scripts/README.md`（基础层，不重复）。
机制级要点：

| 组件 | 位置 | 要点 |
|------|------|------|
| 入口 + 顶层 handler | `scripts/xtask.z42` | `Main` → `_ensureDriverVm` 校验 VM → `_runCli` |
| 路由树 + 分发 | `scripts/xtask_cli.z42` | `_cliRoot` 构造树；`_applyToolchainOpt` 剥离全局标志 |
| 共享基建 | `scripts/common/` | 路径解析、进程执行、versions.toml、golden 枚举 |
| 构建编排 | `scripts/build/` | 见[构建编排](build.md) |
| 测试编排 | `scripts/test/` | 见[测试门禁](test-gate.md) |
| 打包引擎 | `scripts/package/` + `scripts/packages.toml` | 见[打包引擎](packaging.md) |
| 依赖安装 | `scripts/install/` + `scripts/xtask_deps.z42` | 依赖两层模型 + `deps check/install/env` 三正交子命令（见下节） |

### 依赖两层模型（`deps`，simplify-xtask-deps 2026-07-07）

工具链依赖按"没有它平台的构建/测试能不能跑"分两层：

- **平台必备**：`deps install --os <p>` 直接装——android = rust targets + cargo-ndk +
  JDK + build-tier SDK；ios = rust targets + Xcode 检查；wasm = rust targets +
  wasm-pack + **hermetic node**（wasm 测试面必备，从旧"手动 step"升格）。
- **用到才装**：重型/兜底依赖零命令面，消费步骤检测缺失后自动装——android
  emulator tier（emulator + system-image + AVD + Gradle，~4GB）由
  `test platform android run`（`AndroidBackend.RunTests`）安装；node 兜底由 wasm
  测试步骤安装。安装失败 = 该步骤失败，不吞不跳过。

命令面三正交子命令：`deps check [--os]`（唯一只读校验 = presence + versions.toml↔
投影 drift 单实现）、`deps install [--os] [--force]`（纯安装）、`deps env`（可 eval
的 `ANDROID_NDK_HOME` 导出，stdout 纯净）。**check 退出码策略**：drift 与机器无关、
恒致败；presence 仅显式 `--os <p>` 时致败——CI 的 build-and-test 在无平台 SDK 的
runner 上裸跑 `deps check` 当 drift 门禁，presence 缺失在那里是预期的（信息性展示）。

## 边界与限制

- **冷启动依赖网络**：fresh checkout 无种子时必须能下载 nightly（CI 全新 runner 同理）
- **格式漂移窗口**：zbc/zpkg format bump 后，旧 nightly 种子不可读，需等新 nightly 发布
- **z42vm 前置**：xtask 启动即校验 z42vm 可用（`_ensureDriverVm`）；多数命令以子进程驱动 z42vm / z42c

## Deferred

- **z42b 构建编排器**：`src/toolchain/builder/` 仅骨架（无 toml、未接编译）；设计草案见旧
  `docs/design/toolchain/build-orchestrator.md`（前瞻，未实施）
