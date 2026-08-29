# Spec: test workload 打包发布

## ADDED Requirements

### Requirement: payload-only workload 打包

新增 payload-only 打包模式，产 `z42-workload-<label>-test.tar.gz`（单 payload zpkg + manifest，
无 per-RID piece、无 runtime pack）。

#### Scenario: build test workload
- **WHEN** `xtask package workload test <version>`
- **THEN** 产出 `z42-workload-<version>-test/`，含 `z42.testagent.zpkg` + `manifest.toml`（`kind="workload-tooling"`、`host=["*"]`、`runtime-pack=""`、`[contents.payload] payload="z42.testagent.zpkg"`）

#### Scenario: 无 merge 步
- **WHEN** test workload 打包（无 per-RID piece）
- **THEN** `_buildTestWorkload` 一步产出的 dir 直接 tar 即最终发布产物，不经 desktop 式 merge

### Requirement: release-index 含 test 条目

`package index` 生成的 `release-index.json` 的 `workloads` 含 `test`。

#### Scenario: index test 条目
- **WHEN** `xtask package index <label> <dist> …`（`z42-workload-<label>-test.tar.gz` + 其 sha 已在 SHA256SUMS）
- **THEN** `workloads.test = {archive, sha256, host:["*"], runtimes:[]}`

#### Scenario: 缺 archive 校验失败
- **WHEN** test workload archive 或其 sha 缺失
- **THEN** index 生成报错（沿用现有逐项 SHA256 校验），退出码非 0

### Requirement: z42 workload install test（现有 CLI，验证不破坏）

#### Scenario: install 走 runtimes=[] 路径
- **WHEN** `z42 workload install test`（index 有 test 条目）
- **THEN** 下载校验解压 tar 到 `runtimes/<v>/workloads/test/`；`runtimes=[]` → bedding 循环跳过（与 desktop 一致），install 成功

## MODIFIED Requirements

### Requirement: workload 描述泛化为「tooling 或 capability」

**Before:** launcher 文案与 header 把 workload 写死为「platform workload / a platform's tooling」，
positional 值域 `<ios|android|wasm|desktop>`。

**After:** 文案泛化为「platform tooling 或 capability（如 test）」；positional 值域含 `test`；
`launcher_workload.z42:3` header 泛化。CLI 派发逻辑不变（本就 manifest 驱动、认任意 workload 名）。

#### Scenario: install --help 列出 test
- **WHEN** `z42 workload install --help`
- **THEN** positional 值域含 `test`，描述提及 capability workload

## Pipeline Steps
（本 change 属 toolchain/release-infra，无 lexer/parser/typechecker/IR/VM 变更）
- [ ] 打包（xtask_package_test / xtask_package / packages.toml / xtask_cli）
- [ ] index（xtask_release）
- [ ] launcher 描述泛化
- [ ] CI publish（release.yml / ci.yml）
- [ ] 测试 + 文档
