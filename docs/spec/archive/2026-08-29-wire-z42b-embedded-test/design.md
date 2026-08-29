# Design: z42b 接管 embedded 测试的单-bundle 执行与设备组装

## Architecture

两层模型下的职责重划（缝 = bundle manifest + `{app,libs,bundle}` deployable）：

```
                      ┌─────────────── xtask（fleet 编排器）───────────────┐
  语料发现/编译/分片 → _buildTestBundle → manifest.json + {app,libs,bundle}
                      └──────────────────────┬──────────────────────────┘
                                             │  缝（bundle manifest / deployable dir）
                      ┌──────────────────────▼──────────────────────────┐
                      │            z42b（单-bundle 执行器）               │
  host:   z42b test --rid host  <manifest.json> → BundleRunner.RunBundle（in-process）
  device: z42b test --rid <dev> <bundle>        → 组装 {app,libs,bundle} deployable
                      └──────────────────────┬──────────────────────────┘
                                             │  device deployable
                      native build（wasm-pack/xcframework/cargo-ndk）+ RUN 交接
                      （Playwright / xcodebuild-sim / gradle）—— 仍在 xtask（Slice 3 defer）

  共享执行核（z42.test）：
    BundleRunner.RunBundle(manifest, libs, format, outPath)
        ├── golden case（有 entry） → ModuleLoader.RunGoldensIsolated（每个独立 VM）
        └── unit case               → Runner.RunModuleResults（共享 VM）
    调用方：① agent.z42（设备端嵌入 VM）  ② z42b test（host in-process）
```

## Decisions

### D1: BundleRunner 独立成 z42.test 新文件，且 **JSON-free**（吃 `BundleCase[]`，不吃 manifest 路径）

**问题：** agent 的 `_runBundleReport`（golden 隔离 + unit 共享 + 报告聚合）要被 z42b 复用，放哪、吃什么？
**关键约束（实测）：** `z42.test` 依赖仅 `z42.core`+`z42.io`；`TestReport` **刻意手写 JSON 以避免依赖
z42.json**（源码注释明示）。若 `BundleRunner` 直接解析 manifest 路径就要引入 z42.json → 破坏该设计意图。
**决定：** 新文件 `BundleRunner.z42`，签名 **`RunBundle(BundleCase[] cases, string bundleDir,
string libsDir) → TestResult[]`**（纯执行，无 JSON）。同文件定义 `BundleCase{Name,Zbc,Entry,Expected}`
（`Entry==null` = unit）。**manifest.json → BundleCase[] 的解析留在调用方**（agent 与 z42b 各自本就依赖
z42.json，各写 ~20 行薄 adapter）——共享**算法**（golden 隔离比对 + unit 跑 + 聚合），I/O glue 按 host
分。`BundleRunner` 只读复用 `Runner.RunModuleResults` + `ModuleLoader.RunGoldensIsolated`（不改二者签名）。
渲染（json/pretty）+ 退出码由调用方按各自 host 处理（agent 恒 JSON 写 stdout/文件；z42b 按 `--format`
渲染 + 返回退出码）。

### D2: Slice 2 委托边界 = 仅「语料组装」

**问题：** `_build{Wasm,Ios,Android}Testhost` 既组装 `{app,libs,bundle}` 又跑 native build
（wasm-pack/xcframework/cargo-ndk），委托 z42b 到哪一层？
**选项：** A — 仅「语料组装」（`_assembleEmbeddedCorpus` + wasm `files.json`）委托 z42b，native build
留 xtask；B — 组装 + native build 一起搬进 z42b。
**决定：** **A**。native 平台构建是 z42b `build --rid` / `export` 的边界（交叉编译 runtime cdylib /
xcframework），与「运行一个 bundle」的 executor 职责正交；且 native build 本地/CI 依赖重（NDK/Xcode/
wasm-pack）。Slice 2 只把「语料 deployable 组装」这一 executor 职责搬进 z42b，`_build*Testhost` 收缩为
「native build（留） + 调 z42b 组装 + 打印 RUN 交接（留）」。RUN 本身 = Slice 3（defer）。

**实现（User 2026-08-29 确认「仍实现 Slice 2」）**：CLI = `z42b test <manifest> --rid <device>
--out <dir> --agent <z42.testagent.zpkg>`（libs 走 `Z42_LIBS`）。z42b 不自找 agent/libs（部署工具不假设
repo 布局）→ xtask 经这三个参数把「bundle + 预建 agent + flat stdlib」交给 z42b，z42b 落
`{app,libs,bundle}`（wasm 加 files.json）。xtask 的 `_z42bStageDeployable` 是这层薄封装；
`_build{Wasm,Ios,Android}Testhost` 的 `_assembleEmbeddedCorpus` 调用点改调它。此边界的权衡（组装是
staging 非 execution，委托加了子进程 plumbing）已与 User 摊清，User 裁定仍按两层模型把组装归 z42b。

### D3: host bundle 运行 = z42b in-process，废除 desktop testhost+agent spawn

**问题：** 现 host 分支 spawn `_ensureDesktopTesthost`（Rust 嵌入 host 二进制）+ agent zpkg 跑 manifest。
z42b 接管后是继续 spawn，还是 in-process 跑？
**选项：** A — z42b 也 spawn testhost+agent（要 z42b 自己 ensure 那两个产物，跨程序重复 xtask 逻辑）；
B — z42b in-process 调 `BundleRunner.RunBundle`（z42b 本就跑在 z42vm 上，直接调 z42.test）。
**决定：** **B**。z42b 是 z42vm-hosted 程序，本就依赖 z42.test（`Runner`），in-process 调
`BundleRunner` 无需 testhost 二进制、无需 agent zpkg（agent 只在**设备端**嵌入 VM 时才需要——那里没有
z42b 可跑）。host 路径因此彻底去掉 testhost+agent 进程 spawn。golden 仍由 `RunGoldensIsolated` 起独立
VM（隔离语义不变）；unit 在 z42b 自身 VM 共享跑（与原 testhost VM 共享等价，正确性不变）。

### D4: `_runModule` 的 target × rid 分派

`z42b test [target] --rid <rid>`（`builder_test.z42:_runModule`）新分派表：

| target 后缀/形态 | rid | 行为 |
|-----------------|-----|------|
| `.zbc` / `.zpkg` | 任意 | 直跑单模块（现状不变，`Runner.RunModule`） |
| `.json`（bundle manifest） | host/空 | `BundleRunner.RunBundle`（in-process，Slice 1） |
| `.json` | device | assemble deployable for rid（Slice 2） |
| `.z42.toml`/空 | host/空 | compile-then-test（现状②a 不变） |
| `.z42.toml`/空 | device | （defer：需先编工程再组装，Slice 3 范畴）→ 报未实现 |

`--rid` 值域沿用 `test embedded`：host（默认）| browser-wasm | ios-arm64 | iossim-arm64 |
android-arm64 | android-x64。rid→category 用 z42b 侧既有 `_ridCategory`（若无则从打包路径复用）。

## Implementation Notes

- **BundleRunner 提取**：把 `agent.z42:_runBundleReport`（`:74-147`）整体搬进
  `z42.test/src/BundleRunner.z42` 的 `public static int RunBundle(string manifestPath, string libsDir,
  string format, string outPath)`；libs 解析（`Z42_LIBS` 或 `<bundle>/../libs`）、`Z42_TEST_JOBS`
  并行度、golden/unit 分流全部随迁。agent 的 `Main` golden/unit/bundle 三分支里，`.json` 分支改为
  `BundleRunner.RunBundle(target, "", format, outPath)`（保持 out-path 写文件语义供设备回读）。
- **z42b host 调用**：`_runModule` 识别 `target.EndsWith(".json")` 且 rid∈{"","host"} →
  `BundleRunner.RunBundle(target, "", format, "")`（out-path 空 = 写 stdout，host harness 抓 stdout）。
  需 `using Std.Test;` 已在 `builder_test.z42`。
- **z42b device 组装**：`_runModule` device 分支调新 `_assembleDeployable(bundleDir, rid, outDir)`——
  从 bundle manifest 所在目录 + agent zpkg + flat libs 组 `{app,libs,bundle}`。agent zpkg 与 flat libs
  的定位：z42b 从 `--libs`/`Z42_LIBS` 或 SDK 布局解析（复用 z42b build 既有 libs 解析）。**agent zpkg
  由 xtask 预建后经参数/约定传入**（z42b 不 rebuild agent——那是语料侧产物）。
- **xtask 委托**：`_testEmbedded` desktop 分支（`:891-902`）→ `z42b test --rid host <manifest>`
  （z42b 定位复用 `_hostZ42b()`/apphost 既有解析）。`_build*Testhost` 的 `_assembleEmbeddedCorpus` 调用
  → `z42b test --rid <rid> <manifest>`（组装），native build 段不动。
- **报告格式不变**：`TestReport.toJson`（`z42.test`）产出形态跨 host/device 一致。

## Testing Strategy

- **单元测试**：`z42.test/tests/bundle-runner/` — 构造含 1 golden + 1 unit 的 mini manifest + 对应
  .zbc/.expected，断言 `RunBundle` 聚合 report 的 pass/fail 计数与退出码。
- **回归（host 全链）**：`xtask test embedded`（desktop 默认路径）经委托后 z42b 跑，报告与改前逐 case
  一致（GREEN gate 内 `test embedded` 段）。
- **VM 验证**：`xtask test`（完整 GREEN gate；embedded/e2e/cross-zpkg/stdlib/compiler/vscode-syntax）。
- **device（Slice 2）**：本地不可全验 → `z42b test --rid android-x64 <bundle>` 只验「组装出正确
  `{app,libs,bundle}` 布局」（本地可验组装、不验 RUN）；实际设备 RUN 交 CI（wasm/ios/android job）。
- **self-host**：改 z42.test（stdlib）+ builder（toolchain）→ 跑 `xtask test compiler` 确认自举字节不动。
