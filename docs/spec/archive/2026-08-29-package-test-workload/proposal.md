# Proposal: test workload 打包发布 + workload 描述泛化

## Why

阶段①已把 test-agent 归位为能力 workload（`src/toolchain/workload/test/agent/`，产
`z42.testagent.zpkg`）。但**当前无 payload-only workload 打包路径**：所有 4 个现有 workload
（desktop/ios/android/wasm）都是「tooling」形态（apphost stubs / SwiftPM / gradle / npm facade），
test workload 是**纯 payload**（只一份平台无关 `z42.testagent.zpkg`，无 per-RID apphost、无 runtime
pack），现有打包/发布/index 机制都不覆盖它。因此 `z42 workload install test` 语法虽已被 manifest 驱动
的 CLI 天然支持（archived D6），却**没有可下载的产物**——广告 test workload 前提是先让它可发布。

同时，launcher 的 install/list 文案与 header 断言「workload = a platform's tooling」，把 workload
写死为「平台工具」，不容纳 test 这类**平台无关能力** workload——诚实列出 test 前须泛化描述。

## What Changes

**① payload-only 打包发布（release-infra，D6）：**
- 新增 payload-only workload 打包模式：产 `z42-workload-<label>-test.tar.gz`，内含
  `z42.testagent.zpkg` + 一份 workload manifest（`kind="workload-tooling"`、`host=["*"]`、
  `runtime-pack=""`、新 `[contents.payload]` 段描述单 zpkg —— 见 design D6）。
- `package index`（`_releaseGenIndex`）的 workload 名单加 `test` 条目（`host=["*"]`、`runtimes=[]`）。
- CI publish（`release.yml` + `ci.yml` publish-nightly）纳入 test workload 的 build/archive/index。

**② workload 描述泛化（依赖①）：**
- `launcher_cli.z42` 的 install/list/uninstall 文案 + positional 值域（加 `test`）：把「platform
  workload」泛化为「platform tooling 或 capability（如 test）」。
- `launcher_workload.z42:3` header「A workload = a platform's tooling」泛化。
- CLI 本身已 manifest 驱动、支持任意 workload 名，故描述泛化技术上独立于①，但**广告 test 前提是①让它
  可下载**——两部分同一 change/PR 一起落。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `scripts/package/xtask_package_test.z42` | NEW | `_buildTestWorkload(root, version, profile)`：编 `z42.testagent.zpkg` + 写 payload manifest → workload dir |
| `scripts/package/xtask_package.z42` | MODIFY | `_workloadPkgHeader` 复用；新增 payload manifest writer（`[contents.payload]`，zpkg 键） |
| `scripts/package/xtask_release.z42` | MODIFY | `_releaseGenIndex` workload 名单 + SHA 校验加 `test` 条目（无 merge：payload-only 无 per-RID，一步 build 即终产物） |
| `scripts/xtask_cli.z42` | MODIFY | `_dispatchPackage` 的 `package workload test <version>` 识别（保留 desktop label-merge / build） |

> **实施期收窄（2026-08-29）**：原 Scope 列 `scripts/packages.toml`（注册 `[package.workload-test]`）——
> **已剔除**。`_buildTestWorkload` 内联写 manifest、不消费 packages.toml；CI 经显式 `package workload test`
> 命令构建；且 `_testPackagesConfig` 硬断言 `package count == 3`，加第 4 个包会破坏该自检。故 packages.toml
> 注册无功能必要且有害，跳过。
| `src/toolchain/launcher/core/launcher_cli.z42` | MODIFY | install/list/uninstall 文案 + positional 值域泛化（含 `test`） |
| `src/toolchain/launcher/core/launcher_workload.z42` | MODIFY | header:3 泛化「tooling 或 capability」 |
| `src/toolchain/launcher/core/launcher.z42` | MODIFY | `--workloads` list 注释泛化（grep 核对发现的同模块一致性修正） |
| `.github/workflows/release.yml` | MODIFY | package/archive test workload + index 纳入 |
| `.github/workflows/ci.yml` | MODIFY | publish-nightly 纳入 test workload（build/archive/index/release notes 表） |
| `src/toolchain/workload/test/README.md` | MODIFY | 登记打包发布落地 + install 描述 |
| `docs/book/src/toolchain/*.md`（packaging/release 页） | MODIFY | payload-only workload 打包机制 + `[contents.payload]` |
| `docs/roadmap.md` | MODIFY | Change C 进度登记 |
| `scripts/package/tests/`（如有 parse 测试目录） | NEW | payload manifest / index test 条目解析单测 |

**只读引用：**
- `scripts/package/xtask_package_desktop.z42` — `_buildDesktopWorkload` 模板（RID 无关、runtime-pack=""）
- `src/toolchain/workload/test/agent/z42.testagent.z42.toml` — agent 工程（打包源）
- `src/toolchain/launcher/core/launcher_network.z42` — `_fetchWorkloadEntry` / `_hostAllowed`（install 消费 index 字段，不改）
- `scripts/test/xtask_test_embedded.z42:18-30` — `_ensureTestAgent`（了解 agent 现构建方式）

## Out of Scope
- test workload 的 runtime pack / bedding —— test workload 无 runtime 依赖（`runtimes=[]`），install 的 bedding 循环天然跳过，不涉及。
- 命名空间 `Z42.TestHost.Agent` 改名（单列 follow-up，本 change 不动）。
- 阶段②b（z42b 接管设备端）——独立 change（wire-z42b-embedded-test）。

## Open Questions
- [ ] design D6a：payload manifest 的 `[contents.payload]` 具体键名（`payload = "z42.testagent.zpkg"`？还是 `zpkg = ...`）——见 design。
- [ ] design D6b：payload-only 是否需要 `_releaseAssembleDesktop` 那样的 merge 步，还是 per-RID build 直接就是最终 tar（test 无 per-RID → 直接 build 即最终产物，无 merge）。
