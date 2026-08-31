# Proposal: `[native]` 依赖声明面 + build-hook 产 native + 传递复制

## Why

组件私有 native 库（当前唯一实例：REPL 行编辑器 `libz42_repl`）今天靠 packaging **硬编码特殊处理**落地：
`_pkgBuildAndStageRuntime` 里一条独立 `cargo build -p z42-repl` + `_pkgStageReplCdylib` 手动把
`libz42_repl.{dylib,so,dll}` 拷进 `programs/z42i/`。这有两个问题：

1. **不通用**：每加一个带 native 的组件都要在 xtask 里再写一段"特殊拷贝"。归档设计 D9/D10 早已铺好
   colocation 地基（`resolve_native_beside` + `_pubBundleProjectNativeDeps` 骨架），但**声明面一直 Deferred**，
   没有真实消费者去驱动。
2. **不自包含**：native 不由 `z42 publish` 产出，publish 目录不完整，packaging 无法"整目录直接拷"。

目标：让一个库/组件在 manifest 里**声明**它携带/需要的 native 库（语言无关：rust/c/c++/vendor blob 一视同仁），
由 build-hook 现场产出（或指向预编译文件），`z42 publish` 就把 native 落进自己的 dist/payload，**引用它的项目
沿依赖闭包自动复制**目标平台那一份。落地后 xtask 的 repl 特殊处理**全部删除**，packaging 退化为"整目录拷
publish 输出"。

## What Changes

- 新 manifest 段 **`[native.<name>]`**：声明本包携带的 native 库；跨平台按 **rid 目录 + 派生文件名**约定统一表达
  （`<dist>/<rid>/lib<name>.<平台后缀>`）。
- 新 **`BuildHooks.ProvideNative(ctx)`** 相位：专用窄钩子，只产 native（`cargo build` 现场编 + 拷入 `Dirs.Dist/<rid>/`
  + `AddOutput`）。消费者遍历闭包时只跑这个，不跑通用 `BeforeAssets`。
- **z42.repl 接入**：`z42.repl.z42.toml` 加 `[build] hooks` + `[native.z42_repl]`；新 `repl/hooks/hooks.z42` 用与 VM
  同源的 cargo（裸 `cargo -p z42-repl`，同 rid→target 映射）产 `libz42_repl` 到 z42.repl 自己的 `dist/<rid>/`
  —— z42.repl 由此**独立自包含**。
- **传递复制**：填实 `_pubBundleProjectNativeDeps` —— publish 消费者时走 path-dep 闭包，对声明 `[native]` 的 dep
  跑其 `ProvideNative`，把**目标 rid** 那份 native 平铺进消费者 payload（`programs/z42i/`）。
- **删 xtask 特殊处理**：`_pkgStageReplCdylib`（定义 + 3 调用点）、`_pkgBuildAndStageRuntime` 的 `cargo build -p
  z42-repl`、`_copyNativeLibs` 的 repl 排除分支。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.project/src/NativeSpec.z42` | NEW | `[native.<name>]` 模型（受限子集 sealed class） |
| `src/libraries/z42.project/src/ProjectManifest.z42` | MODIFY | 加 `NativeSpec[] Natives` + count（构造后填，仿 Analyzers/Lints） |
| `src/libraries/z42.project/src/ManifestLoader.z42` | MODIFY | 加 `_parseNative`（构造后填 pm.Natives） |
| `src/libraries/z42.build/src/BuildHooks.z42` | MODIFY | 加 `virtual void ProvideNative(IPipelineContext ctx)` |
| `src/toolchain/interactive/repl/z42.repl.z42.toml` | MODIFY | 加 `[build] hooks = "hooks"` + `[native.z42_repl]` |
| `src/toolchain/interactive/repl/hooks/hooks.z42` | NEW | `ProjectHooks : BuildHooks`，override `ProvideNative`（cargo 产 libz42_repl → Dirs.Dist/<rid>/） |
| `src/toolchain/builder/core/builder_publish.z42` | MODIFY | 填 `_pubBundleProjectNativeDeps`：闭包 → 跑 dep `ProvideNative` → 拷目标 rid native 进 payload |
| `src/toolchain/builder/core/builder_hooks.z42` | MODIFY | 加"按 dep toml 载入并跑其 ProvideNative"辅助（复用 `_loadProjectHooks`） |
| `scripts/package/xtask_stage_components.z42` | MODIFY | 删 `_pkgStageReplCdylib` + `_pkgBuildAndStageRuntime` 里的 `cargo build -p z42-repl` |
| `scripts/package/xtask_package_desktop.z42` | MODIFY | 删 `_pkgStageReplCdylib(cargoOut, z42iStage)` 调用（L175） |
| `scripts/package/xtask_test_stage_components.z42` | MODIFY | 删 `_pkgStageReplCdylib` 调用（L45） |
| `scripts/package/xtask_package.z42` | MODIFY | 删 `_copyNativeLibs` 的 repl 排除分支（L254） |
| `docs/book/src/runtime/native-libraries.md` | MODIFY | §3 由 Deferred 改为"声明面 + build-hook 机制"实况 |
| `src/libraries/z42.project/README.md` | MODIFY | 功能索引 + 核心文件加 NativeSpec / `[native]` |
| `src/libraries/z42.build/README.md` | MODIFY | 功能索引加 ProvideNative 相位 |
| `src/toolchain/interactive/repl/README.md` | MODIFY | 说明 native 由 hook 产出、不再靠 packaging 特殊处理 |
| `src/toolchain/interactive/repl/hooks/tests/native_provide/...` | NEW | ProvideNative + 传递复制 e2e/单测 |

**只读引用**：
- `src/runtime/src/native/ext.rs`（`resolve_native_beside` 运行期不变，仅确认契约）
- `src/compiler/z42c.pipeline/src/PathDepPlan.z42`、`z42c.driver/src/Main.z42`（闭包/`_resolveDistDir` 复用，不改）
- `versions.toml`、`scripts/package/xtask_package.z42` 的 `_cargoTargetFor`/`_ridToCargo`（rid→target 映射参照）

## Out of Scope

- **显式 per-rid 文件覆盖**（`files."rid" = "..."`，vendor blob 破约定）→ Deferred，等真实 vendor 消费者。
- **静态链接 native**（`.a` 链进 apphost）→ 本 change 只做动态 dlopen colocation。
- **`[native.dependencies]` 之"app 依赖外部预编译库"面**（本 change 是"本包**提供** native"面）→ 同族，后续。
- **移动端 OS 目录复制**（jniLibs/framework）细节 → 骨架留、桌面先行（native-libraries.md §2 已述方向）。
- **真 toolchain 路径固定**（versions.toml 提供 cargo 路径而非仅 channel 校验）→ 独立特性。

## Open Questions

- [ ] `[native.<name>]` 空表 vs `[native] libs=[...]` 数组：默认取每库一张表（可扩展 per-lib 选项），待 design 定死。
- [ ] 单 PR vs support/use 两 PR：已初步核实种子只编 xtask/z42c 源、二者不读 `.Native` → 倾向单 PR；实施第一步再 grep 确认。
