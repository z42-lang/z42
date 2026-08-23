# z42c.pipeline

## 职责
镜像 C# [z42.Pipeline](../../compiler/z42.Pipeline/README.md)：编译管线编排（单文件 + 包级 Lexer→Parser→Sem→IR→Emit）。**B0 骨架：占位类型 `PipelineSkeleton`**（引用全部 5 个直接依赖，验证最深多依赖节点跨包编译）；真实编排 → 端到端 build 待 0.3.9。

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/PipelineSkeleton.z42` | 占位（`namespace Z42.Pipeline`，引用 core/syntax/semantics/ir/project）|
| `src/DepScan.z42` | **扫描编排 hub**（refactor-depscan-concern-split：854→409）：公开扫描 API（`Scan`/`ScanDirs`/`ScanDirsLazy`/`ExtendWithPackage`/`EnsurePackageLoaded`/`ReconcileCandidatesInNs`）+ 共享叶子辅助（`_nsIndexOf`/`_shortOf`/`_nameOfBasename`）。DependencyIndex / nsMap / 跨包类型世界（prelude-first + Ordinal 排序）；`ScanDirsLazy` REPL 惰性路径——`LazyReconWorld` 按包懒填、基类链按 ns 路由只解析引用闭包（lazy-type-world，O(引用) 不随库总量增长）。`ScanDirs` 的 `ZpkgReader.Open` + `TsigReconcile.Rebuild` 经 `DepScanCache` memo（见下）。各簇经 `DepScan._nsIndexOf`/`_shortOf` 单向委回 hub、零簇间边 |
| `src/DepScanTypes.z42` | DepScan 产物数据类（refactor-depscan-concern-split 从 DepScan 拆出）：`DepScanResult`（扫描产物束：Index/nsMap/Exported/惰性 world/惰性 libOpened/类型→ns 索引）+ `NsIndexEntry`（ns 索引条目）。纯数据无逻辑 |
| `src/NsIndexCache.z42` | ns 索引 sidecar 缓存簇（拆自 DepScan）：`repl-scan-nsindex-cache` 落盘缓存「每包→命名空间/类型」，命中免 open-all。指纹（`_libsFingerprint`/`_mtimeMs`）+ 读/写索引（`_readNsIndex`/`_writeNsIndex`）+ 从索引建 scan（`_scanFromIndex`）+ 类型→ns 提取/回填（`_extractNsTypes`/`_fillTypeMap`）|
| `src/ZpkgPathSort.z42` | zpkg 路径确定性枚举 + 排序簇（拆自 DepScan）：多目录合并 + prelude-first + Ordinal 排序（`_sortedZpkgsMulti`/`_sortZpkgKeys`）+ 同名去重（`_dedupByBasename`）+ DepIndex 准入（`_allowedForIndex`）。common-pitfalls §1 加载顺序确定性落点 |
| `src/DepReconcile.z42` | 惰性包加载 + 候选 reconcile 辅助簇（拆自 DepScan）：`_loadOpenedPackage`（按需 Rebuild+DepIndex 并入 scan）+ completer 候选类型追加（`_inCands`/`_typeInExported`/`_appendExportedClass`）|
| `src/DepScanCache.z42` | **F2** 进程级 zpkg memo：按 path 缓存打开的 `ZpkgInfo` + 该包 `TsigReconcile.Rebuild` 结果，把 workspace 逐成员重复解同一 zpkg 的 O(N²) 降成 O(N)（DepScan -71%）。算法/排序/过滤不变 → 字节不动点天然成立；正确性依赖「进程内 path→内容稳定」不变式（头注） |
| `src/WorkspaceBuild.z42` | workspace 成员发现 + 拓扑序 + per-member 布局（WsPlan）|
| `src/IncrementalBuild.z42` | 文件级增量 probe（add-file-level-incremental）：`ProbeFiles` 种子（hash/条目·pin/包级源清单）+ `Close` token 保守边传递闭包（标识符 token ∩ 包内定义名）；`Z42_INCR_DEBUG` 种子+传播链；单测见 `tests/incremental/` |

## 入口点
`Z42.Pipeline`（命名空间）。

## 依赖关系
→ z42c.core, z42c.syntax, z42c.semantics, z42c.ir, z42c.project。stdlib 自动可用。
