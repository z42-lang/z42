# testhost — 跨平台测试宿主(共享)

## 职责

跑测试用例的**平台无关**部分:一份 z42 test-agent(字节码),经嵌入式 z42vm 在每个平台
(desktop 自包含 / wasm / iOS / android)运行同一套 `Std.Test.Runner`。消除"测试 driver 4 语言
各写一遍"的重复。**只做"跑",不做"构建"**——构建走 z42b + workload;跑走这里的 agent + 嵌入。

## 功能索引

| 功能 | 入口 / 文件 |
|------|-----------|
| test-agent(命令 → 跑测试 → 结构化报告) | `agent/src/agent.z42` 的 `Z42.TestHost.Agent.Main` |
| agent 工程(app.zpkg) | `agent/z42.testagent.z42.toml` |
| 实际测试运行逻辑(发现/执行/报告) | `Std.Test.Runner.RunModule`(`src/libraries/z42.test`)|
| 嵌入入口(load app.zpkg + 跑) | `z42-host::run_app` → `z42::app::run`(`src/runtime`) |

## 基础用法

```bash
# 编 agent → app.zpkg（z42b/z42c build）
z42c build src/toolchain/testhost/agent/z42.testagent.z42.toml --release
# 嵌入跑（desktop 参考壳 / 或直接 z42vm）：agent 收 <target.zbc> [format]，吐 JSON 报告
z42vm z42.testagent.zpkg -- <target-test.zbc> json
```

## 如何测试验证

```bash
# desktop 参考端到端（阶段 3）：desktop shell 链 libz42 + z42-host::run_app 跑 agent
./xtask test platform desktop        # （接入后）嵌入式跑一个 test 用例 → 结构化结果
```

## 关联文档

- 设计/机制:[embedded-app-run 设计](../../../docs/design/testing/embedded-app-run.md)、
  [cross-platform-testing](../../../docs/design/testing/cross-platform-testing.md)
- 引入:change `add-embedded-app-run`(`docs/spec/changes/` 或已归档)

## 核心文件

| 文件 | 职责 |
|------|------|
| `agent/src/agent.z42` | test-agent:命令 → `Runner.RunModule` → 报告 |
| `agent/z42.testagent.z42.toml` | agent app 工程(deps: z42.core/io/test) |
