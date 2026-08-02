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
