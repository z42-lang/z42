# Proposal: SDK 库 —— 编译器域等 SDK 内部库的统一解析与部署规则

## Why

编译器域的包（`z42.project` / `z42.build` / `z42c.*` …，下称**SDK 库**）不在 shipped `libs/`，而在 SDK 的
`programs/z42c/`。今天「谁能引用它们、引用后要不要复制」由**四套互不相干的机制**各管一块，其中一套在发布态是坏的：

| 消费方 | 今天怎么够到 | 复制吗 | 问题 |
|---|---|---|---|
| analyzer | `kind = "analyzer"` 专门开一个口，把编译器目录并入解析域 | 否（lib 形态） | 规则绑在 kind 上，别人用不了 |
| **build hooks** | 只看 `Z42_LIBS` 下的 zpkg | 否 | 🔴 **装好的 SDK 里编不过**：实测 `z42b build` 一个带 `[build] hooks` 的工程 ⇒ `E0494: 命名空间 Z42.Build 不存在` / `E0443: undefined type: BuildHooks`。开发树里能用，只因 `Z42_LIBS` 碰巧混装了编译器包 |
| 用户 exe / xtask | `{ path = "${compiler_libs}/<包>.zpkg" }` 路径宏 | 是 | 用户清单里写了一个和 SDK 布局有关的宏；宏名还是已删的 `compiler-libs/` 目录名，易混 |
| z42b / z42i / z42d | 运行期 `probing-paths = "../z42c"` | 否 | 无问题（它们就住在 SDK 里，相对路径与 SDK 版本天然一致）—— **保持** |

另一处不一致：**xtask 在本地与 CI 上的运行方式不同**。本地 `./xtask` 由下载的 SDK（`.z42`）publish 出来、跑在**同一份 SDK**
上 —— 编译与运行同一个工具链，版本严格一致。CI 上 xtask 由种子 SDK 编出，却跑在 cargo 现编的 z42vm + 构建树 stdlib 上 ——
「用种子编、用新版跑」的错位一直存在，且因此 xtask 只能把编译器域包复制在身边。

用户（2026-10-01）定下的规则：

1. 用户的 exe 只有 runtime（runtime 包里没有 SDK 库）⇒ 用到的 SDK 库要**复制出来**。
2. 在 SDK 里运行的东西（工具链程序、analyzer、hooks、xtask）**直接引用 SDK 的 zpkg、不复制**。SDK 库与 SDK 版本强绑定，
   不引入可随意指定位置的新环境变量；能用 SDK 内相对位置 / `${Z42_HOME}` 的就用它。
3. 所有工程默认自动依赖 stdlib；SDK 库**默认不开启**、按需开启。
4. 用 `${Z42_HOME}` 路径却找不到时，提示「是否没有安装 z42 SDK」。
5. CI 与本地一致：xtask 一律跑在 SDK 上 ⇒ xtask 不复制。

## What Changes

**可见性（编译期）**

- **exe / lib**：SDK 库**按名声明才可见**（`"z42.project" = "*"`，不写路径）—— 这就是「开启」；未声明的看不见（E0494 + hint）。
- **analyzer / build hooks**：SDK 库**自动可见**，免声明。修好发布态的 hooks。
- SDK 库目录里那套 stdlib 副本对解析不可见；解析序：私有 dist → `libs/` → SDK 库。
- SDK 库目录的定位沿用现有 `CompilerDomain`（`Z42_COMPILER_LIBS` → `Z42_HOME/programs/z42c` → 由 `Z42_PORTABLE_VM`
  反推 → 开发树 `z42c.driver` dist）—— 不新增定位机制。

**部署（运行期）**

- **exe 默认复制**：用到的 SDK 库连同它在 SDK 库内的**传递闭包**复制进产物；lib 不打包。
- **analyzer / hooks 不复制**：由宿主进程提供（类型同一性）。
- **新 `deploy = "sdk"`（通用配置）**：声明的 SDK 库不复制；z42c 在 runtimeconfig 侧车的 `probing-paths` **自动补一条**
  `${Z42_HOME}/programs/z42c`（只写占位符、不烤具体路径，沿用 VM 已有的占位符展开）。**VM 解析逻辑不变。**
- **VM 提示**：依赖解析失败、且侧车里有展开后不存在的 `${Z42_HOME}/…` 条目 ⇒ 报错附「是否没有安装 z42 SDK？」。
- z42b / z42i / z42d：**不变**（`../z42c` 相对路径）。

**CI 与本地一致**

- ci-bootstrap 把下载的种子 SDK 落成 job 的 `.z42`（同本地 `install-z42.sh`），用它编出并**运行** xtask；compile-toolchain 把这份
  SDK 作为 artifact 交给下游；`xtask-bootstrap-artifact` 与 CI 垫片改用 SDK 的 VM 跑 xtask（xtask 构建 / 测试当前源码时的子进程
  仍用构建树工具，同本地）。
- 之后 xtask 改 `deploy = "sdk"`，不再复制。

**过渡**：`${compiler_libs}` 宏本变更起发 warning，一个 release 后删除。（User 2026-10-01 裁定提前删：W0609 随即退役，旧写法当场报错并给出按名写法。）

## Scope

| 文件 / 模块 | 变更 |
|---|---|
| `src/compiler/z42c.pipeline/src/BuildSession.z42`、`src/compiler/z42c.driver/src/{Main,ExeDeps,BuildPaths}.z42` | 可见性规则；stdlib 副本过滤；exe 复制传递闭包；`deploy = "sdk"`（不复制 + 补侧车 probing）；宏 warning |
| `src/compiler/z42.project/` | `DepEntry.Deploy` 接受 `"sdk"`（校验） |
| `src/toolchain/builder/core/builder_hooks.z42` | hooks 编译走 SDK 库自动可见 |
| `src/runtime/src/probing.rs` 及依赖解析报错处 | `${Z42_HOME}` 未解析的提示 |
| `.github/actions/{ci-bootstrap,xtask-bootstrap-artifact}`、`.github/ci/xtask`、`.github/workflows/ci.yml` | CI 上 xtask 跑在 SDK 上 |
| `scripts/xtask.z42.toml` | 阶段 3：`deploy = "sdk"` |
| `docs/reference/src/toolchain/{z42-toml,runtime-settings,compile-time-extensions}.md`、`docs/internals/src/compiler/project-model.md`、`docs/internals/src/devinfra/{ci,xtask}.md`、`docs/agent/rules/bootstrap-seed.md` | 规则上浮 |

## Out of Scope

- SDK 库的 API 稳定性承诺（文档明说不稳定）。
- `Std.*` 现行行为（免声明、从 `libs/` 解析、不复制）不变。
- z42b / z42i / z42d 的部署方式不变。
- 新的 SDK 库目录（今天只有 `programs/z42c`）。

## 分阶段

1. **support**（一个 PR）：可见性、exe 复制闭包、`deploy = "sdk"`、VM 提示、hooks、宏 warning。仓内消费者不动。
2. **CI 与本地一致**（一个 PR，可与 1 并行）：xtask 在 CI 上跑在种子 SDK 上（仍复制编译器域包，行为不变）。
3. **use**（1 进 nightly 之后）：xtask 改 `deploy = "sdk"`；文档与示例去掉宏。—— 种子 z42c 必须先认识 `deploy = "sdk"`。
4. **删宏**（再一个 release 之后）。

## Open Questions

无（2026-10-01 与用户讨论已定）。
