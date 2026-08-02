# Tasks: 接入嵌入式 test-host（desktop harness flow）

> 状态：🟡 进行中 | 创建：2026-08-02 | 分支：wire-embedded-testhost(off origin/main)

## 阶段 1: 构建助手
- [ ] 1.1 `scripts/test/xtask_test_embedded.z42`：`_ensureTestAgent(root)` —— z42c build
      testhost/agent → z42.testagent.zpkg（缺则建），返回路径
- [ ] 1.2 `_ensureDesktopTesthost(root)` —— cc workload/desktop/shell/testhost.c + 链 libz42.a
      （+ _nativeLibs；缺 libz42.a 则 cargo rustc staticlib）→ testhost 可执行，返回路径

## 阶段 2: 命令 + 端到端
- [ ] 2.1 `_testEmbedded(r)`：ensure agent + testhost → 跑 `testhost agent.zpkg <target> <format>` → 输出
- [ ] 2.2 `xtask_cli.z42` 注册 `test embedded <target.zbc> [format]`
- [ ] 2.3 端到端：`xtask test embedded <sample_tests.zbc> json` → JSON 报告（复现手动 cc 的结果）

## 阶段 3: 文档 + 验证归档
- [ ] 3.1 embedded-app-run.md 补 xtask 用法
- [ ] 3.2 xtask.zpkg 编译 + `test embedded` 端到端绿
- [ ] 3.3 归档 + PR

## 备注
- 复用 #95：z42_host_run_app / libz42.a / z42.testagent。desktop 先行,mobile 复用 agent 构建。

## 归一模型（User 定调，见 design.md；MVP 之上的完整目标）
- [x] 设计定稿：清单驱动命名用例 + 打包一个/全部 + 跑一个/全部 + golden 归一 [Test]（design.md）
- [ ] G1 golden→[Test] wrapper 生成器（build 期：读 ns+expected → 生成 wrapper → 同编）
      + 可能给 Std.Test 补 `CaptureStdout(Action)` 便捷 API
- [ ] G2 清单生成 + bundle（全语料 → zbc + 清单 name→zbc + stdlib）；`--case` 单个
- [ ] G3 agent 加清单 + `--filter`（逐 zbc RunModule）→ 汇总 JSON
- [ ] G4 `xtask test embedded --case/--filter` 重构走 bundle；Rust 内部测试仍 `test runtime`
- [ ] G5 端到端（一个 golden + 一个 [Test] 单元 → bundle → agent → 汇总）；GREEN 以 CI 为权威
- [ ] G6 mobile（后续 change）：bundle 作 asset 打进 app + 各平台壳复用 z42_host_run_app
