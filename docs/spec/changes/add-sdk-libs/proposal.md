# Proposal: SDK 库 —— 编译器域等 SDK 内部库的统一解析与部署规则

## Why

编译器域的包（`z42.project` / `z42.build` / `z42c.*` …，下称**SDK 库**）不在 shipped `libs/`，而在 SDK 的
`programs/z42c/`。今天「谁能引用它们、引用后要不要复制」由**四套互不相干的机制**各管一块，其中一套在发布态是坏的：

| 消费方 | 今天怎么够到 | 复制吗 | 问题 |
|---|---|---|---|
| analyzer | `kind = "analyzer"` 专门开一个口，把编译器目录并入解析域 | 否（lib 形态） | 规则绑在 kind 上，别人用不了 |
| **build hooks** | 只看 `Z42_LIBS` 下的 zpkg | 否 | 🔴 **装好的 SDK 里编不过**：实测 `z42b build` 一个带 `[build] hooks` 的工程 ⇒ `E0494: 命名空间 Z42.Build 不存在` / `E0443: undefined type: BuildHooks`。开发树里能用，只因 `Z42_LIBS` 碰巧混装了编译器包 |
| 用户 exe / xtask | `{ path = "${compiler_libs}/<包>.zpkg" }` 路径宏 | 是 | 用户清单里写了一个**和 SDK 布局有关**的东西；宏名还是已删的 `compiler-libs/` 目录名，易混 |
| z42b / z42i / z42d | 开发树靠混装 flat 按名解析；运行期 `probing-paths = "../z42c"` | 否 | 「自己住在 SDK 里哪个相对位置」写死在清单里 |

用户（2026-10-01）给出的目标规则：

1. 用户的 exe 只有 runtime（runtime 包里没有 SDK 库）⇒ 用到的 SDK 库要**复制出来**。
2. 在 SDK 里运行的东西（工具链程序、analyzer、hooks）可以**直接引用 SDK 的 zpkg、不复制**；xtask 虽不住在 SDK
   目录里，但它本就强依赖 SDK，也**不复制**。
3. 所有工程默认自动依赖 stdlib；SDK 库**默认不开启**、按需开启；SDK 库不只编译器目录，以后还会有别的目录 ——
   **用户清单里不出现任何 SDK 内部路径**，SDK 内部调整不会让用户配置失效。

## What Changes

**可见性（编译期）**

- 新概念 **SDK 库**：SDK 在自己的 `manifest.toml` 里声明 SDK 库目录（今天 = `programs/z42c`），工具链读它；开发树由同一个
  函数映射到 `artifacts/build/compiler/z42c.driver/release/dist`。SDK 库只暴露 `libs/` 里**没有**的包（编译器目录里那套
  stdlib 副本对解析不可见）。解析序：私有 dist → `libs/` → SDK 库。
- **exe / lib**：SDK 库**按名声明才可见**（`"z42.project" = "*"`，不写路径）—— 这就是「开启」；未声明的看不见。
- **analyzer / build hooks**：SDK 库**自动可见**，免声明（它们存在的意义就是扩展 SDK）。修好发布态的 hooks。

**部署（运行期）**

- **exe 默认复制**：用到的 SDK 库连同它在 SDK 库内的**传递闭包**（沿 zpkg 依赖表）复制进产物；lib 不打包，由最终 exe 决定。
- **analyzer / hooks 不复制**：由宿主进程（z42c / z42b）提供，且必须是宿主已加载的那份（类型同一性）。
- **新 `deploy = "sdk"`**：声明的 SDK 库不复制；z42c 在 runtimeconfig 侧车写 `sdk-libs = true`；VM 运行期把「它所在 SDK 的
  SDK 库目录」加进依赖搜索序（排在 `libs/` 之后）。SDK 根沿用 VM 已有推断（`Z42_HOME` → `Z42_PORTABLE_VM` 反推 →
  VM 自身位置），开发树 / CI 用 `Z42_SDK_LIBS` 显式指定。找不到 ⇒ 明确报错「需要在 z42 SDK 上运行」。
  xtask 与 z42b / z42i / z42d 用它（后三者替换掉 `probing-paths = "../z42c"`）。

**过渡**

- `${compiler_libs}` 宏：本变更起发 warning（提示改为按名声明），一个 release 后删除。

## Scope

| 文件 / 模块 | 变更 |
|---|---|
| `src/compiler/z42.project/` | `SdkLibs`：读 SDK 清单的 SDK 库目录 + 开发树映射（单一真相源，替代 `CompilerDomain` 的硬编码）；`DepEntry.Deploy` 接受 `"sdk"` |
| `src/compiler/z42c.pipeline/src/BuildSession.z42` | 解析域：SDK 库按名（exe/lib 需声明；analyzer / hooks 自动）；过滤 stdlib 副本 |
| `src/compiler/z42c.driver/src/{Main,ExeDeps,BuildPaths}.z42` | 同上（driver 路径）；`_bundleExeDeps` 走 SDK 库传递闭包；`deploy = "sdk"` 不复制 + 写侧车；宏 warning |
| `src/runtime/src/probing.rs`（+ runtimeconfig 读取） | `sdk-libs = true` ⇒ 追加 SDK 库目录；`Z42_SDK_LIBS`；SDK 清单字段；缺失报错 |
| `src/toolchain/builder/core/builder_hooks.z42` | hooks 编译走 SDK 库自动可见 |
| `scripts/package/*`、`scripts/packages.toml` | SDK 包 `manifest.toml` 写 SDK 库目录字段 |
| 阶段 2：`scripts/xtask.z42.toml`、`src/toolchain/{builder,interactive,devtools}/core/*.z42.toml`、`.github/ci/xtask`、ci-bootstrap | 改 `deploy = "sdk"`；`Z42_SDK_LIBS` |
| `docs/reference/src/toolchain/{z42-toml,runtime-settings,compile-time-extensions}.md`、`docs/internals/src/compiler/project-model.md`、`docs/agent/rules/bootstrap-seed.md` | 规则上浮 |

## Out of Scope

- SDK 库的 API 稳定性承诺：本变更**不**承诺兼容（文档明说不稳定）。
- `Std.*` 的现行行为（免声明、从 `libs/` 解析、不复制）不变。
- 新增别的 SDK 库目录：本变更只建立「SDK 清单声明目录」的机制，目录仍只有 `programs/z42c`。

## 分阶段（自举纪律：support 与 use 分两个 nightly）

1. **support**：上面全部「可见性 / 部署」能力 + 宏 warning；仓内消费者不动（xtask 仍用宏，z42b 等仍用 probing）。
2. **use**（support 进 nightly 之后）：xtask、z42b / z42i / z42d 改 `deploy = "sdk"`；CI 垫片 / ci-bootstrap 设 `Z42_SDK_LIBS`；
   文档与示例去掉宏。—— 种子 z42c 必须先认识 `deploy = "sdk"`，否则 CI 冷启动编 xtask 即挂。
3. **删宏**（再一个 release 之后）。

## Open Questions

无（2026-10-01 与用户讨论已定：按包声明而非全局开关；xtask 不复制；宏过渡后删除）。
