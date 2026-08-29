# Tasks: 本地路径依赖（path 依赖）+ native 依赖

> 状态：🟡 DRAFT（待 User 确认后进实施） | 创建：2026-08-29
> 跨两个 nightly：阶段 1（support）先合并发 nightly → 阶段 2（use）方可开工。
> **native 依赖（User 2026-08-29 并入本 change，Supersedes #332）**：无两-nightly 约束，随 PR-2 或独立 PR-3。

## 进度概览
- [ ] 阶段 1（PR-1，support）：z42.project 加 `DepEntry.Path` + 解析
- [ ] ⏸ 等阶段 1 nightly 发布
- [ ] 阶段 2（PR-2，use）：z42c path 闭包构建 + 真-stdlib 打包判据（colocated）+ 切 toml + 删 xtask 特殊处理
- [ ] native 依赖（PR-2 内或 PR-3）：runtime 通用 resolver + repl 接入（搬 #332）+ z42b publish 复制骨架
- [ ] （独立 follow-up）single-file 打包合并——需 runtime 内嵌 bundle，另开 change

## 阶段 1（PR-1，support）——独立分支/worktree（纯 z42.project，z42c 不消费）
- [ ] 1.1 `DepEntry.z42`：加 `public string Path;` + 3 参构造函数（design D1）
- [ ] 1.2 `ManifestLoader._parseDeps`：表形式读 `path`（`dv.ContainsKey("path")`），构造点传第三参；纯字符串/省 path → `""`
- [ ] 1.3 单测：path 依赖解析（表/version+path/省 version/纯字符串回落）
- [ ] 1.4 `z42.project/README.md`：`DepEntry` 行补 path 语义
- [ ] 1.5 book manifest 页：`[dependencies]` path 语法（support 阶段先记语法，use 机制留阶段 2）
- [ ] 1.6 GREEN：`xtask test`（全 stage）；确认 z42c 源未读 `.Path`（`grep -n "\.Path" src/compiler` 应为空）
- [ ] 1.7 归档 PR-1 + 合并 → **等 nightly**

## 阶段 2（PR-2，use）——独立分支/worktree，PR-1 nightly 发布后
- [ ] 2.1 `PathDepPlan.z42`（新）：path 闭包发现 + 拓扑序 + 各 dist 解析（复用 WorkspaceBuild 范式；环检测）
- [ ] 2.2 dist 解析辅助（design D3：z42.project `ResolveDistDir` 或 driver 私有，看 BuildConfig 现状定）
- [ ] 2.3 `Main.z42 _build`：前置 path 闭包构建（逐个 `_build`）+ 消费方 libsDirs 追加闭包 dist（去重）
- [ ] 2.4 `_bundleExeDeps`：判据从 `StartsWith("z42.")` 改为真-stdlib（path 依赖私有复制），与 `_pubBundleProjectDeps` 一致（design D2）
- [ ] 2.5 端到端 fixture `path_dep/`：lib foo + exe bar（`{ path="../foo" }`）→ 先建 + 构建期解析 + foo.zpkg colocate 进 bar dist
- [ ] 2.6 `z42.interactive.z42.toml`：`"z42.repl"` → `{ path = "../repl" }`
- [ ] 2.7b（验证）`_pubBundleProjectDeps` 定位 path 依赖 toml：确认删 `_buildReplLib` 后 z42.repl（inLibs=false）被正确 build + colocate 进 `programs/interactive/`；必要时教它认 path 依赖位置
- [ ] 2.8b 验证 z42.repl.zpkg 出现在 SDK `programs/interactive/`（移出 libs）；`strings` 确认 z42.interactive.zpkg 不含 Repl（未合并，符合 colocated）
- [ ] 2.7 xtask：删 `_buildReplLib` + `_ensureToolchainDeps` 调用（+ 相关注释）
- [ ] 2.8 验证 `_z42bPublish` 把 bundle 的 z42.repl.zpkg 带进 z42i payload；REPL 冒烟
- [ ] 2.9 README（z42c.driver / z42c.pipeline）+ book self-hosting / 依赖解析页：path 闭包机制
- [ ] 2.10 GREEN：`xtask test`（全 stage）+ `xtask test compiler` + `xtask test bootstrap`（未越界）+ `xtask build toolchain` 手验 z42i
- [ ] 2.11 归档 PR-2 + 合并 + 清理 worktree/分支

## native 依赖（PR-2 内或独立 PR-3；无两-nightly 约束；Supersedes #332，见 D9/D10）
> #332 的 5 文件 diff 已实现+验证——**直接搬运**，勿重做。worktree ../z42-replisolate 有原始 diff。
- [ ] N.1 搬 #332 packaging 4 文件 diff：`xtask_stage_components._pkgStageReplCdylib`（libz42_repl→programs/z42i/）+ `xtask_package_desktop` 调用 + `xtask_package._copyNativeLibs` 注释 + `xtask_test_stage_components` 断言
- [ ] N.2 `native/ext.rs`：抽 `resolve_native_beside(zpkg_dir, lib_name) → Option<PathBuf>`（平铺，按名，复用 `parse_z42_lib_name` 反向拼 `lib<name><DLL_SUFFIX>`）
- [ ] N.3 `repl_native.rs`：搬 #332 的 candidates() diff（apphost current_exe→`<sdk>/programs/z42i/`）；把那段派生+平铺查找改为调用 N.2 的共享 `resolve_native_beside`
- [ ] N.4 `builder_publish.z42`：native 依赖复制**骨架**（挂 `_pubBundleProjectDeps` 邻位；当前无 `[native.dependencies]` 声明面 → 占位 + Deferred 注释，见 D9 Deferred）
- [ ] N.5 `docs/book/src/runtime/native-libraries.md`：native 解析机制页（stdlib eager vs 非 stdlib 平铺 beside-zpkg + 发布期拍平）+ 挂 SUMMARY
- [ ] N.6 GREEN：`cargo build --release`（runtime）+ `xtask test`（stage-components 断言 + 无 repl WARN）
- [ ] N.7 **关掉 PR #332**（本 change supersede；说明「归并进 add-path-dependencies native 半边」）

## 阶段 3：文档同步（并入各 PR）
- [ ] 3.1 触发矩阵：对外行为（新 manifest 语法）→ book 机制页 + README 功能索引
- [ ] 3.2 死链核对：`grep -rn "_buildReplLib" docs/ scripts/ .claude/` 阶段 2 后清零

## 备注
- 无 zbc/zpkg 格式 bump（path 纯源码/构建期）。
- 两阶段严格分离原因见 design D6（自举轴②）。阶段 2 不得早于阶段 1 nightly。
