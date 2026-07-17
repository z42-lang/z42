# Design: 接入 z42b host build

> 把 PARKED 的 z42.project / z42.build / z42b 接入编译，注入真实编译器，launcher 转发。
> 前瞻架构见 [build-orchestrator.md](../../../design/toolchain/build-orchestrator.md)。

## Architecture

```
z42 build app.z42.toml                              [launcher 转发]
  └─ spawn: z42vm z42b.zpkg -- build app.z42.toml    [AddSpawn spawn-leaf]
       └─ Z42Builder._cmdBuild
            ManifestLoader.Load(toml)               [z42.project]
            → Target(host rid, profile)
            → _orchestrate → Pipeline.Run(ctx)       [z42.build 八相位]
                 Compile 相位:
                   ctx.Compiler.Compile(req)         [ICompiler]
                     = Z42cCompiler.Compile          [z42c.pipeline, in-process]
                         CompileInMemory(...)         [extract-compile-pipeline-api]
                         → CompileResult.Bytes (zpkg)
                         → write OutputZpkg
```

依赖方向（无环）：
```
z42.project → (z42.core, z42.toml)
z42.build   → z42.project
z42c.pipeline（Z42cCompiler）→ z42.build（仅接口）
z42b        → z42.build + z42.project + z42c.pipeline（取 Z42cCompiler 实现）
launcher    → spawn z42b.zpkg（进程隔离，无编译期依赖）
```

## Decisions

### D1: launcher → z42b 用 AddSpawn spawn-leaf（不在 launcher 内 in-process 调）

**问题**：launcher 如何把 `new/build/publish/export` 交给 z42b？

**选项**：A. spawn-leaf（`z42vm z42b.zpkg -- <verb>`）；B. SDK programs 目录发现（launcher-command-dispatch
第二层，启动扫 `programs/` 注册）；C. launcher in-process 加载 z42b.zpkg。

**决定**：选 **A（AddSpawn spawn-leaf）**。复用 `add-workload-command-dispatch` 定义的
`SubcommandRouter.AddSpawn(verb, desc, zpkgPath)` 原语（若该 change 未先落，本变更前借该方法到
z42.cli）。理由：最小、进程隔离（z42b 崩不连累 launcher）、与 workload 命令同机制；B（SDK programs
通用发现）是更大的架构，留 launcher-command-dispatch 推进；C 需 in-process 动态加载（runtime builtin，
更重）。launcher 显式注册 z42b 的 5 个 verb 为 spawn-leaf，`z42 -h` 自动列出。

### D2: Z42cCompiler 住 z42c.pipeline，包装 PackageCompile

> **已实施（2026-07-17，阶段 2a）**：`Z42cCompiler : ICompiler`
> （`src/compiler/z42c.pipeline/src/Z42cCompiler.z42`）落地，z42c.pipeline 加 dep z42.build（仅接口，无环）。
> 3 个端到端单测（编 hello→app.zpkg / 无 Main 报 missing / 空源报 no-sources）+ self-host 7/7 逐字节绿。
> seed 轴：本改动 `using Z42.Build` 依赖 e58de18 nightly（已含 z42.build）方能自举——故须在阶段 1 之后。

**问题**：ICompiler 实现放哪、调什么？

> **已精化（2026-07-17，extract-compile-pipeline-api 落地）**：核心 API 定为
> `PackageCompile.Compile(CompileInputs) -> CompileArtifacts`（住 z42c.pipeline），**依赖从磁盘
> `LibsDirs` 解析、非内存 blob**（见 extract design D3——依赖 zpkg 恒在磁盘，blob 化收益零风险非零，
> 降级为后续）。原「`CompileInMemory(depBlobs)`」设想作废，下面已更新。

**决定**：住 `z42c.pipeline`（依赖 z42.build 仅接口，符合 ICompiler.z42 的 DIP）。`Compile(req)`：
1. `SourceDiscovery.Discover(req.SourceDir, ...)` 取源 → 读 texts + 算 srcHashes。
2. `IrDump.ParseAll(texts, files, n)` → `Cus`（全量；无增量 → `CachedMods=null`）。
3. 构 `CompileInputs`：`LibsDirs` = `req.Deps` 各 zpkg 的**父目录集**（去重）；`DeclaredDeps` = 依赖名；
   `Name/Version/Kind/HasEntry/Entry` 由 req 隐含（app 编译 kind="exe"）；`IsRelease` = profile=="release"。
4. `PackageCompile.Compile(inputs)` → `CompileArtifacts`。
5. `art.ErrorCount>0` → `CompileResult("", "", false, art.Cms 聚合诊断文本)`；否则
   `ZpkgWriterZ.WritePackedWithSidecar(art.Z, isRelease)` → 写 `req.OutputZpkg`（+`.zsym` sidecar）→
   `CompileResult(OutputZpkg, zsym, true, "")`。

不 fork z42c 子进程（in-process），不依赖 z42vm 在 PATH。核心已落地（`PackageCompile`），Z42cCompiler
仅剩「Discover→parse→构 inputs→Compile→写盘」薄适配。

### D3: z42c 侧只加适配，不收敛 manifest 模型（本变更）

z42c.pipeline 的现有 `_build`/CompilePackage 仍用 z42c.project 的 flat ProjectManifest。Z42cCompiler
是**新增的薄适配层**，不改现有路径，不删 z42c.project 任何东西。flat→composed 的收敛是独立高风险变更
（`converge-z42c-onto-z42-project`），不混入。

### D4: Dirs 路径约定 + Inputs.Deps 解析

- `_computeDirs`（已在 builder.z42）：output_dir 默认 `<source>/artifacts/<name>/<profile>`，
  Intermediate=`<outBase>/build`、Dist=`<outBase>/dist`（镜像 project.md，接入时与 z42c dist 约定对齐校验）。
- `Inputs.Deps`（现 builder.z42 返空）：接入时填 —— 扫 `Z42_LIBS`（VM 已回写，见 fix 08e8…前的
  Z42_LIBS commit）+ manifest 声明依赖 → dep zpkg 路径。Z42cCompiler 据此读 blobs。

### D5: desktop publish

**问题**：`z42b publish`（desktop）的 apphost 产出，迁移 launcher_export 还是暂调现有？

**决定（倾向）**：本变更 publish **暂只 build（产 app.zpkg），apphost 落地复用现有 launcher
`Apphost.Produce`**；publish/export 的完整迁移（launcher_export → z42b workload 尾相位）独立推进
（migrate-publish-export-to-z42b）。避免本变更同时吞下平台 workload 的大改面。

## Implementation Notes
- 拓扑构建序：z42.project → z42.build → z42c.*（含 Z42cCompiler）→ z42.builder。workspace
  default-members 顺序 + xtask build 序对齐。
- z42b apphost：同 launcher 模式（z42b.zpkg + apphost stub patch → 仓库根/SDK bin/z42b）。
- 接入后 GREEN：`z42 new demo && cd demo && z42 build` 产 `dist/demo.zpkg`，`z42 run` 输出 Hello。

## Testing Strategy
- z42.project：ManifestLoader/SourceDiscovery/PathTemplate 单测（[Test]，接入后随包测）。
- z42.build：PipelineContext fs/guard 单测。
- Z42cCompiler：内存编译一个 hello 工程 → app.zpkg 可被 z42vm 跑（端到端）。
- z42b：`z42b new` 脚手架文件断言；`z42b build` 端到端产 zpkg；`z42 build` 经 launcher 转发可用。
- 全量 GREEN（接入后）：`z42 xtask.zpkg test` 全 stage 不回归。

## Deferred
- wire-z42b-future-deps-resolve：Inputs.Deps 的完整多源解析（workspace dist + Z42_LIBS）随真实多依赖工程补。
- wire-z42b-future-icompiler-microlib：ICompiler 抽到中立微库（解 z42b→z42c.pipeline 重依赖），见 ICompiler.z42 计划重构。
