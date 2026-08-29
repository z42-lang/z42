# Tasks: 本地路径依赖（path 依赖）+ native 依赖

> 状态：🟡 DRAFT（待 User 确认后进实施） | 创建：2026-08-29
> 跨两个 nightly：阶段 1（support）先合并发 nightly → 阶段 2（use）方可开工。
> **native 依赖（User 2026-08-29 并入本 change，Supersedes #332）**：无两-nightly 约束，随 PR-2 或独立 PR-3。

## 进度概览
- [x] 阶段 1（PR-1，support）：z42.project 加 `DepEntry.Path` + 解析（PR #335 已合并 `04fd1999`）
- [ ] ⏸ 等阶段 1 nightly 发布（#335 晚今日 nightly 38 秒未赶上 → 待下个 nightly；PR-2 前置）
- [ ] 阶段 2（PR-2，use）：z42c path 闭包构建 + 真-stdlib 打包判据（colocated）+ 切 toml + 删 xtask 特殊处理
- [x] native 依赖（独立 PR-3）：runtime 通用 resolver + repl 接入（搬 #332）+ z42b publish 复制骨架（本 PR）
- [ ] （独立 follow-up）single-file 打包合并——需 runtime 内嵌 bundle，另开 change

## 阶段 1（PR-1，support）——独立分支/worktree（纯 z42.project，z42c 不消费）
- [x] 1.1 `DepEntry.z42`：加 `public string Path;` + 3 参构造函数（design D1）
- [x] 1.2 `ManifestLoader._parseDeps`：表形式读 `path`（`dv.ContainsKey("path")`），构造点传第三参；纯字符串/省 path → `""`
- [x] 1.3 单测：path 依赖解析（表/version+path/省 version/纯字符串回落）—— `tests/manifest_path_dep.z42`（5 [Test]）
- [x] 1.4 `z42.project/README.md`：`DepEntry` 行补 path 语义
- [x] 1.5 book manifest 页：`[dependencies]` path 语法（落 `compiler/project-model.md`，非占位 manifest.md；support 阶段先记语法，use 机制留阶段 2）
- [x] 1.6 GREEN：`xtask test` 全 stage 通过（e2e 275/275 + cross-zpkg + stdlib 含 5 path-dep 测试 + z42c 自举 gen1==gen2 不动点 3/3 + vscode-syntax）；z42c 源未读 DepEntry.Path（`.Path` 命中仅 DepScanCache 的文件系统路径缓存，非 dep）
- [ ] 1.7 归档 PR-1 + 合并 → **等 nightly**

## 阶段 2（PR-2，use）——独立分支/worktree，PR-1 nightly 发布后
- [x] 2.1 `PathDepPlan.z42`（新）：沿 `DepEntry.Path` post-order DFS（环检测 `visiting` + 规范化路径去重 `visited`）→ 叶子在前的 `PathDepClosure`
- [x] 2.2 dist 解析辅助：复用 driver 私有 `_resolveDistDir`（BuildConfig 模板上下文在 driver，故闭包序在 pipeline、dist 解析留 driver）
- [x] 2.3 `Main.z42 _build`：top-level（libsDirsCount==0）前置 path 闭包构建（逐成员 `_build`，post-order 累积 dist 作 override）+ 消费方 libsDirs 追加闭包全 dist + Z42_LIBS
- [x] 2.4 `_bundleExeDeps`：加 `projectDir` 参 + 真-stdlib 判据（`<srcRoot>/libraries/<name>`，`_srcRoot` 助手；repo 外回落旧前缀判据），path 依赖私有复制。与 `_pubBundleProjectDeps` 一致（D2）
- [x] 2.5 端到端 e2e（`xtask_compiler_e2e._e2eBuildChecks` 新增 path_dep 检查）：lib foo + exe bar（`{ path="../foo" }`）→ 自动建 foo 闭包 + bar 解析 foo 符号 + foo.zpkg colocate 进 bar dist + bar.zpkg 直跑（`--release`）
  - **关键发现（packed 前提）**：运行期惰性加载器只把 **packed** zpkg 当依赖候选，**indexed**（debug 布局）不作候选 → debug 建的 path 依赖 colocate 后运行期 `undefined function`。故 e2e 与真实部署（z42i→z42.repl）均走 `--release`（消费方+闭包 packed）。既有惰性加载器约束，非 path 依赖新引入；已记入 book `project-model.md` 路径依赖闭包「packed 前提」。
- [x] 2.6 `z42.interactive.z42.toml`：`"z42.repl"` → `{ path = "../repl" }`
- [x] 2.7 xtask：删 `_buildReplLib` **两处调用 + 函数定义**（`xtask_toolchain._ensureToolchainDeps` + `xtask_package_desktop._pkgStageToolchainComponents`）+ 更新注释
- [~] 2.7b/2.8b（验证）删 `_buildReplLib` 后 z42.repl 走 path 闭包 colocate 进 `programs/z42i/`：`_pubBundleProjectDeps` 按名定位不到 z42.repl（不在 libraries/）→ 保守跳过，交由 `_bundleExeDeps`（build 时）+ `_pubCopyDistDeps`（publish 时）搬进 payload。**`xtask build toolchain` 手验中**（GREEN 后）
- [x] 2.8 验证路径：`z42c build z42.interactive`（CLI，libsDirsCount==0）→ 闭包建 z42.repl → `_bundleExeDeps` colocate 进 interactive dist → `_z42bPublish._pubCopyDistDeps` 带进 `programs/z42i/`。**`xtask build toolchain` 手验中**
- [x] 2.9 README（z42c.driver / z42c.pipeline）+ book `compiler/project-model.md` 路径依赖闭包机制 + 交叉链 native-libraries.md
- [~] 2.10 GREEN：`xtask test`（全 stage，跑中）+ `xtask build toolchain` 手验 z42i + `xtask test bootstrap`（未越界）
- [x] 2.11 归档 + 合并：native + path-use 合成 PR #337（squash `57ab2ed9`），CI 必需 check 全绿（唯一红=非必需 bench-regression，其失败发生在 base-tree baseline 捕获步、与本改动无关），User 合并；分支/worktree 已清；本 spec 归档。

## native 依赖（PR-2 内或独立 PR-3；无两-nightly 约束；Supersedes #332，见 D9/D10）
> #332 的 5 文件 diff 已实现+验证——**直接搬运**，勿重做。worktree ../z42-replisolate 有原始 diff。
- [x] N.1 搬 #332 packaging 4 文件 diff：`xtask_stage_components._pkgStageReplCdylib`（libz42_repl→programs/z42i/）+ `xtask_package_desktop` 调用 + `xtask_package._copyNativeLibs` 注释 + `xtask_test_stage_components` 断言
- [x] N.2 `native/ext.rs`：抽 `resolve_native_beside(zpkg_dir, lib_name) → Option<PathBuf>`（平铺，按名，用 `DLL_PREFIX`/`DLL_SUFFIX` 反向拼 `lib<name>.<suffix>`，单一 stat）
- [x] N.3 `repl_native.rs`：搬 #332 的 candidates() diff；`<sdk>/programs/z42i/` 那段派生+查找改为调用 N.2 的共享 `resolve_native_beside`
- [x] N.4 `builder_publish.z42`：`_pubBundleProjectNativeDeps` 骨架（挂 `_pubBundleProjectDeps` 邻位，live-wired no-op；当前无 `[native.dependencies]` 声明面 → Deferred 注释）
- [x] N.5 `docs/book/src/runtime/native-libraries.md`：native 布局/解析页（stdlib eager vs 组件私有平铺 beside-zpkg + 发布期拍平）+ 挂 SUMMARY。**doc-structure 裁决（User 2026-08-29）**：新建独立页 + 把已存在的 `native-extensions.md`（其 §2.4/§2.6/表格/§3 因 repl bin/→programs/z42i/ 变旧）就地修订并交叉链到新页——两页职责：extensions=cdylib C-ABI 机制，libraries=库住哪/怎么找。
- [x] N.6 GREEN：`cargo build --release`（runtime）+ `xtask test`（stage-components 断言 + 无 repl WARN）
- [~] N.7 **关掉 PR #332**：#332 已于 2026-08-29 05:15 CLOSED（未合并）——无需再关，本 PR body 说明「归并进 add-path-dependencies native 半边」。

## 阶段 3：文档同步（并入各 PR）
- [ ] 3.1 触发矩阵：对外行为（新 manifest 语法）→ book 机制页 + README 功能索引
- [ ] 3.2 死链核对：`grep -rn "_buildReplLib" docs/ scripts/ .claude/` 阶段 2 后清零

## 备注
- 无 zbc/zpkg 格式 bump（path 纯源码/构建期）。
- 两阶段严格分离原因见 design D6（自举轴②）。阶段 2 不得早于阶段 1 nightly。
