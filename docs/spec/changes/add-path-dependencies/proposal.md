# Proposal: 本地路径依赖（`[dependencies]` path 依赖）

## Why

z42c 的依赖解析**纯按名字在扁平搜索目录（`Z42_LIBS` / `LibsDirs`）里匹配 `<name>.zpkg`**，没有"标准库 / 非标准库"之分，也不带任何位置信息。这导致**非 stdlib 的组件级依赖**（如 `z42.interactive` 依赖的 `z42.repl`）无法在 manifest 里表达"它的源在哪、要先建它"——只能靠构建脚本手工预建 + stage 进共享目录。

现状代价：

- [xtask_toolchain.z42](../../../../scripts/build/xtask_toolchain.z42) 里有一段 **z42.repl 专属**的特殊处理（`_buildReplLib` + `_ensureToolchainDeps` 调用）：先用 `--output-dir libs` 把 `z42.repl.zpkg` 塞进 stdlib 的 flat dist，`z42.interactive` 才能从同一 `Z42_LIBS` 解析到它。
- `z42.repl` 名字以 `z42.` 开头 → 在 [`_bundleExeDeps`](../../../../src/compiler/z42c.driver/src/Main.z42) 里被 `if (dep.StartsWith("z42.")) continue;` 当成 stdlib，**不打进 apphost 产物**，只能骑在 stdlib libs 里发货——语义上它明明是 interactive 的私有组件。
- `z42c build src/toolchain/interactive/core/z42.interactive.z42.toml` 单独跑会**找不到 z42.repl**，必须依赖 xtask 先 stage。

引入 Cargo 式 `path` 依赖后，manifest 自己表达"这个依赖的源在 `../repl`"，z42c 负责先建依赖闭包 + 解析 + 打包，上述特殊处理**整段删除**，且 `z42c build <toml>` 独立可用。

## What Changes

- `[dependencies]` 的值除"字符串版本"外，支持**表形式** `{ path = "../repl" }`（version 对 path 依赖可省）。
- `DepEntry` 携带 `Path` 字段；`ManifestLoader._parseDeps` 解析并保留 `path`（现注释已声明 `{ version, path }` 但未实现）。
- z42c 单工程 `build`：遇 path 依赖 → 相对本 toml 目录解析出依赖工程 → **先建 path 依赖闭包**（传递 + 去重 + 拓扑序，各建进依赖工程**自己的 dist**）→ 把这些 dist 并入消费方 `libsDirs` → 编译消费方。
- 打包判据修正（User 二次修正 2026-08-29）：**是否 stdlib = 是否真属 `src/libraries/`（或 shipped libs），不再看名字前缀**。z42.repl 不在 src/libraries → 是**私有组件** → colocate 复制进 z42i payload；z42.project 在 src/libraries → 真 stdlib → 走 Z42_LIBS 不复制。`_bundleExeDeps` 判据从 `StartsWith("z42.")` 改为与 publish `_pubBundleProjectDeps` 一致（**path 依赖一律私有复制**）。
- 运行期无需改：[`app.rs` entry-dir search](../../../../src/runtime/src/app.rs) 已先搜 entry zpkg 同目录再搜 libs → colocate 的 z42.repl.zpkg 自动解析。
- `z42.interactive.z42.toml`：`"z42.repl" = "0.1.0"` → `"z42.repl" = { path = "../repl" }`（z42.repl 建进自己的 dist、colocate 进 payload）。
- 删除 xtask 的 `_buildReplLib` 及 `_ensureToolchainDeps` 对它的调用（不再把 z42.repl 塞进 libs）。

**无 zbc/zpkg 格式变更**：path 纯源码/构建期概念，不进二进制；产物里 `declaredDeps` / DEPS 仍是名字。

**打包合并**：本 change 只做 **colocated**（私有依赖分离 zpkg colocate 进 payload，运行期 entry-dir search 解析——零 runtime 改动）。"合成一个文件"由正交的 **single-file**（内嵌分离 zpkg 进 apphost，对齐 .NET PublishSingleFile）承担，需 runtime 支持内嵌 bundle → **独立 follow-up change**，不在本 change（见 design D7/D8 + 部署模型 book 页）。**不引入 `[build].bundle` / 不做源级合编 single-zpkg**（对标 .NET，合并交 single-file，避免托管合并的语义代价）。

### 追加：native 依赖（并入本 change，User 2026-08-29；**Supersedes #332**）

path 依赖把私有组件的 **zpkg**（`z42.repl.zpkg`）colocate 进消费方 payload。但 `z42.repl` 还有个 **native 库** `libz42_repl.{dylib,so,dll}`（host-only REPL 行编辑 cdylib）——它是同一组件的**另一半**，也该跟随组件、colocate 在消费方 zpkg 旁，运行期在**声明它的 zpkg 目录**解析。本 change 一并做：

- **运行期通用 resolver（非标准库 native）**：抽出「给定 zpkg 目录 + native 库名 → 找库」的共享路径 resolver，**唯一布局 = 平铺 `<zpkg-dir>/libX.<suffix>`**（按名定向，不盲扫、无 rid 子目录、非 eager）。**标准库 native 不变**（`<sdk>/native/` eager 扫 + `[Native(lib=)]` 注册，已支持）。
- **repl 归位 + 接入**：`libz42_repl` 从共享 `bin/` → `programs/z42i/`（beside `z42.interactive.zpkg`），由通用 resolver 定位（取代 `repl_native` 的 repl 专属 `programs/z42i` 硬编码）。消除通用 z42vm 盲扫 bin/ 对 repl 的 spurious `ignoring unknown lib repl` WARN（污染 golden，见 memory `xtask-test-z42home-repl-warn-pollution`）。**这一段 = #332 的效果，但走通用机制 → #332 关掉不合。**
- **z42b publish native 复制（多 rid → 平铺拍平）**：发布时按**目标 rid** 把 native 依赖平铺进 dist（镜像 `_pubBundleProjectDeps` 复制 dep zpkg）；「多 rid」复杂度在发布期拍平成单一平铺布局，运行期永不做 rid 选择。移动端复制到 OS 强制目录（android `jniLibs/<abi>/`、ios framework），运行期交 OS loader。

> **repl nuance**：repl 不是 `[Native(lib=)]` ext-builtin——它是带回调 C ABI 的专用 host-editor cdylib，由 repl 子系统 dlopen。故通用化的是**路径解析层**（"非标准库 native 平铺在 zpkg 旁怎么找"）；repl 的回调式加载本身仍专用，只是改用共享 resolver 找路径。**当前唯一非标准库 native 就是 repl**——本 change 铺 resolver + publish 复制机制、接入 repl；`[Native]` 非标准库 app 面（`[native.dependencies]` 语法等）待有真实消费者再落（见 design Deferred）。

**native 无两-nightly 约束**：runtime（Rust resolver）+ z42b publish + packaging，均不涉及 z42c 源新用 stdlib API → 不受轴②约束，可随 PR-2（use）一起落，或独立 PR-3。

## 两阶段落地（自举纪律，跨两个 nightly）

`z42.project`（含 `DepEntry` / `ManifestLoader`）是 **z42c 编译期依赖闭包成员**（`z42c.driver` + `z42c.pipeline` 均 `using Z42.Build.Project`）。按 [bootstrap-seed.md 轴②](../../../../.claude/rules/bootstrap-seed.md)（stdlib API 面），z42c 源码新用一个 stdlib API 必须晚一个 nightly。故本 change 拆两个 PR、跨两个 nightly：

- **阶段 1（support / PR-1）**：`z42.project` 加 `Path` 字段 + 解析。z42c 源码**不动、不读 `.Path`**。→ 发 nightly（种子 z42.project 从此带 `Path`）。
- **阶段 2（use / PR-2）**：阶段 1 的 nightly 发布**之后**，z42c driver 读 `.Path` 做闭包构建/解析/打包 + 切 toml + 删 xtask 特殊处理。

## Scope（允许改动的文件）

### 阶段 1（PR-1，support）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.project/src/DepEntry.z42` | MODIFY | 加 `Path` 字段 + 3 参构造函数（2 参保留或全站改，见 design D1） |
| `src/libraries/z42.project/src/ManifestLoader.z42` | MODIFY | `_parseDeps` 读表内 `path`；构造点传第三参 |
| `src/libraries/z42.project/tests/manifest_path_dep.z42` | NEW | path 依赖解析单测（表形式 / 省 version / 无 path 回落） |
| `src/libraries/z42.project/README.md` | MODIFY | `DepEntry` 行补 `path` 语义 |
| `docs/book/src/toolchain/manifest.md`（或对应 manifest 页） | MODIFY/NEW | `[dependencies]` path 依赖语法说明 |

### 阶段 2（PR-2，use；等 PR-1 nightly 发布后）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.pipeline/src/PathDepPlan.z42` | NEW | path 依赖闭包发现 + 拓扑序 + 各成员 dist 解析（复用 `WorkspaceBuild` 拓扑思路） |
| `src/compiler/z42c.driver/src/Main.z42` | MODIFY | `_build` 前置 path 闭包构建 + 并入 libsDirs；`_bundleExeDeps` 跳过条件改为"非 path 且 z42. 前缀" |
| `src/libraries/z42.project/src/ManifestLoader.z42` | MODIFY | 若需：暴露单工程 dist_dir 解析辅助（[build].dist_dir + `${output_dir}` 模板，缺省 `<dir>/dist`），见 design D3 |
| `src/toolchain/interactive/core/z42.interactive.z42.toml` | MODIFY | `"z42.repl"` 改 `{ path = "../repl" }` |
| `src/compiler/z42c.driver/src/Main.z42`（`_bundleExeDeps` + `_build`） | MODIFY | 判据改真-stdlib（D2）；path 闭包构建 + libsDirs |
| `scripts/build/xtask_toolchain.z42` | MODIFY | 删 `_buildReplLib` + `_ensureToolchainDeps` 调用 |
| `src/compiler/z42c.pipeline/tests/path_dep/<...>` | NEW | 跨工程 path 依赖端到端（先建依赖 → 解析 → 产物打包） |
| `src/compiler/z42c.driver/README.md` / `z42c.pipeline` README | MODIFY | path 依赖解析/构建职责登记 |
| `docs/book/src/compiler/self-hosting.md`（或依赖解析机制页） | MODIFY | path 依赖闭包构建机制 + 与 workspace 的关系 |

### native 依赖（并入 PR-2，或独立 PR-3；无两-nightly 约束，见 D9/D10）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/native/ext.rs` | MODIFY | 抽出通用 `resolve_native_beside(zpkg_dir, lib_name) → Option<PathBuf>`（平铺，按名） |
| `src/runtime/src/corelib/repl_native.rs` | MODIFY | `candidates()` 改用共享 resolver（beside interactive zpkg dir），去 repl 专属 programs/z42i 硬编码 |
| `src/toolchain/builder/core/builder_publish.z42` | MODIFY | native 依赖复制**骨架**（挂 `_pubBundleProjectDeps` 邻位；当前无 `[native.dependencies]` 声明面 → 占位 + Deferred 注释） |
| `scripts/package/xtask_stage_components.z42` | MODIFY | `_pkgStageReplCdylib`：libz42_repl → programs/z42i/（承接 #332） |
| `scripts/package/xtask_package_desktop.z42` | MODIFY | z42i publish 后调 `_pkgStageReplCdylib`（承接 #332） |
| `scripts/package/xtask_package.z42` | MODIFY | `_copyNativeLibs` repl-skip 注释（承接 #332） |
| `scripts/package/xtask_test_stage_components.z42` | MODIFY | repl 不在 bin/、在 programs/z42i/ 断言（承接 #332） |
| `docs/book/src/runtime/native-libraries.md`（或 native 解析机制页） | NEW/MODIFY | native 解析机制（stdlib eager vs 非 stdlib zpkg-relative 平铺）+ 发布期拍平 |

> #332（isolate-repl-cdylib，worktree ../z42-replisolate）的改动**已实现且验证**（cargo+xtask 编译过）——归并进本 change 时**直接搬运**那 5 文件的 diff（packaging 4 + repl_native 1）即可，无需重做；只需在 repl_native 侧把「repl 专属 programs/z42i 派生」进一步抽成通用 `resolve_native_beside` 共享给未来 `[Native]` 消费者。

### 只读引用

- `src/compiler/z42c.pipeline/src/WorkspaceBuild.z42` — 复用拓扑序/dist 解析范式
- `src/compiler/z42c.pipeline/src/DepScan.z42` — libsDirs 多目录扫描解析
- `src/libraries/z42.project/src/BuildConfig.z42` / `ProjectManifest.z42` — dist_dir / manifest 模型
- `scripts/build/xtask_toolchain.z42` 的 `_toolchainZpkg` / dist 解析 — 移植 dist 解析规则参照

## Out of Scope

- workspace（`--workspace`）内的成员解析机制不变（仍按目录 glob + 名字解析兄弟 dist）；path 依赖只服务单工程 `z42c build <toml>` 路径。
- path 依赖的 version 约束校验（path 依赖忽略 version，Cargo 亦然）；语义化版本匹配是独立后续。
- **single-file / 打包合并**（内嵌分离 zpkg 进 apphost）——正交 D 轴，需 runtime 支持，独立 follow-up change（design D7/D8）；本 change 只做 colocated。
- **single-zpkg 托管合并**（源级合编）——对标 .NET 刻意不做，砍掉。
- **`[native.dependencies]` app 声明面**（z42 app/类库经 manifest 声明随工程分发的非标准库 native）——当前唯一非标准库 native 是 repl（由 packaging 直接 colocate），无真实 app 消费者；本 change 只铺 runtime resolver + publish 复制骨架，声明面 Deferred（见 design D9 Deferred）。
- 远程/git 依赖、依赖锁文件——不在本 change。
- `[analyzers]` 段的 path 化——本 change 只做 `[dependencies]`（analyzers 复用 `_parseDeps`，天然顺带解析出 path，但消费逻辑不改）。

## Open Questions

- [ ] PR-2 是否需要在 z42c 引入"自举能力版本号 +1"？倾向否——无新语法/格式，仅 stdlib API 面（轴②）+ 编译器逻辑，两-nightly 已覆盖。待 design 定。
- [ ] 单工程 dist 解析辅助放 `z42.project`（供 z42c 复用）还是 driver 内私有？见 design D3。
