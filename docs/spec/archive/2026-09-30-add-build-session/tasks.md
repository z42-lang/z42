# Tasks: 统一的包构建会话（BuildSession）—— 第一步

> 状态：🟢 已完成（proposal / spec / design 已于 2026-09-30 获 User 批准；实施偏差见 design.md Implementation Notes） | 创建：2026-09-30

## 进度概览
- [x] 阶段 0: 修复前红（smoke + 单测在今天的 Z42cCompiler 上失败）
- [x] 阶段 1: ManifestKnobs + BuildSession（support，无引用方）
- [x] 阶段 2: Z42cCompiler / Pipeline / builder_dev_targets 改接（用户可见）
- [x] 阶段 3: 文档 + GREEN

## 阶段 0
- [x] 0.1 夹具 `src/tests/z42b/manifest-sections/`（[syntax] 关特性；未声明依赖 / 警告见 design 偏差）
- [x] 0.2 `xtask_test_targets.z42` smoke；`tests/buildsession`、`tests/z42ccompiler` 新单测；记录修复前失败形态

## 阶段 1
- [x] 1.1 `ManifestKnobs.z42`：opt / syntax / lints / pack / strip 决议（语义逐项对照 `Main._build` 385-427、330-358）
- [x] 1.2 `BuildSession.z42`：BuildOptions / BuildResult / IBuildReporter / BuildRole / Run（HostTarget 路径；IDeployStep 延到第二步）
- [x] 1.3 `PackageCompile.z42` 删 `HasPkgContext`

## 阶段 2
- [x] 2.1 `ICompiler.z42`：`CompileRequest.Manifest`、`CompileResult.Warnings`（body 字段；ProjectDir 复用 SourceDir）
- [x] 2.2 `Z42cCompiler.z42`：Manifest 非空转 BuildSession（为 null 时退回平铺字段）
- [x] 2.3 `Pipeline.z42`：传清单、strip 按 profile、`ctx.Warn` 呈现警告
- [x] 2.4 `builder_dev_targets.z42`：派生清单补拷 optimize / syntax / lints / analyzers（entry 本就经 ProjectInfo）；父包无源时不列依赖

## 阶段 3
- [x] 3.1 文档：architecture.md、toolchain/z42b.md、reference cli-z42c-z42b.md、pipeline README
- [x] 3.2 `xtask test` 全绿；归档
