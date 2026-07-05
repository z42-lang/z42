# Tasks: 新增 build toolchain / build workload

> 状态：🟢 已完成 | 创建：2026-07-05 | 完成：2026-07-05
> 占用子系统：`toolchain`（ACTIVE.md 归档时释放）
> 变更类型：feat（新命令）+ refactor（路径从 toml 读）

## 进度概览
- [x] 阶段 1: helper + build workload ✅（本地跑通：4 lib → libraries/dist）
- [x] 阶段 2: build toolchain（publish 4 apphost，路径从 toml）✅（4 apphost → 各 publish_dir）
- [x] 阶段 3: 删 build launcher + 修 publish_dir 一致性 + 删泄漏产物 + gitignore ✅
- [x] 阶段 4: packaging（z42b 路径×3）+ build sdk 合并 apphost 改从 toml 读 ✅
- [x] 阶段 5: 文档 + 验证 + 归档 ✅
- [x] 阶段 6（追加）: `build test` 编译测试资产 ✅
- [x] 阶段 7（追加, User directive）: build sdk 补 apphost 完整性 + scratch 目录移出 build/ ✅

## 阶段 6/7 追加（User 迭代指令）
- [x] 6.1 `build test`：`_buildTest`（xtask_regen.z42）= `_ensureToolchainDeps` + `_regenGolden`；`build test` 子命令注册。验证 207 golden ok/0 fail
- [x] 7.1 **build sdk 完整性**：`_sdkMergeApphosts`（xtask_toolchain.z42）—— 跑 build toolchain + publish z42c → 把 5 个 apphost（z42/z42c/z42b/z42d/z42i）从各 publish_dir merge 进 SDK（`Directory.Copy` 递归）。验证：SDK 含 bin/{z42vm,z42c,z42b,z42d,z42i}+z42+programs/*。（User 之前见空 = stale xtask.zpkg）
- [x] 7.2 launcher `bin` 保持根（`z42`），与 nightly SDK 布局一致（User 裁决，不改 bin/z42）
- [x] 7.3 **scratch 移出 `build/`**（User: build 目录只放编译/publish 产物）：`.stdlib-run`/`alllibs`/`e2e`/`selfhost-gen1`/`dogfood` → `artifacts/.scratch/`（gitignored）。build/compiler 现仅 per-member + z42c.driver。验证 test compiler 287 PASS（仅 User 并行 DRAFT `port-incremental-build-cache` 的 incremental/ 测试失败，正交）

## 阶段 1: helper + build workload
- [x] 1.1 `xtask_toolchain.z42` 新建；`_desktopPublishDir(root, tomlPath)`：Std.Toml 读 `[platform.desktop].publish_dir`，相对 toml 目录解析绝对路径（缺省=`${output_dir}/publish`，镜像 z42b publish 语义——见 3.5）
- [x] 1.2 `_buildWorkload()`：编 desktop/ios/android/wasm 4 lib（`_z42cBuildToml`，Z42_LIBS=stdlib）→ dist；从 packaging 内联提取
- [x] 1.3 `build workload` 子命令注册 + dispatch

## 阶段 2: build toolchain
- [x] 2.1 `_buildToolchain()`：前置 ensure（stdlib/z42vm/z42c + `_buildWorkload`）→ 对 launcher/builder/devtools/interactive 各 `publish <toml>`（不带 --output，落 toml 的 publish_dir）
- [x] 2.2 `build toolchain` 子命令注册 + dispatch

## 阶段 3: 清理 + 一致性
- [x] 3.1 删 `build launcher` 子命令（并入 toolchain）
- [x] 3.2 修 devtools publish_dir → `toolchain/devtools/publish`；interactive → `toolchain/interactive/publish`（User 手改路径 + Claude 同步 4 个 toml 注释）
- [x] 3.3 删泄漏产物 `src/toolchain/launcher/core/{z42,programs/}`（+ 顺带清 artifacts/build/toolchain/launcher 根下旧平铺 zpkg/zsym——dist_dir→output_dir 切换后与 dist/ 重复）
- [x] 3.4 `.gitignore` 补规则防 publish 落源码树（`src/**/programs/`、`src/toolchain/*/core/z42`、`.../bin/`）
- [x] 3.5 z42b publish_dir 缺省改 `${output_dir}/publish`（User 设定 2026-07-05）：`builder_publish.z42` `_pubDesktop` 缺省分支 + `_pubDefaultPublishDir` helper（[build].output_dir → workspace 继承 → 项目目录，尾接 /publish）；`xtask_toolchain.z42` `_desktopPublishDir` 缺省对齐、`_toolchainZpkg` 补 output_dir 级联；z42c.driver.toml / launcher.toml 注释 + launcher.md / project.md 缺省描述同步。**已验证**：① `[build].output_dir=out` → `out/publish/<name>` ✓；② 无 [build] → `<projDir>/publish/<name>`（不再泄漏项目根）✓；③ z42c.driver（workspace 继承）→ `artifacts/build/compiler/z42c.driver/release/publish/{bin/z42c, programs/z42c/7 zpkg}` ✓；④ launcher 显式 publish_dir 回归不变 ✓；z42b + xtask 均重编通过

## 阶段 4: 复制端改从 toml 读（核心 SoT）
- [x] 4.1 `package desktop`：workload 编译 → `_buildWorkload`；apphost 定位/publish → 经 `_desktopPublishDir`，去 `_pkgStageDir` 硬编码（toolchain 部分）
- [x] 4.2 `build sdk` / `stage-toolchain`：复制 toolchain apphost 处改从 toml 读 publish_dir
- [x] 4.3 grep 确认无残留硬编码 `artifacts/build/toolchain/<comp>` 字面量（toolchain apphost 域）

## 阶段 5: 文档 + 验证 + 归档
- [x] 5.1 scripts/README + src/toolchain/README + book/build.md：新命令 + 路径-从-toml 约定
- [x] 5.2 验证：`build workload` / `build toolchain` 产物落各 toml publish_dir；改 toml 路径后无需动 xtask（改一个 publish_dir 重跑确认）
- [x] 5.3 `xtask test` 全绿 + `package desktop` 冒烟（apphost 仍正确落包）
- [x] 5.4 ACTIVE.md 释放 toolchain 锁；归档

## 备注
- launcher 依赖 workload libs → build toolchain 前置调 build workload。
- 泄漏产物根因：publish_dir 缺省=项目目录；加 publish_dir + gitignore 双保险。
