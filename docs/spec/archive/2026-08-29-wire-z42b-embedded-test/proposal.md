# Proposal: z42b 接管 embedded 测试的单-bundle 执行与设备组装

## Why

统一测试流水线（unify-test-pipeline-z42b）的两层模型：**z42b = 单目标/单-bundle 执行器**，
**xtask = 语料/fleet 编排器**，二者以 **bundle manifest** 为缝。阶段②a（compile-then-test）已让
z42b 能「编一个工程→跑 [Test]」。阶段②b 要把「跑一个已组装好的 test-bundle」以及「为某 RID 组装
可部署语料」这两件**单目标执行**的事从 xtask 搬进 z42b，使 z42b 成为「运行一个 bundle」的单一 owner，
xtask 只保留语料发现/编译/分片/聚合（真正的 fleet 编排）。

现状（origin/main 实测校正）：
- `z42b test`/`bench` 的 `--rid` option **已注册**但被 `_runModule` 完全忽略（[builder_cli.z42:80,88](../../../../src/toolchain/builder/core/builder_cli.z42) 标「platform deploy pending」；[builder_test.z42:32-53](../../../../src/toolchain/builder/core/builder_test.z42) 不读 rid）。
- embedded 测试的 bundle 运行 + 设备语料组装**全在 xtask** 的 `_testEmbedded`（[xtask_test_embedded.z42:827-903](../../../../scripts/test/xtask_test_embedded.z42)）：host 分支自己 spawn `desktop testhost + agent` 跑 manifest（`:891-902`）；wasm/ios/android 分支 `_build{Wasm,Ios,Android}Testhost` 组装 `{app,libs,bundle}` deployable。
- agent 的 bundle 运行逻辑 `_runBundleReport`（[agent.z42:74-147](../../../../src/toolchain/workload/test/agent/src/agent.z42)：golden 隔离 VM + unit 共享 VM）本质是 `z42.test` 库能力，却只在 agent 里，host 侧 z42b 无法复用。

## What Changes

**Slice 1（host，本地可验）——单-bundle 执行归位 z42b：**
- 把 agent 的 `_runBundleReport` 提取为 `z42.test` 共享函数 `BundleRunner.RunBundle(manifestPath, libsDir, format, outPath)`（golden 隔离 + unit 共享，产 JSON/pretty 报告）；**agent 与 z42b 共用**（设计完整性：一份 bundle-runner，两个调用方）。
- `z42b test --rid host <bundle-manifest.json>`：z42b 直接 in-process 调 `BundleRunner.RunBundle` 跑整个 bundle（不再 spawn testhost+agent 进程）。`_runModule` 识别 `.json` bundle target。
- xtask `_testEmbedded` 的 desktop 分支（`:869-902`）**委托** `z42b test --rid host <manifest>`，删掉 `_ensureDesktopTesthost` spawn 内联逻辑。

**Slice 2（device，CI-only 验）——设备语料组装委托 z42b：**
- `z42b test --rid <device> <bundle>`：为该 RID 组装可部署 `{app,libs,bundle}`（现 `_assembleEmbeddedCorpus` 逻辑）+ wasm 的 `files.json`，输出到 deployable dir。
- xtask `_build{Wasm,Ios,Android}Testhost` 的**语料组装步**委托 z42b；**native 平台构建**（wasm-pack / xcframework / cargo-ndk）与**设备 RUN 交接**（Playwright / xcodebuild-sim / gradle）**保持在 xtask**（native build 属 z42b build/export 边界、设备 RUN 是外部触发，皆非 bundle-executor 职责——见 design D2）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42.test/src/BundleRunner.z42` | NEW | 提取共享 bundle-runner（golden 隔离 + unit 共享 → 报告） |
| `src/libraries/z42.test/src/Runner.z42` | MODIFY | 若 `RunBundle` 复用其 `RunModuleResults`（只读引用即可则不改；实际需要则登记） |
| `src/toolchain/workload/test/agent/src/agent.z42` | MODIFY | `_runBundleReport` 改为调 `BundleRunner.RunBundle` |
| `src/toolchain/builder/core/builder_test.z42` | MODIFY | `_runModule` 读 `--rid`：host+`.json`→`RunBundle`；device→组装 deployable |
| `src/toolchain/builder/core/builder_cli.z42` | MODIFY | `test`/`bench` 的 `--rid` 帮助文案去「platform deploy pending」；target 增 `.json`/deployable 语义说明 |
| `scripts/test/xtask_test_embedded.z42` | MODIFY | desktop 分支委托 `z42b test --rid host`；`_build*Testhost` 组装步委托 z42b |
| `src/libraries/z42.test/tests/bundle-runner/` | NEW | `BundleRunner.RunBundle` 单测（golden+unit 混合 manifest） |
| `docs/book/src/toolchain/*.md`（testing/two-layer 页） | MODIFY | 两层模型「z42b 拥有单-bundle 执行 + 设备组装」机制落地 |
| `src/toolchain/workload/test/README.md` | MODIFY | 登记 ②b 归位；agent 与 z42b 共用 BundleRunner |
| `docs/roadmap.md` | MODIFY | 阶段②b Slice 1/2 进度 |

**只读引用：**
- `src/libraries/z42.test/src/ModuleLoader.z42` — `RunGoldensIsolated` 签名（bundle-runner 复用）
- `src/libraries/z42.test/src/TestReport.z42` — 报告 JSON 形态
- `scripts/xtask_cli.z42` — `test embedded` 注册（理解 dispatch，不改）

## Out of Scope
- 设备端**实际 RUN**编排（wasm Playwright / ios xcodebuild-sim / android gradle）——仍 CI/外部触发，本 change 不接管（Slice 3，defer）。
- native 平台构建（wasm-pack / xcframework / cargo-ndk）搬迁——属 z42b build/export 边界，不在本 change。
- 老 `IPlatformBackend`（`xtask_test_{wasm,ios,android}.z42` 的原生 R1–R7 嵌入契约测试）——与 testagent 语料路径无关，不动。

## Open Questions
- [ ] design D1：`RunBundle` 放 `z42.test` 新文件 `BundleRunner.z42` 还是并入 `Runner.z42`？（倾向新文件，职责分离）
- [ ] design D2：Slice 2 委托边界 = 仅「语料组装」还是「组装+native build」一起搬？（倾向仅组装，native build 留 xtask）
