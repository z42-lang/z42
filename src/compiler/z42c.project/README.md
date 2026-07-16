# z42c.project

## 职责
**编译器 zpkg 后端**（`Z42.Project` namespace）：`.zpkg` 读写 / builder / 缓存。**manifest 模型
（`.z42.toml` 解析 / 源文件发现 / 路径模板）已于 converge 阶段 2（2026-07-16）迁往共享库
`z42.project`（`Z42.Build.Project`），z42c 改引用它**——单一真相 manifest 模型，z42c 与 z42.build 共用。
占位 `ProjectSkeleton` 暂留（semantics/pipeline 仍引用）。
> 后端包名仍叫 `z42c.project`；改名 `z42c.zpkg` 是可选后续 refactor（Decision 1，非 converge 必需）。

## 核心文件（zpkg 后端 —— zpkg 构建链见下）
| 文件 | 职责 |
|------|------|
| `src/ProjectSkeleton.z42` | **过渡占位**：semantics/pipeline 仍引用；各自移植时移除 |

> **已迁往 `z42.project`（本包已删）**：`ProjectModel.z42`（ProjectManifest/WorkspaceManifest/DepEntry）
> / `ManifestLoader.z42`（toml→模型）/ `SourceDiscovery.z42`（源 glob 发现）/ `PathTemplate.z42`（路径模板）。
> 消费方（`z42c.driver/Main.z42`、`BuildPaths.z42`、`z42c.pipeline/WorkspaceBuild.z42`）改用
> `Z42.Build.Project` 的**组合式**模型（`pm.Project.*` / `pm.Sources.*` / `pm.Build.*`）。

## 依赖关系
→ z42c.ir, z42.toml, z42.io（stdlib）。Std / Std.IO 自动可用。

## zpkg 构建链（port-z42c-zpkg-build，2026-06-10）

| 文件 | 职责 |
|------|------|
| `src/SourceDiscovery.z42` | `[sources].include` glob（`**/` 递归）→ 排除 dist/.cache → Ordinal 排序 |
| `src/PackageTypes.z42` | ZbcFileZ / ZpkgExportZ / ZpkgDepZ / ZpkgFileZ（packed 子集模型）|
| `src/ZpkgBuilder.z42` | 组装：ns 去重 + exports FQ 幂等 + entry 四级自动检测 + Sha256Hex |
| `src/ZpkgWriter.z42` | packed 七段（META/STRS/NSPC/EXPT/DEPS/SIGS/MODS）；MODS 体复用 z42c.ir 段构建器（单源防漂移）|
| `src/ZpkgWriterIndexed.z42` | indexed 主文件写入器（add-indexed-zpkg-min-patch，zpkg 0.24）：packed 段面去 MODS 加 FILE（ns/src_rel/src_hash/fnCount/firstSig/zbc_hash）；SIGS 串显式入池（镜像 WriteSigEntries 读取面）|
| `src/ZpkgReader.z42` | packed 消费面子集：META/NSPC/SIGS/TSIG/IMPL + `ReadSourceHashes`（MODS 头 per-file (src,hash,ns) wire 读取工具；probe 消费已移 cache meta）|
| `src/ZpkgReader.z42`（续）| `ReadModuleTypes`（MODS 每模块 TYPE 段解析，reconcile P2）；`ReadModuleSigs` 现灌 P1 元数据（visibility/method_flags/min_arg/params_from/参数名/默认值）到 IrFunction stub |
| `src/TsigReconcile.z42` | **TSIG 对账重建**（unify P2）：从 TYPE/SIGS/IMPL 重建 ExportedModuleZ 与 TSIG oracle 逐字段（归一化后）对账；driver verb `reconcile-tsig`。删 TSIG（P3）前的无损重建安全网。机制见 [project.md](../../../docs/design/compiler/project.md#tsig-对账重建unify-type-metadata-p22026-07-11) |
| `src/CacheStore.z42` | 增量 cache meta 读写（add-file-level-incremental）：`<rel>.meta`（hash/ns/usedDepNs + D5a writer 残留 pool/labels，hex 编码；metaVersion/zbc/zpkg 三重 pin）+ 包级 `package.meta` 源清单；pin 不符/损坏 → null（宁 fresh 不误命中）|

对真 C# CLI：META/NSPC/EXPT/DEPS 逐字节相等；全段 byte-identical 待 TSIG/IMPL（follow-up）。
