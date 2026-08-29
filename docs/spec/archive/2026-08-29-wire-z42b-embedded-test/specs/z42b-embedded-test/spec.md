# Spec: z42b embedded 测试执行

## ADDED Requirements

### Requirement: BundleRunner 共享执行核

`z42.test` 提供 `BundleRunner.RunBundle(manifestPath, libsDir, format, outPath)`，把一个 test-bundle
manifest（`{cases:[…]}`，golden case 含 `entry` 键、unit case 无）聚合运行为一份报告。agent 与 z42b
共用同一实现。

#### Scenario: 混合 manifest 聚合
- **WHEN** manifest 含 1 个 golden case（`{name,zbc,entry,expected}`）+ 1 个 unit case（`{name,zbc}`）
- **THEN** golden 经 `RunGoldensIsolated` 独立 VM 跑并比对 stdout；unit 经 `RunModuleResults` 共享 VM 跑；报告含两 case 的 pass/fail，退出码 = 有任一失败则 1、全过 0

#### Scenario: out-path 写文件
- **WHEN** `outPath` 非空
- **THEN** JSON 报告写入该文件（供设备端无进程 stdout 时回读）；`outPath` 空则写 stdout

#### Scenario: libs 解析回退
- **WHEN** `libsDir` 为空
- **THEN** 依次尝试 `Z42_LIBS` 环境变量、`<bundle>/../libs`（与原 agent 语义一致）

### Requirement: z42b test --rid host 跑 bundle

`z42b test <manifest.json> --rid host`（或省略 rid）in-process 调 `BundleRunner.RunBundle`，不 spawn
testhost/agent 进程。

#### Scenario: host bundle 运行
- **WHEN** `z42b test bundle/manifest.json`（rid 默认 host）
- **THEN** z42b 直接跑 bundle 全部 case，渲染报告到 stdout，退出码反映通过/失败

#### Scenario: 已编译单模块不变
- **WHEN** target 以 `.zbc`/`.zpkg` 结尾
- **THEN** 走 `Runner.RunModule`（现状不变），与 rid 无关

#### Scenario: 工程 toml host 不变
- **WHEN** target 为 `.z42.toml` 或空、rid=host
- **THEN** 走②a compile-then-test（现状不变）

### Requirement: z42b test --rid <device> 组装 deployable

`z42b test <manifest.json> --rid <device-rid>` 为该 RID 组装可部署 `{app,libs,bundle}`。

#### Scenario: 设备语料组装
- **WHEN** `z42b test bundle/manifest.json --rid android-x64`（agent zpkg + flat libs 就位）
- **THEN** 输出 deployable dir 含 `app/z42.testagent.zpkg` + `libs/*.zpkg` + `bundle/`（manifest+case）；wasm RID 额外产 `files.json`

#### Scenario: 未知 rid 报错
- **WHEN** `--rid` 值不在 {host,browser-wasm,ios-arm64,iossim-arm64,android-arm64,android-x64}
- **THEN** 报错列出合法值，退出码非 0

#### Scenario: 设备端 RUN 不在本 change
- **WHEN** device 组装完成
- **THEN** z42b 仅产 deployable 并（如需要）打印 RUN 交接提示；实际 RUN 由 xtask/CI 外部驱动（Slice 3 defer）

## MODIFIED Requirements

### Requirement: xtask embedded 测试委托 z42b

**Before:** `xtask test embedded`（`_testEmbedded`）的 desktop 分支自行 spawn desktop testhost + agent
跑 manifest；`_build{Wasm,Ios,Android}Testhost` 自行 `_assembleEmbeddedCorpus` 组装。

**After:** desktop 分支委托 `z42b test --rid host <manifest>`；`_build*Testhost` 的语料组装步委托
`z42b test --rid <rid> <manifest>`（native build + RUN 交接仍在 xtask）。逐 case 报告与改前一致。

## Pipeline Steps
（本 change 属 toolchain/stdlib，无 lexer/parser/typechecker/IR 变更）
- [ ] z42.test（BundleRunner 提取）
- [ ] agent（改调共享核）
- [ ] z42b builder（`_runModule` rid 分派 + device 组装）
- [ ] xtask（委托接线）
- [ ] 测试（bundle-runner 单测 + embedded 回归）
