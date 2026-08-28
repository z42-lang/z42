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
```

agent 通过转发的 `-- <args>` 收**一条一次性命令**，签名（`agent/src/agent.z42` 的 `Main`）：

```
<target> [format] [out-path]
```

| 参数 | 取值 | 默认 |
|------|------|------|
| `target` | `<x.zbc>`（单测试模块）或 `<manifest.json>`（测试包清单） | 必填 |
| `format` | `json` / `pretty` / `tap` | `json` |
| `out-path` | 报告写到此文件；省略 → 写 stdout | 空（stdout） |

**两种 target：**

- **单模块**（`.zbc`）——`Runner.RunModule` 自动发现该模块的 `[Test]` / `[Benchmark]` 并跑，按 `format` 渲染。给了 `out-path` 时改走 `RunModuleResults` → 聚合 JSON → 写文件。
- **测试包**（以 `.json` 结尾的 manifest `{cases:[…]}`）——聚合成**一份**报告，用例分两类：
  - `golden` = `{name, zbc, entry, expected}`：整个程序，在**全新隔离 VM**（`RunGoldensIsolated`，每例独立 `VmContext` 不串味）跑，比对 stdout 与 `expected` 文件。
  - `unit` = `{name, zbc}`：`[Test]` 模块，共享 VM 跑（命名空间隔离，天然无冲突）。

**bundle 模式的两个 env：**

| env | 作用 |
|-----|------|
| `Z42_LIBS` | golden 隔离 VM 的 stdlib 目录；缺省回落 `<bundle>/../libs` |
| `Z42_TEST_JOBS` | golden 并行度：`0`/unset = auto（核数），`1` = 串行 |

```bash
# 单模块 → stdout（desktop harness 捕获进程 stdout）
z42vm z42.testagent.zpkg -- <target-test.zbc> json
# 单模块 → 写文件（wasm / iOS / Android 无进程 stdout，写 VFS/临时路径再读回）
z42vm z42.testagent.zpkg -- <target-test.zbc> json /tmp/report.json
# 测试包（聚合 golden + unit）
z42vm z42.testagent.zpkg -- bundle-manifest.json pretty
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
