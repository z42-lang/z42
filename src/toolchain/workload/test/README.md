# workload/test — 测试运行能力 workload（按需下载）

## 职责

承载**跑测试流程**所需的**平台无关共享件**——目前是 on-device test-agent（一份 z42 字节码，
经嵌入式 z42vm 在每个平台跑同一套 `Std.Test.Runner`）。作为按需能力 workload
（`z42 workload install test`）分发：**要跑测试才下载**，不进 SDK 恒在核心，z42b 编排时拉取并调用，
自身不夹带此 payload。

**只做「共享那一半」**：平台专属的嵌入 host 壳（`Z42TestHost.swift` / `.kt` / `testhost.c` /
`run.js`）住各 `workload/<plat>/`，随对应平台 workload 下载；本 workload 只放平台无关的 agent。
构建走 z42b + workload，跑走这里的 agent + 嵌入。

## 功能索引

| 功能 | 入口 / 文件 |
|------|-----------|
| test-agent（命令 → 跑测试 → 结构化报告） | `agent/src/agent.z42` 的 `Z42.TestHost.Agent.Main` |
| agent 工程（app.zpkg） | `agent/z42.testagent.z42.toml` |
| 实际测试运行逻辑（发现/执行/报告） | `Std.Test.Runner.RunModule`（`src/libraries/z42.test`）|
| 嵌入入口（load app.zpkg + 跑） | `z42-host::run_app` → `z42::app::run`（`src/runtime`） |

## 基础用法

```bash
# 编 agent → app.zpkg（z42b/z42c build）
z42c build src/toolchain/workload/test/agent/z42.testagent.z42.toml --release
```

agent 通过转发的 `-- <args>` 收一条一次性命令，签名（`agent/src/agent.z42` 的 `Main`）：

```
<target> [format] [out-path]
```

| 参数 | 取值 | 默认 |
|------|------|------|
| `target` | `<x.zbc>`（单测试模块）或 `<manifest.json>`（测试包清单） | 必填 |
| `format` | `json` / `pretty` / `tap` | `json` |
| `out-path` | 报告写到此文件；省略 → 写 stdout | 空（stdout） |

**两种 target：**

- **单模块**（`.zbc`）——`Runner.RunModule` 自动发现该模块的 `[Test]` / `[Benchmark]` 并跑，按
  `format` 渲染。给了 `out-path` 时改走 `RunModuleResults` → 聚合 JSON → 写文件。
- **测试包**（以 `.json` 结尾的 manifest `{cases:[…]}`）——聚合成**一份**报告，用例分两类：
  - `golden` = `{name, zbc, entry, expected}`：整个程序，在**全新隔离 VM**（`RunGoldensIsolated`，
    每例独立 `VmContext` 不串味）跑，比对 stdout 与 `expected` 文件。
  - `unit` = `{name, zbc}`：`[Test]` 模块，共享 VM 跑（命名空间隔离，天然无冲突）。

**bundle 模式的两个 env：** `Z42_LIBS`（golden 隔离 VM 的 stdlib 目录，缺省回落 `<bundle>/../libs`）、
`Z42_TEST_JOBS`（golden 并行度：`0`/unset = auto，`1` = 串行）。

```bash
z42vm z42.testagent.zpkg -- <target-test.zbc> json                 # 单模块 → stdout
z42vm z42.testagent.zpkg -- <target-test.zbc> json /tmp/report.json # 单模块 → 写文件（无 stdout 的嵌入宿主）
z42vm z42.testagent.zpkg -- bundle-manifest.json pretty            # 测试包（聚合 golden + unit）
```

## 如何测试验证

```bash
xtask test embedded        # desktop 嵌入 harness：build agent + 壳 → 跑用例 → 结构化结果
xtask test platform desktop # 平台 backend 端到端（可跑时）
```

## 关联文档

- 设计/机制：[embedded-app-run](../../../../docs/design/testing/embedded-app-run.md)、
  [cross-platform-testing](../../../../docs/design/testing/cross-platform-testing.md)（两层模型 + bundle 缝）
- 引入/演进：change `unify-test-pipeline-z42b`（`docs/spec/changes/` 或已归档）——本 workload 由
  独立 `toolchain/testhost/` 迁入；z42b 接管部署 + payload-only workload 打包为后续阶段

## 核心文件

| 文件 | 职责 |
|------|------|
| `agent/src/agent.z42` | test-agent：命令 → `Runner.RunModule` / bundle 聚合 → 报告 |
| `agent/z42.testagent.z42.toml` | agent app 工程（deps: z42.core/io/test/json） |
