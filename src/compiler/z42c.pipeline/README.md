# z42c.pipeline

## 职责
编译管线编排（单文件 + 包级 Lexer→Parser→Sem→IR→Emit）+ 依赖扫描 + workspace 构建 + 文件级增量。后端三包的编排层，向下调 `z42c.semantics` 编译、`z42.package` 产 zpkg。

## 如何测试验证

```bash
xtask test compiler                 # tests/<unit>/（各带 *.z42.toml）的 [Test] 单元，全部 PASS 即通过
xtask test e2e --dir cross-zpkg     # tests/fixtures/cross-zpkg/：多包编译与链接夹具
xtask test e2e --dir multi-exe      # tests/fixtures/multi-exe/：一工程产多个 exe 的夹具
```

`tests/fixtures/` 放 harness 驱动的夹具工程，不是 `[Test]` 单元；约定见
[测试用例组织规范](../../../docs/internals/src/devinfra/test-layout.md)。

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/Z42cCompiler.z42` | `z42.build` `ICompiler` 实现（wire-z42b）——对外编译入口。BuildSession 的薄封装（`HostTarget`）；`req.Manifest` 必填（drop-legacy-compile-request 删掉了只看平铺字段的旧路径）|
| `src/BuildSession.z42` | 清单驱动的**一次包编译**（add-build-session）：`BuildOptions`（清单 / 源根 / `BuildRole` / 解析域 / 产物路径）→ 旋钮决议 → 源发现 → `[analyzers]` 解析（path 条目代建为 `AnalyzerChild`）→ `PackageCompile.Compile` → 写 packed + `.zsym`。**从不打印**，消息经 `IBuildReporter`（`CollectingReporter` 收集型实现）。今天只服务 z42b（`HostTarget`）；driver 已共用其中的 `ManifestKnobs` 与 `CompilerDomain`，其余步骤仍在 `Main._build` |
| `src/SdkLibs.z42` | **SDK 库的可见性**（add-sdk-libs）：`Plan` 算放行集（exe / lib = 按名声明的 SDK 库 + DEPS 传递闭包；analyzer = 编译器目录里基础解析域中没有的全部包），不放行的经 `MergeTier` 并入扫描 tier 的 `Hidden`；`ExtendDeclared` 把放行集并进声明白名单；`HiddenProviderOf` 给 E0494 点名未声明的 SDK 库。driver 与 BuildSession 共用。放行集为空 ⇒ 什么都不动（z42c 自建 byte-identical） |
| `src/ManifestKnobs.z42` | `[optimize]` / `[syntax]` / `[lints]` / pack / strip 决议为 `KnobResult`（错误行收集、不打印）。**唯一实现**：driver `Main._build` 与 BuildSession 都调它（unify-driver-knobs）|
| `src/ZpkgDeps.z42` | zpkg **DEPS 段**计算（deterministic-zpkg-deps）：每个模块 `UsedDepNs` 里带归属包的条目只记那个包，归属不明的按 ns 保守回落为全部提供包（`Z42C_TRACE_DEPS=1` 打印）；`using` 本身不贡献依赖；测试 / bench 目标加父包。规则见 internals `formats/zpkg.md` 的 DEPS 小节 |
| `src/Z42cReplCompiler.z42` | REPL 增量编译路径（累积声明 + 惰性符号世界）|
| `src/PackageCompile.z42` | 单包编译编排（源发现 → sem → emit → zpkg）|
| `src/CacheStore.z42` | 增量缓存落盘（`z42.io`/`z42.encoding`）；条目 meta（v5）= 源 hash + **名字级声明面指纹 `nsurf`** + 声明面标识符 `sident` + ns/useddep/token + writer 残留 |
| `src/GeneratorLoader.z42` | source generator 加载 + 运行 |
| `src/DepScan.z42` | **扫描编排 hub**（refactor-depscan-concern-split：854→409）：公开扫描 API（`Scan`/`ScanDirs`/`ScanDirsLazy`/`ExtendWithPackage`/`EnsurePackageLoaded`/`ReconcileCandidatesInNs`）+ 共享叶子辅助（`_nsIndexOf`/`_shortOf`/`_nameOfBasename`）。DependencyIndex / nsMap / 跨包类型世界（prelude-first + Ordinal 排序）；`ScanDirsLazy` REPL 惰性路径——`LazyReconWorld` 按包懒填、基类链按 ns 路由只解析引用闭包（lazy-type-world，O(引用) 不随库总量增长）。`ScanDirs` 的 `ZpkgReader.Open` + `TsigReconcile.Rebuild` 经 `DepScanCache` memo（见下）。各簇经 `DepScan._nsIndexOf`/`_shortOf` 单向委回 hub、零簇间边；`Z42C_TRACE_DEPSCAN=1` 把 `ScanDirs` 的 open / sigs / tsig 三段耗时打到 stderr（perf-tsig-reconcile-index 的测量口径）|
| `src/DepScanTypes.z42` | DepScan 产物数据类（refactor-depscan-concern-split 从 DepScan 拆出）：`DepScanResult`（扫描产物束：Index/nsMap/Exported/惰性 world/惰性 libOpened/类型→ns 索引）+ `NsIndexEntry`（ns 索引条目）。纯数据无逻辑 |
| `src/NsIndexCache.z42` | ns 索引 sidecar 缓存簇（拆自 DepScan）：`repl-scan-nsindex-cache` 落盘缓存「每包→命名空间/类型」，命中免 open-all。指纹（`_libsFingerprint`/`_mtimeMs`）+ 读/写索引（`_readNsIndex`/`_writeNsIndex`）+ 从索引建 scan（`_scanFromIndex`）+ 类型→ns 提取/回填（`_extractNsTypes`/`_fillTypeMap`）|
| `src/ZpkgPathSort.z42` | zpkg 路径确定性枚举 + 排序簇（拆自 DepScan）：多目录合并 + prelude-first + Ordinal 排序（`_sortedZpkgsMulti`/`_sortZpkgKeys`）+ 同名去重（`_dedupByBasename`）+ DepIndex 准入（`_allowedForIndex`）。common-pitfalls §1 加载顺序确定性落点 |
| `src/DepReconcile.z42` | 惰性包加载 + 候选 reconcile 辅助簇（拆自 DepScan）：`_loadOpenedPackage`（按需 Rebuild+DepIndex 并入 scan）+ completer 候选类型追加（`_inCands`/`_typeInExported`/`_appendExportedClass`）|
| `src/DepScanCache.z42` | **F2** 进程级 zpkg memo：按 path 缓存打开的 `ZpkgInfo` + 该包 `TsigReconcile.Rebuild` 结果，把 workspace 逐成员重复解同一 zpkg 的 O(N²) 降成 O(N)（DepScan -71%）。算法/排序/过滤不变 → 字节不动点天然成立；正确性依赖「进程内 path→内容稳定」不变式（头注） |
| `src/WorkspaceBuild.z42` | workspace 成员发现 + 拓扑序 + per-member 布局（WsPlan）|
| `src/IncrementalBuild.z42` | 文件级增量 probe（add-file-level-incremental）：`ProbeFiles` 种子（hash/条目·pin/包级源清单）+ `Close` **名字级**失效闭包（incr-name-level-invalidation：已变名字集 → token 命中即失效 → 只有「声明面提到别人的已变名字」才继续传播；无 N² 边矩阵）；`Z42_INCR_DEBUG` 打 `[name-changed]`/`[invalidated] … uses-changed-name X`/`[spread]`；单测见 `tests/incremental/` |

## 入口点
`Z42.Pipeline`（命名空间）。

## 依赖关系
→ z42c.core, z42c.syntax, z42c.semantics, z42.package, z42.project, z42.build（ICompiler 接口）。stdlib 自动可用。
