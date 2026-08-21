# Tasks: Attribute 与编译期 Handler 体系

> 设计 SoT：[design.md](design.md)。多 PR 阶梯，每 PR 单独分支 + GREEN + 合并（parallel-development）。

## 进度概览

| PR | 内容 | bump | 状态 |
|----|------|:---:|------|
| PR1a | HandlerRegistry AST-phase：AttributeSynth+BenchmarkDesugar 收敛 + 三路 kind 判定（无后缀，byte-identical）+ DeclId 概念 | 否 | ✅ 完成 |
| PR1b | HandlerRegistry IR-phase：TestIndexBuilder+StubEmitter 名字识别收敛 + KindOf 细化三路（`[Native]` 不改，byte-identical） | 否 | ✅ 完成 |
| PR2 | 后缀约定（D8）：resolution 展开（`[X]`→`XAttribute`）+ 强制校验 E0444 + test 家族全归 handler + 迁移 fixtures + 反转 `Attribute.z42`/`basic.z42`/`attributes.md` 头注 | 否 | ✅ 完成 |
| PR3a | Analyzer **框架**：`Analyzer`(visitor 模型 ObservedKinds+OnSyntaxNode，**无 delegate**——z42c 规避+命名 delegate 跨包丢 FQ 名 [[z42c-no-cross-pkg-delegates]])/`DiagRule`/`AnalyzerSeverity`/`DiagSink`/`SyntaxKind`(z42c.syntax，非 stdlib——契约暴露 AST) + AnalyzerDriver(AST 遍历分派+诊断映射进 DiagnosticBag) + `*Analyzer` 后缀强制(E0445) + 驱动单测(NoEmptyCatchAnalyzer)。KindOf 不动(analyzer 无应用位) | 否 | ✅ 完成(1-3) |
| PR3a-load | **外部编译期加载 + pipeline 接线**：`[analyzers]` 段(ManifestLoader) + z42c 加载该段 zpkg 元数据发现 `: Analyzer`(Path-A `AssemblyLoadContext.Default().Load`→`GetTypes`→过滤 `GetInterface`) + Path-B 实例化执行(`__load_module`→`Type.GetType`→无参 `Activator.CreateInstance`→as Analyzer→喂 AnalyzerDriver) + PackageCompile 接线(gated on [analyzers]，z42c 自建无→byte-identical) + **编译器级 in-process 集成测试**(非提交二进制→免格式-bump 脆性，合 D9「构建工具 zpkg」定位)。**与 PR4 generator 加载共享此 infra**。红线:z42c 自建不声明 [analyzers] | 否 | ✅ 完成(#235) |
| PR3b | `[lints]` config(ManifestLoader `_parseLints`) + severity 解析(EnabledByDefault + `[lints]` 覆盖 + warnings-as-errors) | 否 | ✅ 完成(#238) |
| PR3c | 局部抑制(合并单 PR，**纯编译期零 zpkg 持久化**)：`#suppress`/`#restore` pragma(新语法，z42c/stdlib 不 use → 单 PR 加 support 不触两-nightly)拦截 Hash→`CompilationUnit.SuppressRegions`(AST-only) + `[Suppress]` attr(**directive**——加 `IsDirectiveAttr`、不写 blob/descriptor、无需 stdlib 类) + `SuppressionSet` 判定挂 `DiagSinkImpl.Report` | 否 | 🟡 进行中 |
| PR4 | Generator/ModuleGenerator 复用 `[analyzers]` 类别加载(D9) + splice/merge（Add/Replace/Augment） | 否 | ⬜ |
| PR5 | `[Deprecated]` directive（D2，持久化 flag+msg，跨包+IDE） | 是 | ⬜ |
| PR6 | caller 编译期宏（D3） | 是(param) | ⬜ |
| PR7 | `--fix` 统一分析+修复（build 期 splice） | 否 | ⬜ |
| 后续 | `[Native]`→`[Extern]` 改名 / `[Layout]`/`[Repr]`(E2) / `OnIrOp` perf lint / 用户 `macro` / 局部变量 attribute | 视需 | ⬜ Deferred |

## PR3a-load · 外部编译期加载 + pipeline 接线（当前，🟡 进行中）

**目标**：把「外部 analyzer zpkg 编译期加载并驱动」接通，让 PR3a 的 `AnalyzerDriver` 不再只吃内存
实例，而是从消费方 `[analyzers]` 段声明的 zpkg 里**发现 + 实例化 + 执行** `: Analyzer` 类型。
零 bump（handler zpkg 是普通 zpkg，"特殊"只在引用方式）。z42c 自建不声明 `[analyzers]` → **byte-identical**。

### 关键设计点（勘察已定，写码直接用）

- **两路加载组合（memory 勘察 + 本次源码复核确认）**：单靠一条路都不够——
  - **Path-A 发现（reflect-only）**：`AssemblyLoadContext.Default().Load(zpkgPath)` → `Assembly.GetTypes()`
    → 过滤 `t.GetInterface("Z42.Syntax.Analyzer") != null` 拿 FQN。`__lctx_load` 进 root context 的
    `AssemblyEntry` 类型**可枚举**，但函数**不并入 live module → 不可调**（`context.rs:load_into`）。
  - **Path-B 可调用**：`__load_module(zpkgPath)`（`load_module_into_vm` flat-merge）→ 函数/类型并入
    live VM **可 `Type.GetType`+`Activator.CreateInstance`+虚调用**（但返回 TIDX、无类型枚举）。
  - 两路对同一 zpkg 各跑一次：A 只为拿 FQN 清单，B 只为让实例可执行。范式 = z42b
    [builder.z42:63-84](../../../../src/toolchain/builder/core/builder.z42)（Load→GetType→CreateInstance→as，已生产验证；那里 FQN 硬编码，这里靠 A 发现）。
- **接口同一性**：analyzer 实现的 `Z42.Syntax.Analyzer` == 编译器进程里**已驻留**的同一接口（z42c 由
  z42c.syntax 构成，运行时它已加载）→ `as Analyzer` cast + `AnalyzerDriver.Run` 直接可用，无需跨版本桥。
- **`__load_module` 绑定归属**：它是 `Std.Test.ModuleLoader` 的 `[Native]`；z42c **不依赖 z42.test**，
  故在 AnalyzerLoader 内**自绑** `[Native("__load_module")]` private extern（native 是 VM builtin，种子里在——无两-nightly 越界）。
- **诊断合入**：`AnalyzerDriver.Run` 产 `DiagnosticBag` → 逐条格式化 append 进 `CompiledModuleZ.DiagMsgs`/
  `DiagCount`；**仅 severity==Error 才 `ErrorCount++`**（analyzer 默认 Warning → 不 fail 编译）。
- **gating**：`AnalyzerZpkgCount>0` 才加载/运行。z42c 自建无 `[analyzers]` → 该分支不进 → 逐字节不动。

### 实施（按依赖顺序）

- [x] 1. `[analyzers]` 段解析（`src/libraries/z42.project/`）：`ProjectManifest` 加 `Analyzers`(DepEntry[])
      + `AnalyzerCount`（构造后填，同 `OptimizeNames`）；`ManifestLoader._parseAnalyzers(root,pm)` 复用
      `_parseDeps(root,"analyzers")`。**单测**（`manifest_roundtrip.z42`）：`test_analyzers_section_parsed`
      + `test_analyzers_section_absent_empty` ✅。
- [x] 2. `AnalyzerLoader.z42`（新，z42c.semantics）：`Load(string[] zpkgPaths, int count) -> Analyzer[]`。
      路径先按 Ordinal sort（common-pitfalls §1）；逐 zpkg 跑 Path-A（`ALC.Default().Load→GetTypes→过滤
      GetInterface("Analyzer")`）+ Path-B（自绑 `[Native("__load_module")]` 使可调）+ 用 `Type.GetType(FullName)`
      → `Activator.CreateInstance` → `as Analyzer` 实例化；growable 用 `_push`。
- [x] 3. `PackageCompile` 接线：`CompileInputs` 加 `AnalyzerZpkgs`+`AnalyzerZpkgCount`（ctor 默认空）；
      `Compile()` 在 `BuildPackageCus` 后、错误门前 gated 跑 `_runAnalyzers`（逐 `inp.Cus[i]` → `AnalyzerDriver.Run`
      → `_mergeDiags` 合入 `cms[i]`，仅 Error 级 `ErrorCount++`）。
- [x] 4. driver 接线（`z42c.driver/Main.z42`）：`pm.Analyzers` 名 → LibsDirs 找 `<name>.zpkg` → 填
      `cin.AnalyzerZpkgs`（找不到 → E BuildError）。**z42b `Z42cCompiler` 未接线**（`CompileRequest` 无
      analyzers 字段，MVP app-compile；共享 PackageCompile 核心 gated 跑，parity 是干净 follow-up）。
- [x] 5. **编译器级 in-process 集成测试**（`pkgcompile_tests.z42`）：不提交二进制——测试内用 `PackageCompile`
      把 fixture analyzer 源编成临时 zpkg（`Z42_LIBS` 供 z42c.syntax/core，TMPDIR 写盘）→ 编含空 catch 的
      consumer（typeless `catch { }` 避 Error 依赖）`AnalyzerZpkgs` 指向它 → 断言 `cms` 含 Z9002。
      `test_external_analyzer_loaded_and_reports` + `test_no_analyzers_no_extra_diags` ✅。
- [x] 6. **轴② bootstrap-staging 破环**：扩 `_ensureBootstrapZ42Ir`（`scripts/build/xtask_compiler.z42`）把当前源
      z42.project 也预建进 flat（早于 z42c self-build）——否则 z42c.driver 对着旧 z42.project 编 → E0401。
      **改 scripts → 重建 xtask.zpkg**。
- [x] 7. GREEN：`xtask test` **全绿（REAL_EXIT=0）**；self-host gen1==gen2 **5/5 逐字节**（z42c 自建无
      `[analyzers]` → byte-identical 保）；e2e 250/0 + cross-zpkg 11/0 + stdlib 全绿 + golden regen 261/0
      + vscode-syntax ✅。
- [x] 8. 文档同步：z42.project README（ManifestLoader `[analyzers]` + ProjectManifest.Analyzers 行 + DepEntry
      段）+ z42c.semantics README（AnalyzerDriver + AnalyzerLoader 行）+ `docs/design/language/attributes.md`
      头注（编译期 handler / 外部 analyzer 加载随 PR3a-load 落地，design.md 为 SoT）。

### 实施实测确认（原「待验证」，已解）

- ✅ ALC.Default().Load + __load_module 对同一 zpkg 双注册无冲突（两独立 registry；集成测试证实）。
- ✅ 测试内把 analyzer 源编成 zpkg 写盘（`ZpkgWriterZ.WritePacked().ToBytes()` + `TMPDIR`）在 z42c 测试环境可用。
- ⚠️ **轴② 教训**：z42c.driver 编译期新用 z42.project 字段 → 必须把 z42.project 加进 `_ensureBootstrapZ42Ir`
  预建集（与 z42.core/z42.ir 同款）；否则 self-build E0401。`build stdlib`/`build compiler` 单独跑的 exit
  经 pipe 到 grep 会假 0——必须直接捕获 xtask 的 `$?`。

## PR3b · [lints] config + severity 决策（当前，🟡 进行中）

**目标**：让 analyzer 报的诊断走 severity 决策链——不再一律用规则默认级。`[lints]` 段做逐规则覆盖
（`Z9002="error"` / `"pkg.*"="none"`）+ `warnings-as-errors`；结合规则 `EnabledByDefault` 门决定是否报
及最终级别。零 bump；z42c 自建无 `[lints]` → LintCount==0 && !WAE → 空配置 → **逐字节不动**。

### 决策链（LintConfig.Resolve，对齐 Roslyn editorconfig）

1. `[lints]` 覆盖：**精确 rule Id 优先**于 `pkg.*` 前缀通配；`"none"` → 抑制（不报）。
2. 无覆盖时 `EnabledByDefault` 定基线：默认禁用 + 无覆盖 → 抑制；否则用 `DefaultSeverity`。
3. `warnings-as-errors` → 最终为 Warning 的规则升 Error（Info/Hidden 不动）。
4. 抑制（禁用 / "none"）→ `DiagSinkImpl.Report` 整条丢弃、不进 bag。

### 实施

- [x] 1. `[lints]` 段解析（`src/libraries/z42.project/`）：`ProjectManifest` 加 `LintNames`/`LintSeverities`/
      `LintCount`/`LintWarningsAsErrors`（构造后填，同 `OptimizeNames`）；`ManifestLoader._parseLints` 把
      `warnings-as-errors`（bool）从逐规则 severity 串（复用 `Keys()`/`AsString()`）拎出。**单测** 3 个
      （`manifest_roundtrip.z42`：parsed / absent-empty / wae-only）。
- [x] 2. `LintConfig.z42`（新，z42c.semantics）：`Resolve(DiagRule)→int`（-1=抑制）——精确+通配查覆盖、
      severity 串解析、EnabledByDefault 门、WAE 升级。`Empty()` = 无覆盖无 WAE（null cfg 默认）。
- [x] 3. `AnalyzerDriver`：`DiagSinkImpl` 持 `LintConfig`，`Report` 先 `Resolve`、抑制则不入 bag；`Run`
      加 `LintConfig cfg` 参（null→Empty）。**analyzer_tests.z42** 加 6 个决策单测（override-to-error /
      none-suppresses / WAE / wildcard / disabled-by-default 不报 / [lints] 显式打开默认禁用规则）。
- [x] 4. `PackageCompile`：`CompileInputs` 加 `LintConfig Lints`（默认 null）；`_runAnalyzers` 传给 `Run`。
      **pkgcompile_tests.z42** 加 2 个端到端（`Lints` none 抑制 / WAE 令 ErrorCount>0 拦编译）。
- [x] 5. driver 接线（`Main.z42`）：`pm.LintCount>0 || pm.LintWarningsAsErrors` → 构造 `LintConfig` 填
      `cin.Lints`。z42b `Z42cCompiler` 未接线（同 PR3a-load，MVP；干净 follow-up）。
- [x] 6. **轴② bootstrap-staging 已被 PR3a-load 覆盖**：z42.project 已在 `_ensureBootstrapZ42Ir` 预建集
      → 新字段随当前源预建进 flat，z42c self-build 见新字段，无 E0401。**无新 staging 工作**；改 scripts
      未触 → 无需重建（但改了 z42.project/z42c 源 → 常规重建 xtask.zpkg 供种）。
- [ ] 7. GREEN：`xtask test` 全绿（REAL_EXIT=0）；self-host gen1==gen2 **5/5 逐字节**（z42c 自建无 `[lints]`
      → byte-identical 保）；manifest/analyzer/pkgcompile 单测全过。
- [ ] 8. 文档同步：z42.project README（`[lints]` 行 + ManifestLoader 段）+ z42c.semantics README（LintConfig
      + AnalyzerDriver Run 签名）+ design.md 头注（PR3b 落地）。

## PR2 · 后缀约定 D8（已完成，破坏性非 byte-identical）

**目标**：落地 D8 后缀约定——store-meta `[X]` 按后缀展开解析到类 `XAttribute`；`: Attribute` 类名缺后缀 →
硬编译错 `E0444`；test 家族全 8 名归 handler（消除 PR1a/1b 保字节不动点的 5/3 差集折衷）。**非
byte-identical**：attributes 类目 golden 重生 + Setup/Teardown/Ignore 不再合成 store-meta 死工厂。

### 实施

- [x] `HandlerRegistry`：加 `StoreMetaClassName(name)=name+"Attribute"`（后缀逻辑单点）；`KindOf` 的 handler
      分支改问 `IsTestHandlerAttr`（全 8 名）→ Setup/Teardown/Ignore 归 Handler、不再 store-meta；删
      `_isTestHandlerNonStoreMeta`。
- [x] store-meta 两处消费改经 helper 取带后缀类名：`AttributeSynth._synthFactory`（`new XAttribute()`）+
      `ClassDescBuilder._attrRefsFromList`（IrAttrRef 持久化类型名 `_qClass(XAttribute)`）。
- [x] `SymbolCollector._passAttributeSuffixEnforce`（3 挂载点，同 `_passSealedEnforce`）：直接基类名 ==
      `Attribute` 且类名不以 `Attribute` 结尾 → `E0444`（`DiagnosticCodes.AttributeSuffixRequired`）。
      语法层检查（不依赖 base 可解析）。Generator/Analyzer 后缀强制留 PR3/PR4（彼时那两 kind 才成真类型）。
- [x] 迁移 fixtures（`[X]` 应用名不动，改类定义/ctor/typeof/cast）：`src/tests/attributes/{basic,field_attrs,
      methods}.z42`、`src/tests/types/param_attributes.z42`、`src/libraries/z42.core/tests/reflection.z42`。
- [x] 反转文档头注：`Attribute.z42` + `basic.z42` + `docs/design/language/attributes.md`（"无后缀 improvement"
      → 后缀强制、比 C# 更严）+ `naming-conventions.md` §14b。
- [x] E0444 负测试：`collect_tests.z42` 加 3 个（缺后缀报 / 带后缀不报 / 非-attribute 类不受约束）。

### 关键设计点

- **store-meta 判定仍是名字驱动**（非全类型解析）：AST-phase 无全符号表，且 PR2 尚无 Generator/Analyzer 类
  → 无歧义可能（`DataAttribute`+`DataGenerator` 同名歧义检测留 PR4）。故 `[X]` 一律展开 `XAttribute`，
  directive/test 家族豁免（走各自 handler，不经 store-meta 工厂）。
- **test 家族全归 handler = 消除 PR1a/1b 字节不动点陷阱**：那个「5 名 handler / 3 名 store-meta」差集是纯
  重构期为保字节一致的折衷；PR2 破坏性，正是收敛它的时机（design：test = 内建 handler、豁免后缀）。

### GREEN

- [ ] worktree 供种 + 重建 xtask.zpkg；`xtask test all` 全绿；self-host gen1==gen2 **5/5**（新行为不动点）。
- [ ] attributes 类目 golden 重生（`--dir attributes`）；reflection 单测（z42.core [Test]）全过。
- [ ] `xtask test bootstrap`：不涉新语法/格式（纯 semantics 逻辑）→ 无越界。

## PR1b · HandlerRegistry IR-phase 名字识别收敛 + KindOf 细化三路（当前）

**目标**：把 IR 级两个子构建器（`TestIndexBuilder` / `StubEmitter`）的 attribute **名字识别**上移到
`HandlerRegistry`——emit **逻辑不变**（design §Implementation Notes「逻辑不变，识别改注册表」），
且把 PR1a 的「Directive 一锅端」`KindOf` 细化成真三路。**零新语法、零 bump、byte-identical**。

### 实施

- [x] `HandlerRegistry.KindOf` 细化三路：`Native`→`Directive`；test 非-store-meta 家族
      (`Test/Benchmark/Skip/ShouldThrow/Timeout`)→`Handler`；其余（含 `Setup/Teardown/Ignore`）→`StoreMeta`。
      **store-meta 判定逐字节保持 PR1a**（非-store-meta 集 = directive ∪ 非-SM-test = 那 6 名）。
      返回值目前只被 `AttributeSynth` 以 `== StoreMeta` 消费 → Directive/Handler 区分为文档性、无行为影响。
- [x] 新增注册表查询：`IsNativeDirective(name)`（StubEmitter 用）+ `IsTestHandlerAttr(name)`（8 名触发集，
      TestIndexBuilder 用）。
- [x] `StubEmitter`：两处 `at.Name == "Native"` → `HandlerRegistry.IsNativeDirective(at.Name)`。
- [x] `TestIndexBuilder`：删私有 `_isTestAttrName`（8 名）→ `_hasTestAttr` 改问 `HandlerRegistry.IsTestHandlerAttr`。
- [x] **BenchmarkDesugar 的 `== "Benchmark"` 自触发保留**（applied generator 类名剥后缀即触发，intrinsic；
      PR1a 已把它收进 `RunAst`，其 trigger 不属本 PR 收敛面）。

### 关键设计点（byte-identical 陷阱，PR1a 起沿用）

- **两个 test 名集不同、且必须不同**：`IsTestHandlerAttr` = 全 8 名（TestIndexBuilder 触发集，含
  Setup/Teardown/Ignore）；`KindOf` 的 Handler 子集 = 5 名（非-store-meta）。差集 {Setup/Teardown/Ignore}
  现仍走 store-meta 工厂——对齐两集会破自举字节不动点（PR1a 陷阱）。注册表用两个独立函数显式建模。
- **TestIndexBuilder 现状=handler 形，终态=store-meta**：design 记 TIDX 终态是 store-meta+反射发现、
  TIDX 退休（独立后续变更）；本 PR 标注的是**现状机制**（eager 聚合表 = module-generator 形）→ 归 Handler。

### GREEN（全绿）

- [x] worktree 供种（`.z42`/`xtask`/`xtask.zpkg` 沿用 PR1a 种子，无 bump）。
- [x] `xtask test all` 全 stage gate 绿；self-host gen1==gen2 逐字节 **5/5**；z42c `[Test]` 24 units via TIDX 全过（直接覆盖 TestIndexBuilder 路径）。
- [x] `xtask test incremental` incr==full 逐字节（demo 5/5 + xtask 50/50 whole-dist byte-identical）。
- [x] zbc-format golden 零漂移；Setup/Teardown/Ignore + Native 行为逐字节保持。

## PR1a · HandlerRegistry AST-phase + 三路 kind 判定（已完成）

**目标**：纯内部重构——把 AST 级两个 pass 收敛进 `HandlerRegistry`，用**三路 kind 判定**替换 `_isUserAttr`
名字白名单，**零新语法、零 bump、外部可见行为逐字节不变** → self-host gen1==gen2、5/5 不动点保持。

### 阶段 0 · 勘察（已完成）

- [x] 4 pass 分两层已确认：AST 级 `AttributeSynth.Run(BenchmarkDesugar.Run(raw))`（`IncrementalDriver.z42:52`
      / `IrDump.z42:82`）；IR 级 `TestIndexBuilder`/`StubEmitter` 为 IrGen 子构建器（`IrGen.Generate` 内）。
- [x] `_isUserAttr`（`AttributeSynth.z42:120-124`）唯一消费点 `AttributeSynth.z42:101`。
- [x] attr→IrAttrRef→zpkg 数据流；分流点 = `Attr.FactoryFunc`。
- [x] **字节不动点陷阱**：`_isUserAttr` 黑名单缺 `Setup/Teardown/Ignore`（它们现会被合成工厂）。

### 阶段 1 · HandlerRegistry 脚手架（AST-phase）

- [ ] 定义 `HandlerRegistry`，AST-phase 入口签名保持 `CompilationUnit → CompilationUnit`（与现有两 pass 同构）。
- [ ] 实现三路 kind 判定：directive 注册表（canonical）→ 实现 handler 接口 → else store-meta。
      **PR1a 阶段先只落 store-meta 分支的判定**（directive/handler 分支在 PR1b/PR4 接入），且 store-meta 判定
      **逐字节复刻 `_isUserAttr`**（含 Setup/Teardown/Ignore 走工厂的现状），不得对齐 `_isTestAttrName`。
- [ ] 奠定 `DeclId` 概念（PR1a 用不到 Replace/Augment，仅预留寻址）。

### 阶段 2 · AST 级两 pass 收敛

- [ ] `AttributeSynth`（用户 attr→工厂）→ registry 的 store-meta 默认路径。
- [ ] `BenchmarkDesugar` → registry 编排的内建 Generator（AST-phase 变换，逻辑不变）。
- [ ] 把 `IncrementalDriver.z42:52` + `IrDump.z42:82` 的 `AttributeSynth.Run(BenchmarkDesugar.Run(raw))`
      换成 `HandlerRegistry.RunAst(raw)`（两处必须同步改）。
- [ ] 删 `_isUserAttr`（消费点已改走 registry）。

### 阶段 3 · GREEN + 自举不动点

- [x] worktree 供种（`.z42`/`xtask`/重建 `xtask.zpkg`，见 [[fresh-worktree-seed-setup]]）。
- [x] `xtask test all` 全 stage gate 绿（e2e goldens + stdlib + compiler + vscode-syntax）；self-host gen1==gen2 逐字节 **5/5**。
- [x] `xtask test incremental` incr==full 逐字节（demo + xtask，55/55 files byte-identical）——覆盖 `IncrementalDriver.ParseAllTk` 改动路径。
- [x] zbc-format golden 零漂移；`[Setup]/[Teardown]/[Ignore]` 行为逐字节保持（store-meta 判定复刻 6 名 `_isUserAttr` false-集，未对齐 `_isTestAttrName`）。

### PR1a 实测勘察修正（写码时发现，与阶段 0 DRAFT 出入）

- **挂载点是 4 处，非 DRAFT 说的 2 处**：`AttributeSynth.Run(BenchmarkDesugar.Run(...))` 除
  `IncrementalDriver:52` / `IrDump:82` 外，还有 `IrDump:558`（`BuildModuleD` 跨包路径）+
  `IrDump:667`（`_buildFOpt` 内联单测路径）。四处全切 `HandlerRegistry.RunAst`。
  （`bench_desugar_tests.z42:13` 直调 `BenchmarkDesugar.Run` 保留——单测隔离，函数仍 public。）
- **DeclId 非死代码**：`AttributeSynth._process` 实际 `new DeclId(keyPrefix)` 并用 `did.Key` 构工厂名
  （`did.Key == keyPrefix` → 逐字节一致）。奠定寻址概念且真被执行，不违反 philosophy 反 speculative。
- **`test incremental` 是必跑的额外 gate**：`test all` 不含它，而本 PR 动了增量路径 → 必须单独跑。

## PR3c · 局部抑制 `#suppress`/`#restore` + `[Suppress]`（当前，🟡 进行中）

**目标**：让 analyzer 诊断可局部关闭（对应 C# `#pragma warning disable/restore` + `[SuppressMessage]`）。
两机制合并单 PR（User 裁决 2026-08-21）。抑制检查点统一在 `DiagSinkImpl.Report`，抑制区随 `cu` 流入
`AnalyzerDriver.Run`（无需改 Run 签名 / `_runAnalyzers`）。**无格式 bump**（源码级、编译期消费即弃 +
store-meta blob 走现有反射机制）。z42c/stdlib 只加 support 不 use → 无两-nightly 纪律、self-host byte-identical。
设计 SoT：[design.md](design.md) §诊断/severity/开关「PR3c 落地」。

### 阶段 1：`#suppress`/`#restore` 区间指令（新语法）
- [x] 1.1 `CompilationUnit` 加 `SuppressRegion[] SuppressRegions` + `int SuppressRegionCount` 字段
  （`z42c.syntax/src/Decl.z42`）；新增 `public sealed class SuppressRegion { string RuleId; int Start; int End; }`
- [x] 1.2 Parser 拦截 `Hash` token：在语句列表（`_stmtP._parseBlock` 循环）+ 顶层/成员声明列表边界
  （`Parser.ParseCompilationUnit` 循环 + `_declP` 成员循环）见 `#` + ident `suppress`/`restore` → 解析指令。
  累加器挂主 `Parser` 实例，子解析器经引用共享；`_push` growable idiom 累积区间；`ParseCompilationUnit`
  收尾把区间挂到 CU。开区间栈按 RuleId 配对；EOF 未闭合 → 延伸到 `cu.Span.End`。
- [x] 1.3 边界/错误：`#restore` 无匹配开区间 → 诊断（宽松：忽略或 warn）；`#suppress` 缺 RuleId → parse error。
- [x] 1.4 dump/parse 单测（`z42c.syntax/tests/`）：`#suppress`/`#restore` 正确产出 SuppressRegions（区间数 + Start/End）。

### 阶段 2：`[Suppress]` 声明级抑制（directive——纯编译期，不写 zpkg）
> User 裁决 2026-08-21：`[Suppress]` 归 **directive** 而非 store-meta——抑制是编译期本地概念、无运行时消费者，
> 持久化纯膨胀。核实：`AttributeSynth._process:104` 只对 store-meta 写 IrAttrRef；`StubEmitter` 只烘 Native；
> 无 `NativeAttribute` 类→directive 无需 backing 类。故归 directive = 零 blob、零 descriptor、无需 stdlib 类。
- [x] 2.1 `HandlerRegistry.IsDirectiveAttr` 加 `name == "Suppress"`（不加 `IsNativeDirective`）→
  `KindOf("Suppress") == Directive`。**不建 z42.core 类**（同 `[Native]` 靠名字识别）。
- [x] 2.2 验证：`[Suppress("Z9002")]` 应用后 `AttributeSynth` 跳过（非 store-meta→无 blob）、`StubEmitter`
  忽略（非 Native→不烘）、`AttributedDecl` 仍留在 AST（driver 可按名读 `attr.Args[0]`）。self-host byte-identical。

### 阶段 3：driver 抑制判定
- [x] 3.1 `z42c.semantics` 新增 `SuppressionSet`（`Add(ruleId,start,end)` + `IsSuppressed(ruleId,pos)`；growable）。
- [x] 3.2 `AnalyzerDriver.Run` 内建 SuppressionSet：① 灌入 `cu.SuppressRegions`；② walk decls 收集
  `[Suppress("Id")]`（`AttributedDecl` 未 `_unwrap` 前读 `attr.Name=="Suppress"` + `attr.Args[0]` 字符串
  字面量，区间 = `Inner.Span`）。传给 `DiagSinkImpl`。
- [x] 3.3 `DiagSinkImpl` 加 `SuppressionSet Supp` 字段；`Report` 在 severity resolve 前判
  `Supp.IsSuppressed(rule.Id, at.Start)` → 命中 return（不报）。

### 阶段 4：测试
- [x] 4.1 semantics 单测（`tests/analyzer/analyzer_tests.z42`）：SuppressionSet 命中/不命中；driver + `#suppress`
  区间 + `[Suppress]` 声明各抑制一处、区间外仍报。
- [x] 4.2 pkgcompile 端到端（`z42c.pipeline/tests/pkgcompile/`）：fixture analyzer emit Z9002 于空 catch；
  consumer 源在 `#suppress Z9002`/`#restore` 内放一处空 catch + `[Suppress("Z9002")]` 方法内放一处 + 区间外
  放一处 → 断言 cms 只含区间外那一处 Z9002。

### 阶段 5：验证 + 文档
- [x] 5.1 GREEN：`xtask test`（含 e2e/cross-zpkg/stdlib/compiler/vscode-syntax）REAL_EXIT=0 + self-host 5/5 逐字节。
- [x] 5.2 book 语言参考：`#suppress`/`#restore` 指令 + `[Suppress]` attribute（机制页 + attributes.md）。
- [x] 5.3 目录 README 同步（`z42c.syntax` / `z42.core` / `z42c.semantics` 功能索引 + 核心文件）。
- [x] 5.4 design.md 「PR3c 落地」note 已写（本 PR 完成后核对与实现一致）。

## 备注

- 每 PR 合并前并入 main 最新 + 重跑 GREEN（parallel-development §3）。
- **语义耦合自查**：并发 worktree `z42-record [add-record-attribute]`（record 用 attribute 式声明）、
  `z42-conv [add-user-conversions]`（动 semantics）与本 change 邻接——PR2（后缀迁移）/ PR3（analyzer 触
  semantics）开工前主动对表/知会（parallel-development §4）。
- **PR2 后缀约定是破坏性迁移**（改现有 attribute 类名 + 反转 `Attribute.z42`/`basic.z42` 已记录的"无后缀"
  决定），**非 byte-identical**；会破一代自举、warm 重建自愈（D7 式）。与纯重构 PR1a/1b 分开，不混。
- 带 bump 的 PR（PR5/6）走 bootstrap-seed 两阶段纪律，bump 前 `xtask test bootstrap`。
