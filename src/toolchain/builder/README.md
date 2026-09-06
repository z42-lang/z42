# toolchain/builder — z42 构建编排器（`z42b`）

## 职责

z42 项目「编译 → 发布」**全流程的构建编排器**：读 `z42.toml` / `--rid` →
装配并驱动 [`z42.build`](../../libraries/z42.build/) 管线，逐相位调度执行
（Resolve → Compile → Trim → Assets → Configure → GenerateProject → NativeBuild → Package）。
编译为 `z42b.zpkg`（Exe-mode），由 launcher 命令分发调用
（`z42 build` / `publish` / `export` / `run --rid` / `test`）。

类比关系（沿用 launcher 的「z42 源 → zpkg → apphost」模式）：

```
src/toolchain/builder/core/*.z42  →  z42b.zpkg  →  apphost z42b
（对照 launcher/core/*.z42 → launcher.zpkg → z42）
```

**不做**：
- **编译本身** —— 经 `z42.build` 的 `ICompiler` 接口**在进程内**调编译器库（z42c）。
  与独立 `z42c.driver` CLI **引用同一份实现，不 fork z42c 子进程**；本模块只编排 + 注入。
- **平台专属实现** —— 住各 workload 的 `*.workload.zpkg`（`: WorkloadBase` 子类）。
- **管线接口/相位流程定义** —— 住 [`src/libraries/z42.build/`](../../libraries/z42.build/)
  （`Pipeline` / `IPipelineContext` / `ICompiler` / `WorkloadBase` / `BuildHooks`）。本模块是**驱动方**。

> **取代原 `packager/` 占位**：旧 packager 设想的「把 z42 程序 + 运行时打成可分发件」
> 只是本管线尾部 `Assets` / `Package` 两个相位的一部分；构建编排是其超集，故 packager
> 占位并入本目录，不再单列。

## 核心文件（`core/`）

**LIVE（已入 build，`include` 于 `z42.builder.z42.toml`）：**

| 文件 | 职责 |
|------|------|
| `core/builder_cli.z42` | **CLI 路由**（对照 `launcher_cli.z42`）：`Std.Cli` 嵌套 router + dispatch。LIVE verbs：test / bench / clean / **publish**；new / build / export 仍打 "pending wire-z42b-host-build" |
| `core/builder_test.z42` | **test / bench**：反射式 `[Test]`/`[Benchmark]` 运行器（取代 Rust z42-test-runner，retire-test-runner）。**target 双形态**：已编译 `.zbc/.zpkg` 直跑 / 工程 `z42.toml`（或无 target 默认）→ **compile-then-test**（经注入编译器 `_buildProject` 现编到 dist 再反射跑，`add-z42b-compile-then-test`）|
| `core/builder_publish.z42` | **desktop publish**（move-publish-to-z42b）：产 apphost + `[platform.desktop]` `bin`/`payload` 布局 + 依赖/native/payload 落位。launcher 转发 `z42 publish` 至此，并经 `Z42_APPHOST_TEMPLATE` 传预解析的 apphost stub。**不依赖 z42.project/z42.build**（不碰自举串味雷区）|
| `core/builder_publish_build.z42` | **产物新旧这一步**（fix-publish-stale-payload）：`_pubEnsureBuilt` 每次都经 z42c 编一遍（增量，未变即空转）→ 源码改了 publish 就重出产物；`--no-build` 保留「就用现成字节」契约给 xtask 的 SDK 组装 / 自举不动点路径。此前是「zpkg 文件在就当已最新」，会把旧 payload 静默重签 |
| `core/builder_apphost.z42` | **内联 apphost patcher**（`_pubProduceApphost`）：z42b 兼作测试运行器，`stdlib [Test]` 构建阶段只有 stdlib 在 Z42_LIBS，**看不到 `z42.workload.desktop`**——故内联 patcher，z42b 保持纯 stdlib 依赖。xtask 打包（`_packageDesktop`）已改为调用 `z42b publish` 复用此实现（move-desktop-packaging-to-publish），不再自带副本。⚠ MAGIC 须与 Rust stub 同步 |

**PARKED（不在 build，待 `wire-z42b-host-build` 接入 in-process 编译器 API）：**

| 文件 | 职责 |
|------|------|
| `core/builder.z42` | **编排核心**：`_orchestrate` 选路径 → 构造 `Pipeline`（注入 `ICompiler` + workload + hooks）+ `PipelineContext`→ `Run`。标准路径进程内组合（零子进程/零代码生成）|
| `core/builder_commands.z42` | **命令处理**：build/export 共用 `_runVerb`（ManifestLoader → Target → `_orchestrate`）|
| `core/builder_new.z42` | **`new` 脚手架**：生成 z42.toml + src 模板（exe/lib/test）+ .gitignore + README。纯 `Std.IO` |

## 计划模块（实现期补全）

| 模块 | 职责 |
|------|------|
| 共享编译实现适配 | `_hostCompiler()` 暂返 `NoCompiler`；待 `extract-compile-pipeline-api` 落地 `PackageCompiler`/`CompileResult` 后封装为 `Z42cCompiler : ICompiler`（与 `z42c.driver` 同一份）|
| driver 装配 | 项目带自定义 `build/` 时，组装一次性 driver 源码（链 `z42.build` + workload + 项目 `build/`），用**同一 `ICompiler`** 编译后运行 |
| 平台 workload | `_selectWorkload()` 暂返 `WorkloadBase` no-op；待各 `*.workload` 库的 `: WorkloadBase` 子类（desktop/ios/android/wasm）|
| test/bench | 见 `retire-test-runner` spec（前置 boxing 0.3.11 + Method.Invoke 0.3.12）|

> **`IPipelineContext` 实现归属（2026-06-23 决策）**：暂置 `z42.build` 库
> （[`PipelineContext.z42`](../../libraries/z42.build/src/PipelineContext.z42)），编排器 import 它构造 ctx。
> in-process 编译让**标准路径无需生成 driver**（直接进程内组合 Pipeline 跑），仅项目带自定义
> `build/` 的自定义路径才落 driver 生成。
>
> **计划重构**：`ICompiler` 等编译接口后续抽到中立微库，z42c 与 z42b 同依赖该微库——面向接口，
> 「改成直接调 z42c」只换实现不动调用方。落地写 `build-orchestrator.md` 时入 Deferred。

## 依赖关系

- 依赖 [`src/libraries/z42.build/`](../../libraries/z42.build/)（管线框架接口）、
  [`src/libraries/z42.project/`](../../libraries/z42.project/)（`z42.toml` 模型）。
- 调用 `z42c`（编译）、各 workload（平台尾相位）；经 `extern` 调 VM native 原语
  （Sign / Archive / Hash / Download / ProbeVersion，住 `runtime`）。
- 被 launcher 命令分发调用（见 [`docs/design/toolchain/launcher-command-dispatch.md`](../../../docs/design/toolchain/launcher-command-dispatch.md)）。

## 状态

🔴 **占位 / 未接编译**。当前仅目录骨架 + 本 README，**未登记 workspace / xtask / CI**，
不影响任何现有构建。

落地走 spec-first（架构性变更），设计文档 `docs/design/toolchain/build-orchestrator.md`（待建）。
**前置**：replace-csharp S5 完成（z42c 成生产编译器、`toolchain` 子系统解锁）。
推进计划见 `docs/roadmap.md`。
