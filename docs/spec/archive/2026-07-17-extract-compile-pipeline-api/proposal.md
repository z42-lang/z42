# Proposal: 抽取包编译核心为库级 API（extract-compile-pipeline-api）

> 状态：已实施（2026-07-17；PackageCompile 落地，self-host 7/7 逐字节 + 4 单测绿）
> 子系统：`compiler`（z42c.driver + z42c.pipeline）
> 前置：无（`converge-z42c-onto-z42-project` 已落地，composed `ProjectManifest` 可用）
> 下游：`wire-z42b-host-build`（其 D2 的 `Z42cCompiler : ICompiler` 包装本变更抽出的核心）

## Why

`z42b`（构建编排器）要**在进程内**调编译器（不 fork `z42c` 子进程、不依赖 z42vm 在
PATH、诊断以结构返回而非解析 stdout——见 [`ICompiler.z42`](../../../src/libraries/z42.build/src/ICompiler.z42)
的设计动机）。但**当前没有库级编译入口**：单包编译核心整个嵌在
[`z42c.driver/Main.z42::_build`](../../../src/compiler/z42c.driver/src/Main.z42)（约 200 行），
与 CLI 关注点（`ConsoleError` / `ExitCode` / 读 `Z42_LIBS` env / 增量 probe / 落盘）耦合。

`wire-z42b` 的 `Z42cCompiler` 无从复用它 → wire-z42b 被本变更阻塞（2026-07-17 核实：
`z42c.pipeline` 无 `CompileInMemory`/`CompilePackage`，`Z42cCompiler.z42` 未创建）。

本变更把 `_build` 的**纯编译核心**（源+依赖 → 组装好的 in-memory `ZpkgFileZ` + 结构化诊断）
抽到 `z42c.pipeline`，让 `z42c.driver` 与 `Z42cCompiler` **共享同一实现**。

## What Changes

1. **`z42c.pipeline` 新增 `PackageCompile`**：纯函数式核心
   `Compile(inputs) -> CompileArtifacts`——输入内存源文本 + 依赖解析结果 + manifest 派生字段，
   输出组装好的 `ZpkgFileZ z` + `ZbcFileZ[] mods` + 诊断（**不落盘、不写 Console、不返回 ExitCode**）。
2. **`z42c.driver/_build` 瘦身为薄 CLI 包装**：arg 解析 / manifest load / 源读入 / 增量 probe /
   落盘（packed sidecar vs indexed dist）/ `ConsoleError`+`ExitCode` 映射**留在 driver**；
   编译核心委托 `PackageCompile.Compile`。**产物字节完全不变**（同一核心逻辑 → self-host
   gen1==gen2 逐字节门禁是硬验证）。
3. **可选 bytes 便捷入口**：`PackageCompile.ToPackedBytes(artifacts, isRelease) -> {main, sym}`
   给 `Z42cCompiler` 直接拿字节写 `OutputZpkg`（driver 走自己的 indexed/packed 分支）。

## What This Does NOT Do（明确划走）

- **不改产物字节**：这是纯 refactor（抽取 + 委托），非行为变更。任何字节漂移 = bug。
- **不做「依赖 blob 内存化」**：wire-z42b design D2 设想的 `depBlobs[]`（依赖 zpkg 以字节传入）
  **降级为可选后续**——依赖 zpkg 恒在磁盘（`Z42_LIBS` / dist 目录），核心仍经
  `DepScan.ScanDirs(libsDirs)` 从磁盘解析依赖，`Z42cCompiler` 传其依赖父目录即可达成
  「in-process、不 fork z42c」的真实目标。真·内存 blob provider（依赖不在磁盘的场景）留
  `extract-compile-pipeline-api-future-blob-provider`，无现实需求不预建（见 design D3）。
- **不接 z42b / 不改 launcher / 不动 CI**：全部归 `wire-z42b-host-build`。本变更只在
  `compiler` 子系统内落地，wire-z42b 之后才消费。
- **不动增量语义**：增量 probe/cache 仍是 driver 的 dev-loop 关注点；核心把 `cus`（已 parse）
  与 `cachedMods` 作为**输入**接收（driver 传增量产物，`Z42cCompiler` 传全量 parse + null）。

## Scope（允许改动的文件）

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/compiler/z42c.pipeline/src/PackageCompile.z42` | NEW | 编译核心 `Compile` + `CompileArtifacts` + `ToPackedBytes` |
| `src/compiler/z42c.driver/src/Main.z42` | MODIFY | `_build` 委托 `PackageCompile.Compile`，只留 CLI 关注点 |
| `src/compiler/z42c.pipeline/z42c.pipeline.z42.toml` | 可能 MODIFY | 若核心用到 driver 当前独有的 dep（应无——用的都是 semantics/ir 已依赖项）|
| `src/compiler/z42c.pipeline/tests/**` | NEW | `PackageCompile` 单测（编一个 hello 包 → 断言 mods/entry/诊断）|
| ~~`docs/design/compiler/compiler-architecture.md`~~ | — | docs/design 已冻结（doc-system D2，SoT 迁 book）；核心/CLI 分层 + 数据流由本 change 的 [design.md](design.md) 承载（实现原理 doc）|
| `docs/spec/changes/ACTIVE.md` | MODIFY | 登记 `compiler` 占用 |

## Out of Scope
- wire-z42b 的一切（Z42cCompiler、z42.build 接入、launcher、CI、apphost）。
- 依赖 blob 内存化 provider；workspace 多成员编译核心（`WorkspaceBuild` 已是库级，不动）。

## GREEN 判据
- self-host 7/7 gen1==gen2 **逐字节不变**（refactor 无字节漂移的权威证明）。
- 全 `[Test]` 绿 + 新增 `PackageCompile` 单测绿。
- `xtask test compiler` exit=0。
