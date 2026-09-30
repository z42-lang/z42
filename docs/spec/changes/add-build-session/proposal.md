# Proposal: 统一的包构建会话（BuildSession）—— 第一步：z42b 走完整的清单语义

## Why

「构建一个包」有两套平行实现：z42c CLI 的 `Main._build`（约 700 行，功能完整）与 z42b 经 `ICompiler`
反射加载的 `Z42cCompiler`（约 140 行）。后者是前者的残缺副本，**`z42b test` / `z42b bench` / `z42b build`
编译出的产物与 `z42c build` 不一致**：

- 忽略 `[optimize]` / `[syntax]` / `[lints]` / `[analyzers]`（含 generator）、manifest `entry`；版本号写死 `0.0.0`；
- 警告全部丢弃（只在失败时回传诊断）；
- 把 `Z42_LIBS` 下**全部** zpkg 当成已声明依赖 ⇒ 没有 E0497、也不查「声明的依赖找不到」；
- release 构建主 zpkg 照剥符号、`.zsym` 却因 `Pipeline` 写死 `StripSymbols = false` 被丢弃 ⇒ 调试符号丢失。

两份实现各自演进、已经漂移，这正是全仓审查（2026-09-30）第 4 步要消灭的「平行实现」。

## What Changes

本 change 是两步中的第一步（第二步 `unify-driver-on-build-session` 见 Out of Scope）：

- 在 `z42c.pipeline` 新增 **`BuildSession`**：按清单驱动的全部编译决策（opt / syntax / lints / analyzers /
  entry / version / pack / strip / 声明依赖检查）集中于此；诊断经 `IBuildReporter` 回调或结果对象收集，
  不直接打印；部署（exe 装配、侧车）经可空的 `IDeployStep` 回调，不进 pipeline。
- 新增 **`ManifestKnobs`**：`[optimize]` / `[syntax]` / `[lints]` / pack 决议的纯函数版本（输入清单，输出决议 +
  错误行），供 BuildSession 使用；第二步 driver 也改用它，届时删除 `Main._build` 里的同段代码。
- **`Z42cCompiler` 改为 BuildSession 的薄封装**（Role = HostTarget：强制 packed 单产物、不解析 path 闭包——
  z42b 自己编排闭包、不增量）。
- `CompileRequest` 加 body 字段 `Manifest`（`ProjectManifest` 对象，测试目标的清单只在内存里）、`ProjectDir`；
  `CompileResult` 加 `Warnings`。均为构造后赋值的 body 字段（种子 ABI 约束）。
- `z42.build/Pipeline`：传清单、StripSymbols 按 profile（release ⇒ true），并把警告经 `ctx.Warn` 呈现。
- `builder_dev_targets._deriveTargetManifest`：派生测试目标清单时补拷 optimize / syntax / lints / analyzers / entry。
- 删除死字段 `CompileInputs.HasPkgContext`（全仓无读取）。

**用户可见变化**：`z42b test` / `bench` / `build` 开始认清单各段、报 E0497 与缺失依赖、显示警告、release 保留 `.zsym`。
`z42c build` 行为与字节**不变**（本 change 不碰 driver）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.pipeline/src/BuildSession.z42` | NEW | `BuildOptions` / `BuildResult` / `IBuildReporter` / `IDeployStep` / `BuildRole` / `BuildSession` |
| `src/compiler/z42c.pipeline/src/ManifestKnobs.z42` | NEW | opt / syntax / lints / pack 决议（纯函数，返回错误行）|
| `src/compiler/z42c.pipeline/src/Z42cCompiler.z42` | MODIFY | 改为 BuildSession 薄封装 |
| `src/compiler/z42c.pipeline/src/PackageCompile.z42` | MODIFY | 删 `HasPkgContext` |
| `src/compiler/z42c.pipeline/README.md` | MODIFY | 登记新文件 |
| `src/compiler/z42c.pipeline/tests/buildsession/buildsession_tests.z42` | NEW | 会话单测（旋钮 / pack / 依赖检查 / 诊断收集）|
| `src/compiler/z42c.pipeline/tests/buildsession/z42c.pipeline.test.buildsession.z42.toml` | NEW | 测试单元清单 |
| `src/compiler/z42c.pipeline/tests/z42ccompiler/z42ccompiler_tests.z42` | MODIFY | 覆盖 manifest 各段在 host 路径生效 |
| `src/compiler/z42.build/src/ICompiler.z42` | MODIFY | `CompileRequest.Manifest/ProjectDir`、`CompileResult.Warnings`（body 字段）|
| `src/compiler/z42.build/src/Pipeline.z42` | MODIFY | 传清单、strip 按 profile、呈现警告 |
| `src/toolchain/builder/core/builder_dev_targets.z42` | MODIFY | 派生清单补拷各段 |
| `scripts/test/xtask_test_targets.z42` | MODIFY | 新 smoke：`z42b test` 认 `[syntax]` / 报 E0497 / 显示警告 |
| `src/tests/z42b/manifest-sections/z42.toml` | NEW | smoke 夹具 |
| `src/tests/z42b/manifest-sections/src/lib.z42` | NEW | smoke 夹具 |
| `src/tests/z42b/manifest-sections/tests/t.z42` | NEW | smoke 夹具 |
| `docs/internals/src/compiler/architecture.md` | MODIFY | 构建会话一节 |
| `docs/internals/src/toolchain/z42b.md` | MODIFY | Compile 相位走 BuildSession、各段生效 |
| `docs/reference/src/toolchain/cli-z42c-z42b.md` | MODIFY | z42b test/build 认清单各段、E0497、警告 |

**只读引用**：`src/compiler/z42c.driver/src/Main.z42`（`_build` 各阶段的语义来源）、`BuildPaths.z42`、`ExeDeps.z42`、
`src/toolchain/builder/core/builder.z42`、`builder_test.z42`、`docs/agent/rules/bootstrap-seed.md`。

## Out of Scope

- **第二步 `unify-driver-on-build-session`**（晚一个 nightly 之后）：增量 / indexed dist / cache / path 闭包 / analyzer
  代建搬进 pipeline，driver `_build` 与两个 workspace 循环改为 BuildSession + `DriverReporter` / `DriverDeploy`，删除旧副本。
  须等本 change 的新符号随 nightly 发布（bootstrap-seed「support 先行、晚一个 nightly 再 use」）。
- z42b 路径的增量缓存、多 `[[exe]]` 目标、z42b 与会话的 path 闭包统一。

## Open Questions

见 design.md「待 User 裁决」。
