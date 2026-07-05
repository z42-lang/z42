# Proposal: 新增 build toolchain / build workload —— 路径从 z42.toml 读，不硬编码

## Why

1. **命令面不对称**：`build compiler`↔`src/compiler`、`build stdlib`↔`src/libraries` 都是"目录→产物"的整齐映射，但 `src/toolchain/` 没有对应命令——4 个 apphost（launcher/z42b/z42d/z42i）+ 4 个 workload lib **只在 `package desktop` 里内联编译/发布**（[xtask_package_desktop.z42:83-115](../../../../scripts/package/xtask_package_desktop.z42#L83)），dev 想单独产工具链没有入口。`build launcher` 又是单独的 apphost stub 命令，零散。
2. **xtask 硬编码 toolchain 输出/publish 路径（SoT 违背）**：packaging 用 `_pkgStageDir(root,"launcher")` 显式暂存目录 + `artifacts/build/toolchain/builder/z42.builder.zpkg` 硬编码，而不是读组件 toml 的 `[platform.desktop].publish_dir`。**改 toml 路径就得改 xtask** ——唯一真相源应是 toml。
3. **publish_dir 不一致 + 泄漏产物**：devtools 缺 `/publish`、interactive 落在 `devtools/` 下（应 `interactive/`）；`src/toolchain/launcher/core/` 下有 publish 泄漏的 `z42`(372KB) + `programs/`（publish_dir 缺省=项目目录时的残留，正是加 publish_dir 的动机）。

## What Changes

1. **新 `xtask build workload`**：编 4 个 workload lib（desktop/ios/android/wasm，kind=lib）→ 各自 dist。从 packaging 内联提取为独立命令（User：workload 独立处理）。
2. **新 `xtask build toolchain`**：对 4 个 apphost 组件各调 `publish <toml>` → 产 native apphost 到**其 toml 声明的 publish_dir**。前置：workload libs（launcher 依赖它们）→ 调 `build workload`；+ stdlib/z42vm/z42c（同 build compiler）。
3. **删 `build launcher`**：并入 `build toolchain`（launcher 是其中一个 apphost）。
4. **核心原则——路径从 toml 读**：新增 xtask helper `_desktopPublishDir(root, tomlPath)`（用 Std.Toml 读 `[platform.desktop].publish_dir`，相对 toml 目录解析）。凡"定位/复制 toolchain apphost 产物"的 xtask 代码（`build toolchain` 自身、`build sdk` 组装、`stage-toolchain`、`package desktop` 复制端）一律经该 helper 从 toml 读，**不再硬编码 `artifacts/build/toolchain/...` 或 `_pkgStageDir`**。
5. **修 publish_dir 一致性**：devtools → `toolchain/devtools/publish`；interactive → `toolchain/interactive/publish`（launcher/builder 已对，`toolchain/<组件>/publish` 统一格式）。
6. **删泄漏产物 + gitignore**：删 `src/toolchain/launcher/core/{z42,programs/}`；`.gitignore` 补 `src/toolchain/**/core/z42`、`programs/` 等，防 publish 误落源码树。
7. **publish_dir 缺省改 `${output_dir}/publish`**（User 设定 2026-07-05）：z42b publish 在 `--output` 与 `[platform.desktop].publish_dir` 均未设时，不再落项目目录（泄漏根因），改为 `[build].output_dir`（无则 workspace `[workspace.build].output_dir` 继承，再无则项目目录）+ `/publish`——对齐 [project.md L3 表](../../../design/compiler/project.md) 已有的 `${output_dir}/publish` 默认。xtask `_desktopPublishDir` 缺省语义同步；`_toolchainZpkg` 补 `output_dir → ${output_dir}/dist` 级联（launcher toml 已从 dist_dir 切 output_dir）。

## Scope（允许改动的文件）

| 文件 | 类型 | 说明 |
|------|------|------|
| `scripts/xtask_cli.z42` | MODIFY | build 子命令：加 `toolchain`/`workload`，删 `launcher`；dispatch |
| `scripts/build/xtask_toolchain.z42` | NEW | `_buildToolchain`（publish 4 apphost，路径从 toml）/ `_buildWorkload`（编 4 lib）/ `_desktopPublishDir` helper |
| `scripts/package/xtask_package_desktop.z42` | MODIFY | workload/apphost 编译改调 `_buildWorkload`/`_buildToolchain`；`_z42bPublish` 定位改经 `_desktopPublishDir`（去 `_pkgStageDir` 硬编码 for toolchain） |
| `scripts/build/xtask_stdlib.z42` | MODIFY | `_buildSdk` / `_stageToolchain` 复制 toolchain apphost 处改从 toml 读 publish_dir |
| `src/toolchain/devtools/core/z42.devtools.z42.toml` | MODIFY | publish_dir → `toolchain/devtools/publish` |
| `src/toolchain/interactive/core/z42.interactive.z42.toml` | MODIFY | publish_dir → `toolchain/interactive/publish` |
| `src/toolchain/launcher/core/z42.launcher.z42.toml` | MODIFY | （已加 publish_dir，确认格式一致） |
| `src/toolchain/builder/core/z42.builder.z42.toml` | MODIFY | （已加 publish_dir，确认一致） |
| `src/toolchain/launcher/core/z42` + `programs/` | DELETE | 泄漏产物 |
| `.gitignore` | MODIFY | 防 publish 落源码树 |
| `scripts/README.md` + `src/toolchain/README.md` | MODIFY | 新命令 + 路径-从-toml 约定 |
| `docs/book/src/dev/build.md` | MODIFY | build toolchain/workload 命令 |
| `src/toolchain/builder/core/builder_publish.z42` | MODIFY | publish_dir 缺省 `${output_dir}/publish`（What Changes 7）|
| `src/compiler/z42c.driver/z42c.driver.z42.toml` | MODIFY | 注释：publish_dir 缺省语义更新 |
| `docs/design/runtime/launcher.md` | MODIFY | publish_dir 缺省描述更新（§publish）|
| `docs/design/compiler/project.md` | MODIFY | publish_dir 缺省描述更新（§apphost-as-config）|

**只读引用**：`src/libraries/z42.project/src/{ManifestLoader,DesktopConfig}.z42`（publish_dir 解析既有语义）。

## Out of Scope

- packaging 里**非 toolchain** 的硬编码路径（stdlib/z42c 等）——本次只把 **toolchain apphost/workload** 的路径改为 toml 驱动。
- `build sdk` 与 `stage-toolchain` 的合并（simplify 已评估为独立更大变更，不在此）。
- devtools/interactive 的功能实现（仍是 planned 占位）。

## Open Questions（已裁决 2026-07-05）

- [x] `build toolchain` **自动**调 `build workload` 作前置（launcher 依赖它，同 build compiler 缺 stdlib 时自建）。
- [x] `build all` **不纳入** toolchain/workload（保持 runtime+compiler+stdlib，避免每次全量）。
