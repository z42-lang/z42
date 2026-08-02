# Proposal: 接入嵌入式 test-host（desktop harness flow）

## Why

`add-embedded-app-run`(#95)已把嵌入基座 + desktop C 壳 + 共享 test-agent 打通,但只在**手动 cc**
下验证。要成为可用的 harness,需把"build agent → 嵌入壳跑测试 → 收结构化结果"**productionize
进 xtask**。这是原始三问(构建 test 应用 / 构建资产 / 返回结果)的 desktop 端到端落地,也是后续
mobile 壳复用的模板。

## What Changes

1. **xtask 命令 `test embedded <target.zbc> [format]`**:构建 test-agent(z42.testagent.zpkg,
   缺则 z42c build)+ 构建 desktop testhost 壳(cc `workload/desktop/shell/testhost.c` + 链 libz42,
   缺则 build)→ 嵌入跑 `testhost agent.zpkg <target.zbc> <format>` → 输出结构化结果(json)。
2. **共享构建助手**:`_ensureTestAgent`(编 agent → app.zpkg)+ `_ensureDesktopTesthost`
   (cc 壳,static 链接)——放 toolchain,后续 mobile 复用 agent 构建那半。
3. 复用 #95 的 `z42_host_run_app` C 符号 + libz42.a + agent。

## Scope

| 文件 | 变更 | 说明 |
|------|------|------|
| `scripts/test/xtask_test_embedded.z42` | NEW | `_testEmbedded` + `_ensureTestAgent` + `_ensureDesktopTesthost` |
| `scripts/xtask_cli.z42` | MODIFY | 注册 `test embedded` 子命令 |
| `docs/design/testing/embedded-app-run.md` | MODIFY | 补「xtask test embedded」用法 |

**只读**:`workload/desktop/shell/testhost.c`、`testhost/agent/z42.testagent.z42.toml`、
`xtask_test_desktop.z42`(_nativeLibs / cc 链接范式)。

## Out of Scope

- mobile 壳(wasm/ios/android)—— 复用 agent + z42_host_run_app,后续 change。
- 全 src/tests 套件批量 + JUnit 汇总 + 接入 `test platform`——先打通单模块 MVP,批量后续。
- 命令通道 persistent agent。

## Open Questions
- 无（#95 已定型;本 change 是其 desktop harness 落地）。
