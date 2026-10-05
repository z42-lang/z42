# toolchain/builder — z42 构建编排器（`z42b`）

## 职责

z42 项目「编译 → 发布」**全流程的构建编排器**：读 `z42.toml` / `--rid` →
装配并驱动 [`z42.build`](../../compiler/z42.build/) 管线，逐相位调度执行
（Resolve → Compile → Trim → Assets → Configure → GenerateProject → NativeBuild → Package）。
编译为 `z42b.zpkg`（Exe-mode），由 launcher 命令分发调用
（`z42 build` / `publish` / `export` / `run --rid` / `test`）。

类比关系（沿用 launcher 的「z42 源 → zpkg → apphost」模式）：

```
src/toolchain/builder/core/*.z42  →  z42b.zpkg  →  apphost z42b
（同 launcher 模式：launcher/core/*.z42 → launcher.zpkg → z42）
```

**不做**：
- **编译本身** —— 经 `z42.build` 的 `ICompiler` 接口**在进程内**调编译器库（z42c）。
  与独立 `z42c.driver` CLI **引用同一份实现，不 fork z42c 子进程**；本模块只编排 + 注入。
- **平台专属实现** —— 住各 workload 的 `*.workload.zpkg`（`: WorkloadBase` 子类）。
- **管线接口/相位流程定义** —— 住 [`src/compiler/z42.build/`](../../compiler/z42.build/)
  （`Pipeline` / `IPipelineContext` / `ICompiler` / `WorkloadBase` / `BuildHooks`）。本模块是**驱动方**。

## 功能索引

| 功能 | 入口 / 文件 |
|------|-----------|
| 命令路由（new / test / bench / clean / publish / build / export） | `core/builder_cli.z42` |
| 管线编排与编译器注入 | `core/builder.z42` 的 `_orchestrate` / `_hostCompiler` |
| 反射式 `[Test]` / `[Benchmark]` 运行 | `core/builder_test.z42` |
| desktop publish（apphost + payload） | `core/builder_publish.z42`、`core/builder_apphost.z42` |
| 设备 RID 构建 + 部署 + 运行 | `core/builder_device*.z42` |

## 基础用法

```bash
z42 build / z42 publish / z42 test     # 经 launcher 转发到 z42b
```

完整命令参考见 [z42 / z42c / z42b 命令参考](../../../docs/reference/src/toolchain/cli-z42c-z42b.md)。

## 如何测试验证

```bash
xtask test targets      # tests/fixtures/{manifest-targets,z42b}/ 夹具：[[test]] / [[example]] / [[bench]] target、清单段、build hook、孤儿源守卫
xtask test stdlib       # z42b 作为 [Test] 运行器跑全部 stdlib 单元
```

`tests/fixtures/` 放 harness 驱动的夹具工程，不是 `[Test]` 单元；约定见
[测试用例组织规范](../../../docs/internals/src/devinfra/test-layout.md)。

## 关联文档

- 设计与机制：[z42b 构建编排器](../../../docs/internals/src/toolchain/z42b.md)、[launcher](../../../docs/internals/src/toolchain/launcher.md)

## 核心文件（`core/`）

| 文件 | 职责 |
|------|------|
| `core/builder_cli.z42` | **CLI 路由**（对照 `launcher_cli.z42`）：`Std.Cli` 嵌套 router + dispatch。verbs：new / test / bench / clean / publish（用户经 `z42` 到达，帮助名写 `z42 <verb>`）+ build / export（编排方直接调用）|
| `core/builder.z42` | **编排核心**：`_orchestrate` 选路径 → 构造 `Pipeline`（注入 `ICompiler` + workload + hooks）+ `PipelineContext` → `Run`；`_hostCompiler` 运行时加载 `programs/z42c/z42c.pipeline.zpkg` 注入编译器（缺失则 `NoCompiler` 兜底）|
| `core/builder_commands.z42` | **命令处理**：build / export / publish 共用 `_runVerb`（ManifestLoader → Target → `_orchestrate`）|
| `core/builder_hooks.z42` | 项目 build hook 动态注入：把 `[build] hooks` 目录编成 zpkg → 加载 `Build.ProjectHooks` |
| `core/builder_new.z42` | **`new` 脚手架**：生成 z42.toml + src 模板（exe/lib）+ .gitignore + README；工程名校验、`--path` 为父目录 |
| `core/builder_test.z42` | **test / bench**：反射式 `[Test]`/`[Benchmark]` 运行器。target 双形态：已编译 `.zbc/.zpkg` 直跑 / 工程 `z42.toml`（或无 target）→ compile-then-test |
| `core/builder_dev_targets.z42` | test / bench 目标从真 manifest 解析并在内存派生 `ProjectManifest`（保留父包身份）|
| `core/builder_publish.z42` | **desktop publish**：产 apphost + `[platform.desktop]` `bin`/`payload` 布局 + 依赖/native/payload 落位；launcher 转发 `z42 publish` 至此 |
| `core/builder_publish_build.z42` | publish 前每次经 z42c 增量编一遍；`--no-build` 保留「就用现成字节」供 xtask 的 SDK 组装 / 自举不动点路径 |
| `core/builder_publish_sidecar.z42` | publish 时把 `<name>.runtimeconfig.toml` 侧车随 zpkg 一起搬进部署布局 |
| `core/builder_apphost.z42` | **apphost patcher 的唯一实现**（`_pubProduceApphost`）；xtask 打包经 `z42b publish` 复用。MAGIC 须与 Rust stub 同步 |
| `core/builder_device{,_ios,_android}.z42` | 设备 RID 的 build + deploy + run 驱动（wasm / iOS 模拟器 / Android 模拟器），`xtask test platform` 委托于此 |
| `core/builder_pins.z42` | 由 `scripts/versions.toml` 生成的平台版本常量（xtask 生成、不入库）|
| `core/hooks/hooks.z42` | z42b 自身的 build hook：编译前把 `versions.toml` 常量渲染成 `builder_pins.z42` |
| `tests/fixtures/` | harness 驱动的夹具工程（`manifest-targets/`、`z42b/`）|

## 依赖关系

- 依赖 [`src/compiler/z42.build/`](../../compiler/z42.build/)（管线框架接口）、
  [`src/compiler/z42.project/`](../../compiler/z42.project/)（`z42.toml` 模型）。
- 调用 `z42c`（编译）、各 workload（平台尾相位）；经 `extern` 调 VM native 原语
  （Sign / Archive / Hash / Download / ProbeVersion，住 `runtime`）。
- 被 launcher 命令分发调用（见 [`docs/internals/src/toolchain/launcher.md`](../../../docs/internals/src/toolchain/launcher.md)）。
