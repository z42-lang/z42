# Tasks: `[native]` 预编译库配置与复制

> 状态：🟢 归档（rebase origin/main `1a288ba3` 后本地 GREEN 全绿 + 核心逻辑 raw-z42b 实证 + dogfood 无回归；e2e leg CI-gated）。User 6.5 裁决：Q1=含 `<rid>/` 子目录；Q2=parse单测+合成e2e；Q3=单 PR 已由证据定。
> GREEN 标准：`xtask test` 全 stage 绿（e2e + cross-zpkg + stdlib 含 native parse 单测 + z42c 自举
> gen1==gen2 3/3 + vscode-syntax）；预编译 e2e 走本地 `xtask package sdk && xtask test dist` 或 CI。

## 阶段 0: 前置确认
- [x] 0.1 grep 确认 seed 编的 **xtask/z42c 源**（scripts/ + src/compiler/）不引用 `NativeSpec`/`.Natives`/`.Dir` → **单 PR 安全**（Q3 已由证据裁决）
- [x] 0.2 User 6.5 裁决：Q1=含 `<rid>/` 子目录；Q2=parse单测+合成e2e

## 阶段 1: 模型 + 解析（z42.project）✅
- [x] 1.1 `NativeSpec.z42` 加 `string Dir`（构造函数改 `(name, dir)`）
- [x] 1.2 `ManifestLoader._parseNative` 读每张 `[native.<name>]` 子表 `dir` 键 → 填 `NativeSpec.Dir`（缺省 ""）
- [x] 1.3 `manifest_native.z42` 加断言：`dir="prebuilt"` → `Dir=="prebuilt"`；空表 → `Dir==""`（AC1）→ **测过：4 files passed**

## 阶段 2: 消费侧预编译分支（builder_publish.z42）✅
- [x] 2.1 `_pubBundleProjectNativeDeps` 改分流：有 native → hooks 有无分别走 `_pubRunDepProvideNative` / `_pubCopyPrebuiltNative`（Decision 3）
- [x] 2.2 NEW `_pubNativeFileName(name, rid)` —— rid 派生 `<prefix><name><suffix>`（镜像 `_familyOfRid`，Decision 2）
- [x] 2.3 NEW `_pubCopyPrebuiltNative` —— 稳定名序遍历 `[native.*]`，读 `dir`，定位 `<depDir>/<dir>/<rid>/<派生名>` → 平铺进 distDir；缺 dir/缺文件 warn-skip（R2.2/R2.4）
- [x] 2.4 **（Decision 5，User 批准的扩展）** `_pubBundleProjectNativeDeps` 按 `{ path }` 解析 path-dep（去 srcRoot bail）+ NEW `_pubResolveDepToml`/`_pubDepPath`。**dogfood 无回归**（repl `libz42_repl.dylib` 仍 colocate）+ **手验通过**（仓外 tempdir path-dep → payload 含 `libpnfoo.dylib`）

## 阶段 3: e2e ✅（逻辑手验通过，待 test dist 全跑）
- [x] 3.1 `xtask_test_dist.z42` `_apphostSmoke` 加 prebuilt-native 一腿：合成 lib（`[native.pnfoo] dir="prebuilt"` + 假 `prebuilt/<hostRid>/<派生名>`）+ path-dep exe → `z42 publish` → 断言 payload 有 `<派生名>`（AC3）。**已加 `apphost = true`**（gate 要求）
- [x] 3.2 rid 派生（三 rid 族）：e2e 内 host rid 覆盖；跨 rid 派生由 `_pubNativeFileName` code + 手验 macos-arm64→libpnfoo.dylib 覆盖

## 阶段 4: 文档同步
- [ ] 4.1 `native-libraries.md §3`：把「无 hook 的 committed 预编译库消费路径」从 Deferred 移为实况；补 `dir` schema + rid 派生 + 布局示例
- [ ] 4.2 `z42.project/README.md`：NativeSpec 增 `Dir` 字段（若功能索引提及 NativeSpec）
- [ ] 4.3 更新本 change design 的 Deferred → 只剩 explicit-files（B）与 cross-produce

## 阶段 5: GREEN + 落地
- [x] 5.1 改 scripts/*.z42 后重建 xtask.zpkg ✓
- [x] 5.2 `xtask test` 全 stage **GREEN**（e2e 275 + cross-zpkg + stdlib + z42c 自举 gen1==gen2 3/3 + vscode-syntax）✓
- [x] 5.3 `xtask test stdlib z42.project`（native parse 单测：4 files passed，含新 `dir` 断言）✓
- [~] 5.4 预编译 e2e：**本地 `test dist` 无法验**——本地 SDK 包缺 desktop workload apphost → `z42 publish`（launcher）在 native bundling **之前**就于 apphost-stub 解析处中止（pre-existing [[apphost-publish-needs-desktop-workload]]；且本地包 golden 273 全 compile-fail 是 z42c CLI mismatch `--emit zbc -o` 系统性 pre-existing，与本 change 无关）。**核心逻辑已本地实证**=**raw z42b publish**（Z42_APPHOST_TEMPLATE 绕过 workload）→ payload 含 `libpnfoo.dylib`（+ 手验 rid 派生 macos-arm64→libpnfoo.dylib）。e2e leg 交 **CI**（有 workload apphost）验；rid 提取已修（`z42-<version>-<rid>-release`→`macos-arm64`，robust os-prefix search）
- [ ] 5.5 rebase origin/main（已推进：fix-struct-property-getter 已合）+ 重跑 GREEN → 开 PR（body 三段 + 页脚）→ auto-merge → 归档随 PR 同一逻辑单元
- [ ] 5.6 合并后清理 worktree `../z42-precompnative` + 本地/远程分支

## 验证证据（landing 依据）
- **本地 GREEN**：`xtask test` 全绿（authoritative 本地 gate）。
- **核心逻辑实证**（raw z42b，仓外 tempdir path-dep）：`z42b publish --rid macos-arm64` → `payload/libpnfoo.dylib` + `pnapp.zpkg` + `pnlib.zpkg`。覆盖 Decision 5（path-dep 按 `{path}` 解析、srcRoot="" out-of-repo）+ `_pubCopyPrebuiltNative` + `_pubNativeFileName`。
- **dogfood 无回归**：`xtask build toolchain` → `programs/z42i/libz42_repl.dylib` 仍 colocate（Decision 5 对 hook 路径无影响）。
- **CI-gated**：e2e leg（`_apphostSmoke` prebuilt-native 一腿）+ 打包/冷路径按 bootstrap-seed 规则以 CI 为准。

## 备注
- **3.1 e2e 无真实预编译消费者** → 合成 fixture（写假 native）。本地 launcher-publish 受 desktop-workload-apphost 限制不可跑，故核心靠 raw z42b 实证 + CI。
- **单 PR**：0.1 grep 已确认 seed 编的 xtask/z42c 源不引用 `NativeSpec`/`.Natives`（无新语法/格式）。
