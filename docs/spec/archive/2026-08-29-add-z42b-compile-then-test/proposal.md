# Proposal: z42b test 支持 compile-then-test（从 .z42.toml 先编译再跑）

> 状态：DRAFT（User 已确认 6.5，2026-08-29）| 类型：feat(toolchain)
> 前置：wire-z42b-host-build 已实质落地（`_hostCompiler` 已运行时注入 Z42cCompiler、
> `z42b build <toml>` 端到端可用）——本变更验证其已就绪。
> 上游程序：统一测试流水线归 z42b（archive `2026-08-29-unify-test-pipeline-z42b`）阶段② host 首刀。

## Why

`z42b test` 今天只接受**已编译**的 `.zbc/.zpkg`——`builder_test.z42:24-42` 对 toml 目标显式报
"building from … is pending wire-z42b-host-build"。但该前置已实质落地：`_hostCompiler()`
已用运行时反射注入 `Z42cCompiler`，`z42b build <toml>` 端到端可用（`_runVerb`）。因此 host 侧
「给一个测试工程的 `.z42.toml`，一步编译并跑其 `[Test]`」现在可以实现。

这是统一测试流水线**阶段②的 host 首刀**：让 `z42b test` 对工程与产物**统一入口**，消除调用方
「先 `z42b build` 再 `z42b test <dist.zpkg>`」的两步（正是 `xtask_test_targets.z42:257-268` 今天
做的两步）。平台 deploy（`--rid`）与 xtask 四平台 backend 委托是阶段②后续，不在本刀内。

## What Changes

1. `z42b test <project.z42.toml>`（及无 target 时默认 `z42.toml`）→ 复用 build 编译到
   `dist/<name>.zpkg`，再反射跑 `[Test]`（`Runner.RunModule`）。
2. `z42b test <file.zbc|.zpkg>` 路径**不变**（已编译产物直跑）。
3. `test` / `bench` 命令加 `--release` flag + `--rid` option（compile-then-test 的 profile / RID；
   默认 debug / host。`--rid` 仅让共享 build 助手可用，平台 deploy 仍 out-of-scope）。
4. 从 `_runVerb` 抽出 `_buildProject(r, mode) → BuildOutcome{Rc, Dist}` 共享 build 助手
   （build 分支与 test 复用，保留精确 exit code）。
5. **（扩展，User 裁决）Z42cCompiler 尊重工程 kind**：`ICompiler.CompileRequest` 加 `Kind` 字段，
   z42b 传入 manifest `Project.Kind`，`Z42cCompiler` 按 kind 编 exe（默认，需 Main）/ lib（免 Main）。
   —— 否则 z42b 注入的编译器 MVP 恒 kind=exe+Main，**测试工程（lib 式、无 Main、只有 `[Test]`）编不了**，
   compile-then-test 对真实测试工程无意义。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/toolchain/builder/core/builder_commands.z42` | MODIFY | 抽 `_buildProject(r, mode) → BuildOutcome`；`_runVerb` 改用之 |
| `src/toolchain/builder/core/builder_test.z42` | MODIFY | `_runModule`：toml/默认 → `_buildProject` → `RunModule`；产物路径不变；更新顶注 |
| `src/toolchain/builder/core/builder_cli.z42` | MODIFY | test/bench 加 `--release`/`--rid`；positional help 改 `<file.zbc|.zpkg|project.z42.toml>` |
| `src/libraries/z42.build/src/ICompiler.z42` | MODIFY | `CompileRequest` 加 `Kind` 字段（"exe"/"lib"） |
| `src/libraries/z42.build/src/Pipeline.z42` | MODIFY | `Compile` 相位构 `CompileRequest` 传 `ctx.Project.Kind` |
| `src/compiler/z42c.pipeline/src/Z42cCompiler.z42` | MODIFY | `inp.Kind = req.Kind`（"" → 默认 exe）；lib 免 Main |
| `src/compiler/z42c.pipeline/tests/z42ccompiler/z42ccompiler_tests.z42` | MODIFY | 3 构造点加 kind 参；新增 `test_compiles_lib_no_main` |
| `src/toolchain/builder/core/builder_hooks.z42` | MODIFY | hooks 合成 `CompileRequest` 传 `"exe"` |
| `src/tests/manifest-targets/compile-then-test/` | NEW | 最小 **kind=lib** test 工程（z42.toml + `src/Tests.z42` 自由函数 `[Test]` + `.gitignore`） |
| `scripts/test/xtask_test_fixtures.z42` | MODIFY | fixtures stage 加 `z42b test <toml>` compile-then-test smoke（assert rc==0） |
| `src/toolchain/builder/README.md` | MODIFY | `builder_test.z42` 行登记 compile-then-test 双形态 |
| `src/tests/manifest-targets/README.md` | MODIFY | 功能索引 + 核心文件登记 compile-then-test 夹具 |
| `docs/design/testing/cross-platform-testing.md` | MODIFY | 分阶段块拆 ②a（host compile-then-test 已落）/②b |

**只读引用：**
- `src/toolchain/builder/core/builder.z42` — `_orchestrate` / `_computeDirs` / `Dirs` 形状
- `src/libraries/z42.test/src/Runner.z42` — `RunModule(path, format) → int` 签名
- `scripts/test/xtask_test_targets.z42` — 今天「build 后 `z42b test <artifact>`」两步现状参照
- `docs/spec/archive/2026-08-29-unify-test-pipeline-z42b/design.md` — 阶段②分阶段依据（D4/D5）

## Out of Scope

- `z42b test --rid <platform>` 的 deploy+agent 路径（阶段②core，后续）。
- `xtask_test_platform.z42` / `xtask_test_targets.z42` 四平台 backend 委托 z42b（阶段②core）。
- test workload payload-only 打包 / 发布（archive design D6）。
- 非 stdlib 工程依赖的 host 解析——本刀覆盖 deps=stdlib 的常见情形；非 stdlib deps 走既有
  dist dep-bundle + Z42_LIBS，与今天 `z42b test <artifact>` 现状同路，不额外处理。

## Open Questions

- 无 target 时默认 `z42.toml`？→ **已定：是**（对齐 `z42b build` 默认，User 6.5 确认）。
