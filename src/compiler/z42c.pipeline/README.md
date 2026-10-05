# z42c.pipeline

## 职责
编译管线编排（单文件 + 包级 Lexer→Parser→Sem→IR→Emit）+ 依赖扫描 + workspace 构建 + 文件级增量。后端三包的编排层，向下调 `z42c.semantics` 编译、`z42.package` 产 zpkg。

## 功能索引
命名空间 `Z42.Pipeline`。

| 功能 | 入口 |
|------|------|
| 对外编译入口（`ICompiler` 实现） | `Z42cCompiler` |
| REPL 增量编译（`IReplCompiler` 实现） | `Z42cReplCompiler` |
| 清单驱动的一次包编译 | `BuildSession` / `BuildOptions` |
| 单包编译编排 | `PackageCompile.Compile` |
| 依赖扫描 / 跨包类型世界 | `DepScan.Scan` / `ScanDirs` / `ScanDirsLazy` |
| workspace 成员发现 + 拓扑序 + 布局 | `WorkspaceBuild`（`WsPlan`）|
| 文件级增量 probe / 失效闭包 | `IncrementalBuild.ProbeFiles` / `Close` |

## 如何测试验证

```bash
./xtask test compiler                 # tests/<unit>/（各带 *.z42.toml）的 [Test] 单元，全部 PASS 即通过
./xtask test e2e --dir cross-zpkg     # tests/fixtures/cross-zpkg/：多包编译与链接夹具
./xtask test e2e --dir multi-exe      # tests/fixtures/multi-exe/：一工程产多个 exe 的夹具
```

`tests/fixtures/` 放 harness 驱动的夹具工程，不是 `[Test]` 单元；约定见
[测试用例组织规范](../../../docs/internals/src/devinfra/test-layout.md)。

## 关联文档
- 设计 / 机制：[self-hosting.md](../../../docs/internals/src/compiler/self-hosting.md)、[compiler-fingerprint.md](../../../docs/internals/src/compiler/compiler-fingerprint.md)、[project-model.md](../../../docs/internals/src/compiler/project-model.md)

## 核心文件
### 编译入口与会话
| 文件 | 职责 |
|------|------|
| `src/Z42cCompiler.z42` | `z42.build` `ICompiler` 实现——对外编译入口。BuildSession 的薄封装（`HostTarget`）；`req.Manifest` 必填 |
| `src/Z42cReplCompiler.z42` | `IReplCompiler` 实现：REPL 增量编译路径（累积声明 + 惰性符号世界）|
| `src/BuildSession.z42` | 清单驱动的**一次包编译**：`BuildOptions`（清单 / 源根 / `BuildRole` / 解析域 / 产物路径）→ 旋钮决议 → 源发现 → `[analyzers]` 解析（path 条目代建为 `AnalyzerChild`）→ `PackageCompile.Compile` → 写 packed + `.zsym`。**从不打印**，消息经 `IBuildReporter`（`CollectingReporter` 收集型实现）。目前由 z42b（`HostTarget`）使用；driver 共用其中的 `ManifestKnobs` 与 `CompilerDomain`，其余步骤仍在 driver 的 `Main._build`。同文件含 `CompilerDomain`（编译器域解析域）|
| `src/PackageCompile.z42` | 单包编译编排（源发现 → sem → emit → zpkg）|
| `src/CtorKnownFixup.z42` | 整包装配后的 `ObjNew.CtorKnown` 置位（区分「零构造器类」与「构造器名解析不到」）|
| `src/GeneratorLoader.z42` | source generator 加载 + 运行 |
| `src/ManifestKnobs.z42` | `[optimize]` / `[syntax]` / `[lints]` / pack / strip 决议为 `KnobResult`（错误行收集、不打印）。**唯一实现**：driver 与 BuildSession 共用 |
| `src/SdkLibs.z42` | **SDK 库的可见性**：`Plan` 算放行集（exe / lib = 按名声明的 SDK 库 + DEPS 传递闭包；analyzer = 编译器目录里基础解析域中没有的全部包），不放行的经 `MergeTier` 并入扫描 tier 的 `Hidden`；`ExtendDeclared` 把放行集并进声明白名单；`HiddenProviderOf` 给 E0494 点名未声明的 SDK 库。driver 与 BuildSession 共用；放行集为空 ⇒ 什么都不动 |
| `src/ZpkgDeps.z42` | zpkg **DEPS 段**计算：每个模块 `UsedDepNs` 里带归属包的条目只记那个包，归属不明的按 ns 保守回落为全部提供包（`Z42C_TRACE_DEPS=1` 打印）；`using` 本身不贡献依赖；测试 / bench 目标加父包。规则见 internals `formats/zpkg.md` 的 DEPS 小节 |

### 依赖扫描
| 文件 | 职责 |
|------|------|
| `src/DepScan.z42` | **扫描编排 hub**：公开扫描 API（`Scan`/`ScanDirs`/`ScanDirsLazy`/`ExtendWithPackage`/`EnsurePackageLoaded`/`ReconcileCandidatesInNs`）+ 共享叶子辅助。DependencyIndex / nsMap / 跨包类型世界（prelude-first + Ordinal 排序）；`ScanDirsLazy` 为 REPL 惰性路径——`LazyReconWorld` 按包懒填、基类链按 ns 路由只解析引用闭包。`ScanDirs` 的 `ZpkgReader.Open` + `TsigReconcile.Rebuild` 经 `DepScanCache` memo。`Z42C_TRACE_DEPSCAN=1` 把 open / sigs / tsig 三段耗时打到 stderr |
| `src/DepScanTypes.z42` | 产物数据类：`DepScanResult`（Index/nsMap/Exported/惰性 world/惰性 libOpened/类型→ns 索引）+ `NsIndexEntry`。纯数据无逻辑 |
| `src/NsIndexCache.z42` | ns 索引 sidecar 缓存：落盘缓存「每包→命名空间/类型」，命中免 open-all。指纹 + 读/写索引 + 从索引建 scan + 类型→ns 提取/回填 |
| `src/ZpkgPathSort.z42` | zpkg 路径确定性枚举 + 排序：多目录合并 + prelude-first + Ordinal 排序 + 同名去重（`_dedupByBasename`）+ DepIndex 准入（`_allowedForIndex`）|
| `src/DepReconcile.z42` | 惰性包加载 + 候选 reconcile 辅助：`_loadOpenedPackage`（按需 Rebuild + DepIndex 并入 scan）+ completer 候选类型追加 |
| `src/DepScanCache.z42` | 进程级 zpkg memo：按 path 缓存打开的 `ZpkgInfo` + 该包 `TsigReconcile.Rebuild` 结果，把 workspace 逐成员重复解同一 zpkg 的 O(N²) 降成 O(N)；正确性依赖「进程内 path→内容稳定」不变式 |
| `src/WsTier.z42` | 依赖解析分档：workspace per-member 布局下，成员 dist 档与外部档（`--compile-libs`，否则 `Z42_LIBS`）分开解析，成员 dist 里 colocate 的载荷副本不作解析源 |

### workspace 与增量
| 文件 | 职责 |
|------|------|
| `src/WorkspaceBuild.z42` | workspace 成员发现 + 拓扑序 + per-member 布局（`WsPlan`）|
| `src/IncrementalBuild.z42` | 文件级增量 probe：`ProbeFiles` 种子（hash / 条目·pin / 包级源清单）+ `Close` **名字级**失效闭包（已变名字集 → token 命中即失效 → 只有「声明面提到别人的已变名字」才继续传播）；`Z42_INCR_DEBUG` 打 `[name-changed]` / `[invalidated]` / `[spread]`；单测见 `tests/incremental/` |
| `src/CacheStore.z42` | 增量缓存落盘（`z42.io`/`z42.encoding`）；条目 meta = 源 hash + **名字级声明面指纹 `nsurf`** + 声明面标识符 `sident` + ns/useddep/token + writer 残留 |
| `src/DepIdentity.z42` | 依赖身份：增量缓存键里「这次编译看到的依赖」一维；每包取 BLID，无 BLID 回落整文件 Murmur3，使依赖接口变化能使消费方 cache 失效 |
| `src/CompilerFingerprint.z42` | 编译器语义指纹：`Entries` 列表的内容哈希；每条语义变更追加一行 slug。规则见 [compiler-fingerprint.md](../../../docs/internals/src/compiler/compiler-fingerprint.md) |

## 依赖关系
`z42c.core`、`z42c.syntax`、`z42c.semantics`、`z42.package`、`z42.project`、`z42.build`（`ICompiler` / `IReplCompiler` 接口，无环）、`z42.io` + `z42.encoding`（CacheStore）。stdlib 自动可用。
