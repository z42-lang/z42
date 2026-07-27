# Design: 抽取包编译核心为库级 API

> 状态：**已实施**（2026-07-17）——`PackageCompile`（`src/compiler/z42c.pipeline/src/PackageCompile.z42`）
> 落地，`_build` 委托；self-host 7/7 gen1==gen2 逐字节不变 + 4 单测绿。本 design 即该分层的**实现原理 doc**
> （docs/design 已冻结，doc-system D2）。
>
> 把 [`z42c.driver/_build`](../../../src/compiler/z42c.driver/src/Main.z42) 的纯编译核心
> 抽到 `z42c.pipeline`，供 driver CLI 与（下游 wire-z42b 的）`Z42cCompiler` 共享。

## 现状：编译核心在哪、耦合了什么

`_build(tomlPath, isRelease, outputDirOverride, libsDirsOverride, libsDirsCount, noIncremental)`
线性走完（`src/compiler/z42c.driver/src/Main.z42`）：

```
① manifest load        ManifestLoader.Load(toml) → ProjectManifest（composed）
② 源发现               SourceDiscovery.Discover(projectDir, pm.Sources.Include)
③ libsDirs 决议        libsDirsOverride | Z42_LIBS env               ← CLI 关注点
④ 源读入 + hash        File.ReadAllText ×N + Sha256Hex ×N
⑤ 增量 probe           IncrementalDriver.Prepare → 全命中即 preserved  ← CLI/dev-loop 关注点
⑥ 依赖解析             DepScan.ScanDirs(libsDirs) → DepIndex + usings 聚合 → ImportedSymbolLoader.Load
⑦ parse-all            IncrementalDriver.ParseAllTk（或增量复用 prep.Cus）
⑧ 包级编译             IrDump.BuildPackageCus(texts, srcs, cus, depIndex, imported, isRelease, cachedMods)
                        → CompiledModuleZ[]（每文件 IrModule + Exported + 诊断）
⑨ 诊断门               cm.ErrorCount>0 → ConsoleError + return BuildError  ← CLI 关注点
⑩ 组装                 ZbcFileZ[] 拼装 → ZpkgBuilder.BuildDependencyMap → BuildPackedD → ZpkgFileZ z
⑪ entry 校验           z.Entry == "<ambiguous>"/"" → ConsoleError + BuildError  ← CLI 关注点
⑫ 落盘                 packed → WritePackedWithSidecar + File.WriteAllBytes
                        indexed → _writeIndexedDist                    ← CLI/落盘关注点
⑬ cache 落盘           单工程模式逐文件写 .zbc                          ← CLI/dev-loop 关注点
```

**纯核心 = ⑥⑧⑩（+⑦的产物 cus 作输入）**：源文本 + 依赖 → 组装好的 in-memory `ZpkgFileZ` +
结构化诊断。其余（①②③④⑤⑨⑪⑫⑬）是 CLI / dev-loop / 落盘 / 呈现，**留在 driver**。

`Z42cCompiler` 需要的正是这个纯核心：给它 `SourceDir`（→②④）、`Deps`（→⑥ 的 libsDirs）、
`Profile`（→ isRelease/isPacked）、`OutputZpkg`（→⑫ 自己写），它内部只跑 ⑥⑦⑧⑩ 再落盘。

## Decisions

### D1: 核心边界 —— 「源+依赖 → 组装好的 `ZpkgFileZ` + 诊断」，不落盘不呈现

**决定**：`PackageCompile.Compile` 吃**已读入的内存源** + **manifest 派生字段** + **依赖解析所需的
libsDirs**，吐**组装好的 in-memory 包 + 诊断**。落盘（packed sidecar / indexed dist）、`ConsoleError`、
`ExitCode`、增量 probe、cache 落盘**全部留在调用方**。

理由：
- 落盘有两种形态（packed vs indexed，⑫），且 indexed 走 `_writeIndexedDist`（多文件 + FILE 目录）。
  核心产 `ZpkgFileZ z` + `ZbcFileZ[] mods`（in-memory），让**调用方决定怎么写**——driver 走
  packed/indexed 分支，`Z42cCompiler` 恒 packed。核心不吞落盘 = 两个调用方零重复、零 if-CLI 分叉。
- 诊断以 `CompileArtifacts.Diagnostics[]` + `ErrorCount` 结构返回（`CompiledModuleZ` 已带
  `DiagMsgs`/`ErrorCount`，核心聚合即可）；driver 映射到 `ConsoleError`，`Z42cCompiler` 映射到
  `CompileResult.Diagnostics`。**诊断绝不在核心里 `ConsoleError`**（否则 z42b 拿不到结构、还会污染 stdout）。

**API 形状**（伪代码，z42 无 out 参数 → 用结果 record 承载）：

```z42
namespace Z42.Pipeline;

// 编译核心的输入（纯数据；调用方备齐内存源 + 依赖目录 + manifest 派生字段）。
public sealed class CompileInputs {
    public string[] Texts; public string[] Files; public string[] SrcHashes; public int SrcCount;
    public CompilationUnit[] Cus;          // 已 parse（driver 传增量复用；Z42cCompiler 传全量 ParseAllTk）
    public IrModule[] CachedMods;          // 增量 cached IrModule（null = 全量）
    // manifest 派生（composed ProjectManifest → 这些标量；核心不认 toml，解耦 manifest 模型）
    public string Name; public string Version; public string Kind;   // "exe"/"lib"
    public bool HasEntry; public string Entry;
    public bool IsRelease;
    // 依赖解析（磁盘目录——见 D3；核心内部 DepScan.ScanDirs）
    public string[] LibsDirs; public int LibsDirsN;
    public DepEntry[] DeclaredDeps; public int DepCount;   // DepIndex 过滤白名单
}

// 组装好的 in-memory 包 + 结果状态。Ok=false 时 z 无意义、看 Diagnostics/ErrorCount。
public sealed class CompileArtifacts {
    public ZpkgFileZ z;         // 组装好的包（未写盘）
    public ZbcFileZ[] Mods; public int ModCount;
    public string[] Diagnostics; public int DiagCount; public int ErrorCount;
    public string EntryStatus;  // "ok" | "ambiguous" | "missing"（exe 专属；调用方决定报错措辞）
}

public static class PackageCompile {
    // 纯核心：⑥依赖解析 → ⑧包级编译 → ⑩组装。零落盘 / 零 Console / 零 ExitCode。
    public static CompileArtifacts Compile(CompileInputs inp) { ... }

    // 便捷：把 packed 产物投影为字节（Z42cCompiler 直接写 OutputZpkg；driver 走自己的分支）。
    public static PackedBytes ToPackedBytes(CompileArtifacts art, bool isRelease) { ... }
}
public sealed class PackedBytes { public byte[] Main; public byte[] Sym; public bool HasSidecar; }
```

### D2: 增量作**输入**，不进核心

**问题**：⑤⑦⑬ 的增量（probe / parse 复用 / cache 落盘）要不要进核心？

**决定**：**不进**。核心把 `Cus`（已 parse 的 CU）与 `CachedMods` 当**输入**：
- driver：先跑增量 probe → 得 `prep.Cus` / `prep.Cached` → 传核心；核心 `BuildPackageCus` 消费
  `cachedMods`（cached 文件跳过 typecheck/codegen）——**与现状完全一致**，故字节不变。
- `Z42cCompiler`：`ParseAllTk(texts)` 得全量 `Cus`、`CachedMods=null` → 传核心（全量编译）。

理由：增量是 dev-loop 的 CLI 关注点（依赖 probe cache 目录布局、meta 回填），塞进核心会把
`Z42cCompiler` 也拖进 cache 目录语义。核心只认「给你 CU 和 cached module，编译组装」——增量策略
留在 driver。**未来** z42b 若要增量，另经 `wire-z42b-future-deps-resolve` 补，不预建。

### D3: 依赖仍从磁盘目录解析（libsDirs），blob 内存化降级为后续

**问题**：wire-z42b design D2 设想 `CompileInMemory(texts, files, depBlobs[], ...)`——依赖 zpkg
以**字节 blob** 传入。要不要现在就把 `DepScan` 改成吃 blob？

**决定**：**不**。核心仍 `DepScan.ScanDirs(libsDirs)` 从**磁盘目录**解析依赖（与 `_build` 现状一致）。
`Z42cCompiler.Compile(req)` 把 `req.Deps`（依赖 zpkg 路径）的**父目录集**作为 `LibsDirs` 传入。

理由：
- 依赖 zpkg **恒在磁盘**（`Z42_LIBS` flat dir / workspace dist 目录）——「in-memory blob」解决的
  「依赖不在磁盘」在现实中不存在。真实目标是「**in-process、不 fork z42c、结构化诊断**」，磁盘
  依赖 + 内存源 + 内存产物已完全达成。
- `DepScan.ScanDirs` → `ScanBlobs` 的重构面（改依赖解析的输入形态）**收益为零、风险非零**（依赖解析
  是跨包决议的核心，动它要重验 DepIndex 确定性——common-pitfalls §1）。不为不存在的需求付这个价。
- 真需要（如依赖来自内存缓存、网络）时，另起 `extract-compile-pipeline-api-future-blob-provider`
  加 `DepScan.ScanBlobs(ZpkgBlob[])` 并联入核心——**加法、不改现有磁盘路径**。

> ⚠️ 这是对 wire-z42b design **D2 的精化**：`CompileInMemory(depBlobs)` → `PackageCompile.Compile
> (LibsDirs)`。wire-z42b 落地时其 D2 步骤 2「读 dep zpkg 字节 → ZpkgBlob[]」改为「取 dep 父目录 →
> LibsDirs」。**须同步回填 wire-z42b design.md D2**（实施本变更时一并改，避免两处漂移——CLAUDE.md
> 规范冲突检测）。

### D4: 核心住 `z42c.pipeline`，manifest 解耦为标量输入

**决定**：`PackageCompile` 住 `z42c.pipeline`（`Z42cCompiler` 也将住此包，同址零新跨包依赖）。核心
**不认 `ProjectManifest`**——吃 `Name/Version/Kind/HasEntry/Entry/IsRelease` 标量（`CompileInputs`）。

理由：
- 依赖用的 `IrDump.BuildPackageCus`（z42c.semantics）、`ZpkgBuilder`/`ZpkgWriterZ`（z42c.ir）、
  `DepScan`（z42c.ir）——`z42c.pipeline` 现有依赖已全覆盖，**不新增 toml dep**。
- 核心吃标量而非 `ProjectManifest`：driver 从 composed `pm.Project.{Name,Version,Kind,...}` 映射，
  `Z42cCompiler` 从 `CompileRequest` + 默认值（app 编译 kind 由 req 隐含）映射。核心不绑 manifest
  模型 → 将来 manifest 再演进也不动核心。

### D5: `isPacked` 决议留调用方

`_build` 的 `isPacked = pm.Project.HasPack ? pm.Project.Pack : isRelease` + 「indexed 与 --release
strip 冲突」拦截是 **CLI 策略**，留 driver。核心只认 `IsRelease`（影响 ⑧ 的 codegen strip 与 ⑫ 的
sidecar），产 `ZpkgFileZ z`；**怎么写（packed/indexed）由调用方定**。`Z42cCompiler` 恒 packed。

## 数据流（分层后）

```
z42c.driver/_build (CLI)                     Z42cCompiler.Compile(req)  [wire-z42b 落地]
  ├ ManifestLoader.Load                        ├ SourceDiscovery.Discover(req.SourceDir)
  ├ SourceDiscovery / read / hash              ├ read texts / hash
  ├ IncrementalDriver.Prepare  ─┐              ├ ParseAllTk → Cus       (CachedMods=null)
  ├ ParseAllTk / prep.Cus       │              │
  │                             ▼              ▼
  └─────────►  PackageCompile.Compile(CompileInputs)  ◄─────────┘
                     │  ⑥ DepScan.ScanDirs → DepIndex + imported
                     │  ⑧ IrDump.BuildPackageCus → CompiledModuleZ[]
                     │  ⑩ 组装 ZbcFileZ[] → BuildPackedD → ZpkgFileZ
                     ▼
              CompileArtifacts { z, Mods, Diagnostics, ErrorCount, EntryStatus }
  ├ ErrorCount>0 → ConsoleError+ExitCode       ├ ErrorCount>0 → CompileResult(false, diags)
  ├ EntryStatus → 报错措辞                      ├ ToPackedBytes → File.Write(req.OutputZpkg)
  ├ packed → WritePackedWithSidecar+write       └ CompileResult(OutputZpkg, zsym, true, "")
  ├ indexed → _writeIndexedDist
  └ cache 落盘
```

## 实施顺序（byte-identical 纪律）

1. **加 `PackageCompile`（不接调用方）**：把 ⑥⑧⑩ 逐字**平移**进 `Compile`，签名照 `CompileInputs`。
   独立编译通过。
2. **`_build` 改委托**：删 ⑥⑧⑩ 内联，改调 `PackageCompile.Compile`，⑨⑪⑫⑬ 从 `CompileArtifacts`
   取值。**此步唯一目标 = self-host gen1==gen2 逐字节不变**（纯平移，无逻辑改动）。
3. **加单测**：`PackageCompile` 编一个 hello 包 → 断言 `ModCount` / `EntryStatus="ok"` / 诊断为空；
   一个坏源 → `ErrorCount>0`。
4. driver 落地后跑 `xtask test compiler`（self-host 字节门 + 全 [Test]）。

> **风险**：⑥ 的「usings 聚合」用 `new string[8]` 起始 + 翻倍扩容（已随 #2a 8-using 修复思路一致，
> 此处 driver 内是 `allUsings` 局部，本就带扩容——见 Main.z42 当前实现）。平移时**原样搬**，勿"顺手
> 优化"，任何改动都可能致字节漂移。

## Testing Strategy
- self-host 7/7 gen1==gen2 逐字节（**权威**：纯 refactor 的字节不动点证明）。
- `PackageCompile` 单测（hello 包组装 / 坏源诊断 / exe entry 决议 / lib 无 entry）。
- 全 `[Test]` 无回归；`xtask test compiler` exit=0。

## Deferred
- `extract-compile-pipeline-api-future-blob-provider`：`DepScan.ScanBlobs` + 内存 blob 依赖（D3）。
- `wire-z42b-future-deps-resolve`：z42b 增量 + 多源依赖解析（D2）。

## 对下游 wire-z42b 的净效果
- `Z42cCompiler.Compile` 落地 = 「Discover → read → ParseAllTk → `PackageCompile.Compile`（LibsDirs=
  req.Deps 父目录）→ `ToPackedBytes` → 写 `OutputZpkg` → `CompileResult`」，**无新编译逻辑**。
- wire-z42b design **D2 需精化**：`CompileInMemory(depBlobs)` → `PackageCompile.Compile(LibsDirs)`
  （见本文件 D3；实施时同步改 wire-z42b design.md，单一真相源）。
