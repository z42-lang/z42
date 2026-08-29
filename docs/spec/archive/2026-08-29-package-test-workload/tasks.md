# Tasks: test workload 打包发布 + workload 描述泛化

> 状态：🟢 已完成 | 创建：2026-08-29 | 完成：2026-08-29

## 进度概览
- [x] 阶段 1: payload-only 打包（build + manifest）
- [x] 阶段 2: release-index test 条目
- [x] 阶段 3: launcher 描述泛化
- [x] 阶段 4: CI publish
- [x] 阶段 5: 测试与文档

## 阶段 1: payload-only 打包
- [x] 1.1 新建 `scripts/package/xtask_package_test.z42`：`_buildTestWorkload(root, version, profile)`——复用 `_ensureTestAgent` 编 agent → `z42.testagent.zpkg` 落 `z42-workload-<v>-test/`
- [x] 1.2 复用 `_workloadPkgHeader` 写头 + 追加 `[contents.payload] payload="z42.testagent.zpkg", host=["*"]`（在 `xtask_package_test.z42` 内）
- [x] 1.3 ~~`scripts/packages.toml` 注册~~ **剔除**：`_buildTestWorkload` 内联写 manifest、不消费 packages.toml；`_testPackagesConfig` 断言 package count==3，加第 4 包会破坏自检（见 proposal 收窄注）
- [x] 1.4 `scripts/xtask_cli.z42:_dispatchPackage`：`package workload test <version>` 特例（保留 desktop label-merge / build）
- [x] 1.5 本地验 ✅：`xtask package workload test nightly` → `z42-workload-nightly-test/`（agent zpkg + manifest.toml，`[contents.payload]` 段正确）

## 阶段 2: release-index test 条目
- [x] 2.1 `xtask_release.z42:_releaseGenIndex`：workload 名单 `wls` 加 `test`（size 5）；SHA256 校验循环含 `z42-workload-<label>-test.tar.gz`；`_jWorkload("test", ["*"], [])`
- [~] 2.2 index 产出本地不可全验（需 SHA256SUMS + 完整归档）→ 交 CI

## 阶段 3: launcher 描述泛化
- [x] 3.1 `launcher_cli.z42`：`:250/:304/:310/:312/:319` 文案泛化「tooling 或 capability（test）」；`:313` positional 加 `test`
- [x] 3.2 `launcher_workload.z42:3` header 泛化
- [x] 3.3 本地验 ✅：`z42 workload install --help` → 「a capability like test」+ `<ios|android|wasm|desktop|test>`；`workload --help` 泛化

## 阶段 4: CI publish（本地不可全验 → 交 CI）
- [x] 4.1 `.github/workflows/release.yml`：macos-arm64 单 host 跑 `package workload test` + archive `z42-workload-<v>-test.tar.gz`；index/upload glob `z42-*` 已覆盖
- [x] 4.2 `.github/workflows/ci.yml` publish-nightly：macos-arm64 建 + 单归档 `z42-workload-nightly-test.tar.gz`（循环后一次，无 merge）+ release notes Workloads 表加 test 行

## 阶段 5: 测试与文档
- [~] 5.1 index test 条目解析单测——`_releaseGenIndex` 需完整 SHA256SUMS/归档才能跑，本地不可单验 → CI 验（release/nightly）；打包布局由 5.4 本地验覆盖
- [x] 5.2 `xtask test` 完整 GREEN gate 全绿（含 compiler 自举 3/3 gen1==gen2；rebase 到 ②b 后重跑复验）
- [x] 5.3 spec scenarios 逐条覆盖确认（本地可验：打包布局 + manifest + launcher help；index/publish 链交 CI）
- [x] 5.4 文档同步：`workload/test/README.md`（打包发布 + install 描述）；`docs/book/src/dev/packaging.md`（payload-only workload + `[contents.payload]` 机制）
- [x] 5.5 命令面 grep：`platform workload` 泛化（scripts/ + launcher/ 清零，剩 launcher_workload:3 已含「or a capability」）
- [x] 5.6 roadmap：rebase 到 ②b 后 flip `package-test-workload` 行为 ✅ + 登记归档名

## 备注
- D6=复用 workload-tooling + 新 [contents.payload]（User 裁决）；D6a=键名 `payload`；D6b=无 merge 步；D6c=描述措辞。见 design.md。
- 命名空间 `Z42.TestHost.Agent` 改名 = 独立 follow-up，本 change 不动。
- 完整 release/publish 链本地不可全验 → 以 CI 为准（见 bootstrap-seed.md「cold 路径 GREEN 以 CI 为准」）。
