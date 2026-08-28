# Tasks: cross-zpkg fixture 编译改用 release VM（消灭 test-host(linux-x64) pole）

> 状态：🟡 进行中 | 创建：2026-08-29 | 类型：perf（test 基础设施，无外部行为变更）

**变更说明：** cross-zpkg 测试 harness 把「编译 fixture（target→ext→main 三阶段 z42c 调用）」与
「运行 main.zpkg」拆成两个 VM——**编译走 release z42vm、运行走 debug z42vm**。此前两者共用 debug VM。

**原因：** `test-host(linux-x64)` 31–32min，其余腿只要 12–21min；差的 ~18min 全来自它独家跑的 cross-zpkg
stage。根因是该 stage 在 **debug VM** 上串行编译 ~40 个 fixture 包（13 fixture × 3 阶段），debug VM 编译慢
release 一个量级。铁证：同一 gate 在 `test-consume(linux-x64)` 用 release VM 只跑 **1.2min**（vs 18min，15×）。
debug VM 的价值（overflow-checks + 66 个内存布局 `debug_assert!`）是**执行期**触发的，编译走 debug 零覆盖
价值——故编译改 release、只保留运行走 debug，覆盖不丢而 pole 消失。预期 linux-x64 32→~16min，与其余腿齐平。

**文档影响：** `docs/book/src/dev/test-gate.md`（机制页：cross-zpkg 编译 VM 说明）；`.github/workflows/ci.yml`
cross-zpkg coverage 注释（描述性，顺带校正）。

## 任务

- [x] 1.1 `scripts/test/xtask_test_cross.z42` `_testCrossZpkgImpl`：`vmBin` 拆成 `compileVm`(release)/`runVm`(debug)；
      toolchain 路径两者同为 release（toolchain 只带 release VM）；release 缺失时 compileVm 回落 debug（本地兜底）
- [x] 1.2 `_runOneCrossCase` 签名 + body：3 阶段编译用 `compileVm`，run main.zpkg 用 `runVm`
- [x] 1.3 文档同步：test-gate.md 机制说明 + ci.yml 注释校正
- [x] 1.4 验证：改后 scripts 成功编出 xtask.zpkg（z42 代码编译干净）；逻辑等价于 CI `test-consume` 已全绿的
      release-编译配置（1.2min）。**本地 from-source GREEN 客观不可得**——唯一本地种子已过时 2+ nightly，连
      origin/main 的 `[Record]`/tuple 语法都解析不了（分阶段自举纪律：编当前源须用最新 nightly 种子，仅 CI 有）。
      按 bootstrap-seed.md，种子-gated 路径 GREEN **以 CI 为准**：PR 的 ci-bootstrap 下载最新 nightly + 两代自举，
      在全部 test-host 腿跑改过的 cross-zpkg。

## 备注
- `_buildCompilerPkg`/`_invokeBuildCompiler` 的内部形参名仍叫 `vmBin`——它们只做编译，现恒接 `compileVm`，语义正确不改名。
- 非 cross-zpkg 的 multi-exe 路径（`_testMultiExe`，xtask_test_multiexe.z42:51 也用 debug VM 编译）是同类可优化项，
  但不在本 change scope（不是 test-host pole）；如需另开 change。
