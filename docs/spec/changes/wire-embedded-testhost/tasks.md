# Tasks: 接入嵌入式 test-host（desktop harness flow）

> 状态：🟡 进行中 | 创建：2026-08-02 | 分支：wire-embedded-testhost(off origin/main)

## 阶段 1: 构建助手
- [x] 1.1 `scripts/test/xtask_test_embedded.z42`：`_ensureTestAgent(root)` —— z42c build
      testhost/agent → z42.testagent.zpkg（缺则建），返回路径
- [x] 1.2 `_ensureDesktopTesthost(root)` —— cc workload/desktop/shell/testhost.c + 链 libz42.a
      （+ _nativeLibs；缺 libz42.a 则 cargo rustc staticlib）→ testhost 可执行，返回路径

## 阶段 2: 命令 + 端到端
- [x] 2.1 `_testEmbedded(r)`：ensure agent + testhost → 跑 `testhost agent.zpkg <target> <format>` → 输出
- [x] 2.2 `xtask_cli.z42` 注册 `test embedded <target.zbc> [format]`
- [x] 2.3 端到端：`xtask test embedded <sample_tests.zbc> --format json` → JSON（total:2 passed:2）✅ 本地验证（工具链修复后）

## 阶段 3: 文档 + 验证归档
- [ ] 3.1 embedded-app-run.md 补 xtask 用法
- [x] 3.2 xtask.zpkg 编译（49 文件，含 xtask_test_embedded）+ `test embedded` 端到端绿 ✅
- [ ] 3.3 归档 + PR

## 备注
- 复用 #95：z42_host_run_app / libz42.a / z42.testagent。desktop 先行,mobile 复用 agent 构建。

## 归一模型（User 定调，见 design.md；MVP 之上的完整目标）
- [x] 设计定稿：清单驱动命名用例 + 打包一个/全部 + 跑一个/全部 + golden 归一 [Test]（design.md）
- [x] G1 golden→[Test] wrapper 生成器（_wrapGoldenSource + _buildGoldenCase：读 ns+expected →
      注入 using Std.Test + 追加 [Test]（TestIO.captureStdout + Assert.Equal，短名）→ 同编）。
      `Std.Test.captureStdout` 已存在无需补。验证：`xtask test embedded <golden-dir>` 对
      zlib_format(无 ns) + default_params(有 ns) 均 passed。踩坑：FQ Std.Test.TestIO 解析成
      field-chain→Null，改用 using+短名
- [x] G2 bundle 构建（_buildTestBundle：goldens 归一 + stdlib [Test] units → zbc + manifest.json name→zbc；build 期 filter）
- [x] G3 agent manifest 模式（读 manifest → 逐 zbc Runner.RunModuleResults → 手动拼 TestResult[] → TestReport.toJson 一次汇总）；+ Runner.RunModuleResults(z42.test)
- [x] G4 `xtask test embedded [--case/--filter | golden-dir | .zbc]`（CLI 注册 --case/--filter）；Rust 内部测试仍 test runtime
- [x] G5 端到端验证：--filter zlib_format(golden→[Test]) 1/1；--filter split_options(stdlib unit) 7/7；--filter assert 43/43 多用例汇总（module=bundle 一次报告）。GREEN 全量以 CI 为权威
- [ ] G6 mobile（后续 change）：bundle 作 asset 打进 app + 各平台壳复用 z42_host_run_app
