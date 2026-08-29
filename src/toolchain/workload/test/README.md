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
| 单模块执行逻辑（发现/执行/报告） | `Std.Test.Runner.RunModule`（`src/libraries/z42.test`）|
| **bundle 执行逻辑（golden 隔离 + unit 共享 + 聚合）** | `Std.Test.BundleRunner.RunBundle`（`src/libraries/z42.test`）——**agent 与 z42b 共用一份核**（wire-z42b-embedded-test ②b） |
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
- **测试包**（以 `.json` 结尾的 manifest `{cases:[…]}`）——聚合成**一份**报告。agent 把 manifest 解析成
  `BundleCase[]` 后交 **`Std.Test.BundleRunner.RunBundle`** 执行（**host 上 z42b 直接调同一函数**，无需
  agent/testhost 进程——wire-z42b-embedded-test ②b）。用例分两类：
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
xtask test embedded        # host: xtask 组 bundle → 委托 z42b test --rid host（in-process）
xtask test embedded --rid android-x64  # device: xtask 组 bundle → z42b 组装 {app,libs,bundle} deployable
xtask test platform desktop # 老 IPlatformBackend 原生 R1–R7 嵌入契约（与 test-agent 语料路径无关）
```

## 打包发布（payload-only workload）

test workload 是**纯 payload** workload（不同于 desktop/ios/android/wasm 的 per-RID「tooling」）：
只含一份平台无关 `z42.testagent.zpkg`，无 per-RID apphost、无 runtime pack。打包与发布：

```bash
xtask package workload test [<version>]   # 产 z42-workload-<version>-test/（agent zpkg + manifest.toml）
z42 workload install test                 # 用户按需下载安装（release-index.json 的 test 条目）
```

manifest 复用 `kind="workload-tooling"`（`host=["*"]`、无 runtime pack），单 zpkg 由新
`[contents.payload]` 段描述（install 侧 `runtimes=[]` → 天然跳过 bedding，同 desktop）。CI（release /
publish-nightly）在 macos-arm64 单 host 建一次 + 归档 `z42-workload-<label>-test.tar.gz` + 纳入
`package index`。见 change `package-test-workload`。

## 关联文档

- 设计/机制：[test-pipeline](../../../../docs/book/src/toolchain/test-pipeline.md)（两层模型：z42b 单-bundle
  执行器 + xtask fleet 编排器 + BundleRunner 缝，SoT）；旧
  [embedded-app-run](../../../../docs/design/testing/embedded-app-run.md)、
  [cross-platform-testing](../../../../docs/design/testing/cross-platform-testing.md)（迁移中）
- 引入/演进：change `unify-test-pipeline-z42b`（阶段①归位）+ `wire-z42b-embedded-test`（②b：z42b 接管
  host bundle 执行 + 设备语料组装）+ `package-test-workload`（payload-only 打包发布 + `workload install`
  描述泛化为「平台 tooling 或能力」）

## 核心文件

| 文件 | 职责 |
|------|------|
| `agent/src/agent.z42` | test-agent：命令 → `Runner.RunModule`（单模块）/ manifest→`BundleRunner.RunBundle`（bundle）→ 报告 |
| `agent/z42.testagent.z42.toml` | agent app 工程（deps: z42.core/io/test/json） |
