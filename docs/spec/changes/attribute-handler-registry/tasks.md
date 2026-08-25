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
| PR3c | 局部抑制(合并单 PR，**纯编译期零 zpkg 持久化**)：`#suppress`/`#restore` pragma(新语法，z42c/stdlib 不 use → 单 PR 加 support 不触两-nightly)拦截 Hash→`CompilationUnit.SuppressRegions`(AST-only) + `[Suppress]` attr(**directive**——加 `IsDirectiveAttr`、不写 blob/descriptor、无需 stdlib 类) + `SuppressionSet` 判定挂 `DiagSinkImpl.Report` | 否 | ✅ 完成(#241) |
| PR4a | applied generator **最终引擎**(方案 B post-bind，参考 Roslyn)：契约 `Generator`/`GenSink`/`GenTarget`(z42c.semantics，visitor 无 delegate) + `*Generator` 后缀 E0447 + 三 sink(**Augment=脱糖 synthetic partial+自动标原类型 partial** / **Replace=AST 层按 DeclId 替换** / **AddSource=新 CU**) + ordering fixup(剥触发 attr+删预合成 store-meta 工厂) + `PackageCompile` provisional bind→driver→union 重编(gated，z42c 自建 byte-identical) + **集成测试注入驱动** | 否 | ✅ 完成(#245) |
| PR4c | `ModuleGenerator` + `GenContext`(`TypesWith<T>`/`MethodsWith<T>` 强类型符号查询，靠泛型方法 #240) + `GeneratorDriver.RunModules`(扫全编译→AddSource，越界 E0448) + `PackageCompile` 同 gated 块接在 applied 之后 + `*Generator` 后缀 E0447 扩到 `ModuleGenerator` + 集成测试注入驱动 | 否 | ✅ 完成 |
| PR4d | 外部 generator zpkg 加载(`GeneratorLoader`，编译内发现 `: Generator`/`: ModuleGenerator` + AnalyzerLoader 式 Path-A/B) 经 **`[analyzers]` 段**(D9 单段覆盖两类，User 2026-08-22 裁决；非独立 `[generators]`) + **VM 执行 golden**(真跑生成方法)；复用 PR3a-load infra | 否 | ✅ 完成 |
| PR4e | 有界多轮**引擎+契约+指纹（support，未接线）**：契约加 `Consumes()`/`Produces()`(Generator+ModuleGenerator) + `GeneratorDriver.RunRounds`(统一节点 Kahn 最长路径分层 + 成环 **E0449** 字面量 + 逐层 re-bind + 轮内 applied 先·module 后) + 多轮/成环/退化单测 + **产物纳入增量指纹**(handler zpkg 内容指纹揉入 srcHashes，driver-intra 即 active)。**PackageCompile 仍单轮**——接线 RunRounds 是新跨成员符号撞 F2 冷启动，留 PR4e-wire | 否 | ✅ 完成 |
| PR4e-wire | **接线激活多轮**（RunRounds 随 nightly `main@9fc6414` 发布后）：PackageCompile 单轮 `Run`+`RunModules` → `GeneratorDriver.RunRounds`；E0449 字面量→常量 `DiagnosticCodes.GeneratorDependencyCycle`。staged-bootstrap 轴② use 阶段。GREEN：test all 5/5 byte-identical + bootstrap 无越界 + incremental 5/5+51/51 byte-identical | 否 | ✅ 完成 |
| PR4f(Deferred) | 生成产物**跨 build 增量缓存复用**(现 generator 一律强制全量重编，正确但非增量)：需 probe 感知 generator(probe 时运行/指纹化产物)——独立较大 change | 否 | ⬜ Deferred |
| PR5 | `[Deprecated]` directive（D2；方法+类+字段；**零格式-bump attr-ref 哨兵**，非最初计划的 flag+msg 格式 bump）→ 详见下「PR5 子任务」 | 否 | ✅ 完成(#268) |
| PR6 | **参数默认值统一常量表示 + 修跨包塌零 bug**（D3 默认值部分）：ConstBlob 取代标量 `kind`（含 struct/enum/数组/命名常量）+ `$Default` param attr-ref 哨兵持久化 + 跨包省略实参用真实默认值（**修 bug，已验证**）+ 反射迁移到 ConstBlob；`default_kind` 字节退休写 0。**caller 宏拆到 PR6b**（User 2026-08-24 裁决） → 详见下「PR6 子任务」 | **否**（零-bump，骑 zbc 1.15 param attr-ref 通道） | ✅ 完成(#282) |
| PR6b | **caller 编译期宏**（D3 本体）：`caller_member!()`/`caller_line!()`/`caller_file!()`/`module_path!()` 表达式宏（限参数默认值位，support-先行）+ `$Caller:*` 哨兵 + 调用点注入 enclosing-member/line/file/namespace（复用 PR6 的 ConstBlob 持久化 infra + IrParamDefault.CallerPrefix/ExportedParamZ.CallerKind/Z42FuncType.ParamCallers 已就位）。**F2-安全设计**：parser 把 `name!()` 译成既有 `IdentExpr("$macro:"+name)` 哨兵（**不新增 syntax→semantics 跨包类型** → 避冷启动 stale-cache），语义层白名单认；诊断 **E0450** 用字面量发码 | 否（零-bump，零 Rust 格式改） | ✅ 完成(#290) |
| PR7-support | **代码修复契约（D4「同套 splice」，support 未接线）**：z42c.syntax 加 `TextEdit`/`CodeFix`/`FixSink`（统一 analyzer+fix，analyzer 下转 `diags as FixSink` 报诊断+修复，非 C# 拆两类型）；F2-安全预置接线桩（`AnalyzerDriver.Run` 加 `bool fix` 重载体空转 + `CompileInputs.Fix` 未接）。**纯 additive、无 in-tree 消费 → byte-identical、冷启动零消费**；随 nightly 进种子后 PR7-wire 零新跨成员符号（PR4e/6b staged-bootstrap 同款） | 否 | 🟡 实施中 |
| PR7-wire | **接线 `z42c build --fix` 就地重写源**（契约随 nightly 发布后）：`DiagSinkImpl` 实现 `FixSink` 收集编辑 → 按 `TextEdit.At.File`+字节区间就地重写源文件（`Span` 自带 File/Start/End，无需额外传路径）；driver `--fix` flag→`inp.Fix`→`_runAnalyzers` 调 6-参 `Run`；内建可修复规则 demo + 跨包 golden（第三方 analyzer 携修复，litmus B） | 否 | ⬜ 待 nightly |
| 后续 | `[Native]`→`[Extern]` 改名 / `[Layout]`/`[Repr]`(E2) / `OnIrOp` perf lint / 用户 `macro` / 局部变量 attribute | 视需 | ⬜ Deferred |

## PR4a · applied generator 最终引擎（方案 B post-bind，当前，🟡 进行中）

**目标**：落地 design.md 的 Generator splice 模型（applied 那半）——generator 吐**源码文本**、按 DeclId 引用，
合并器施加 Add/Replace/Augment → **重新 parse+typecheck「用户码 ∪ 生成码」**。**方案 B（最终架构，User 2026-08-21
裁决）**：post-bind 执行（能查解析后符号），**参考 C# Roslyn source generator**（post-compilation 查 semantic
model / 只 add / union 重 bind / 单轮→有界多轮）。`Replace`/`Augment` 是 z42 超 C# 的增量。零 bump。z42c 自建
不注入 generator → gated off → **byte-identical**。

### 关键设计决策（User 2026-08-21 已确认；design.md §Generator 实现落法 是权威）

- **D-a Augment 落法 = 脱糖 synthetic partial + 自动标原类型 partial**：`Augment(T, members)` → 生成
  `partial <kind> T { members }` 新 CU（AddSource 追加进 `cus[]`）+ **在原 CU 的 `T` ClassDecl 上自动设
  `IsPartial=true`**（规避 E0430「所有碎片须标 partial」，用户免写 partial——卖点）。partial 合并
  （`SymbolCollector._passClassStubs` 符号层 + `IrDump._buildMergedPartial` AST 层，都遍历整个 `cus[]`、
  typecheck 前跑、支持 class/struct/record/interface）自然把成员并进 T。**既参考 C# partial 合并、又避开
  start-only span**（不改现有源码字节）。
- **D-b generator 来源 = 测试注入实例**（本 PR）：引擎真实接进 `BuildPackageCus`，但 generator 实例由集成测试
  `new TestGen()` 注入（对镜 PR3a 测 NoEmptyCatchAnalyzer）。**编译内自动发现 `:Generator` 类 + 外部 zpkg
  加载 → 延后 PR4d**（需「编译 generator→load→Activator」机制，与外部加载同源）。
- **D-c 触发名 = `Generator.AppliedName()`**（返回 == 类名剥 `Generator` 后缀）：**实测转向**——对 `Generator`
  接口引用 `x as Object` 的运行期 checked-cast 在自举 VM 返回 Null（反射取实例类名崩），故契约加显式
  `AppliedName()` 报触发名。D8 命名规则不变（类名仍须 `<Trigger>Generator`，E0447 强制），约定
  `AppliedName()==类名剥后缀` 由文档 + E0447 双保。
- **D-d DeclId**：保留 `Key` 字符串 + 建 `Key→Decl` 反查（Replace 定位原节点）；method key **加 arity** 消歧重载。

### 数据流（接进 `IrDump.BuildPackageCus`）

```
BuildPackageCus(texts, files, count, cus, ...):
  symbols0 = coll.CollectAll(cus, ...)                 // #1 provisional bind（Roslyn: semantic model）
  gr = GeneratorDriver.Run(inp.Generators, cus, count, symbols0)
       · 每 gen：类名剥后缀→触发名→遍历 cus 找 [触发名]@decl（AttributedDecl）
       · 调 gen.Generate(GenTarget{DeclId, decl+符号}, sink)
       · GenSink 收集：AddSource / Replace(DeclId) / Augment(DeclId)
  if gr.HasOutput:
       cus' = applyGenOps(cus, gr)                     // Augment→partial 新 CU+标原 T partial；Replace→AST 替换；AddSource→新 CU
       // 现有流程照常在 union 上（symbols 从头重收）
  symbols = coll.CollectAll(cus', ...)                 // #2 real bind on union
  _buildMergedPartial + typecheck + IrGen → zpkg
  （gr.HasOutput=false → cus'==cus → 与现状逐字节一致）
```

### 实施

- [ ] 1. 契约 `z42c.semantics/src/Generation.z42`（NEW，**注意在 semantics 非 syntax**——方案 B 的 `GenTarget`
      须暴露解析后 `Z42ClassType`，syntax 层引用不到语义符号；与 design.md 原稿含 `TypeSymbol`/`SymbolKind`
      一致、更贴 Roslyn）：`interface Generator { void Generate(GenTarget t, GenSink sink); }` + `interface GenSink
      { void AddSource(string hint, string src); void Replace(DeclId id, string src); void Augment(DeclId id,
      string membersSrc); }` + `interface GenTarget`（DeclId/Kind/被贴 AttributedDecl AST + 解析 `Z42ClassType`）。
      无 delegate（[[z42c-no-cross-pkg-delegates]]）。`ModuleGenerator`/`GenContext` 留 PR4c。`DeclId` 留在
      semantics（HandlerRegistry），无需搬。
- [ ] 2. `DiagnosticCodes.z42`：`GeneratorSuffixRequired = "E0447"`（避开 generic-methods #240 占的 E0446）。
- [ ] 3. `SymbolCollector._passGeneratorSuffixEnforce`（3 挂载点，同 `_passAnalyzerSuffixEnforce`）：
      `_baseHasSimpleName(c, "Generator") && !c.Name.EndsWith("Generator")` → E0447。纯语法层。
- [ ] 4. `HandlerRegistry`：`KindOf` 加 Generator 分支（本 PR generator 靠注入，KindOf 的 Generator 判定=引擎侧
      按注入 generator 的触发名集，非全局——`IsGeneratorAttr(name, injectedSet)`）；`DeclId` 加 `Key→Decl` 反查
      helper（复用 AttributeSynth key scheme + arity）。
- [ ] 5. `GeneratorDriver.z42`（NEW，z42c.semantics）：`Run(Generator[] gens, int n, CompilationUnit[] cus, int count,
      SymbolTable symbols) → GenOutcome{ CompilationUnit[] extraCus, int extraCount, bool hasOutput }`。
      `GenSinkImpl : GenSink` 收集 ops；`_applyOps` 落地三 sink（Augment 脱糖 partial+标记、Replace AST 替换、AddSource 新 CU）；
      护栏：两 gen 碰同一 DeclId → 确定性报错（design 红线①）。
- [ ] 6. 接线 `PackageCompile.CompileInputs`：加 `public Generator[] Generators; public int GeneratorCount;`（默认空）；
      `IrDump.BuildPackageCus` 加 provisional CollectAll→driver→union（gated，空→原路 byte-identical）。
      增量缓存：spliced 文件强制 fresh 或整体走非增量 union（实现时按 agent 勘察的 `IncrementalDriver.Prepare` 约束处理）。
- [ ] 7. 集成测试 `z42c.semantics/tests/generator/*`（NEW）：`AddEqGenerator`(Augment) / `ReplaceBodyGenerator`(Replace) /
      `AddSiblingGenerator`(AddSource) 各一，测试 `new` 实例喂 `PackageCompile.Compile`（fixture + 注入）→ 断言合并后
      typecheck 过 + IrGen + **VM 跑出预期输出**（端到端）。+ `collect_tests.z42` 加 E0447 负测（缺后缀报/带后缀不报）。
- [ ] 8. docs：`design.md`（本节 B 落法细化，见 §Generator 实现落法）+ `docs/book`/`attributes.md`（generator splice 机制页）
      + `z42c.syntax`/`z42c.semantics` README 功能索引。

### GREEN（全绿）

- [ ] worktree 供种 + 重建 `xtask.zpkg`；`xtask test` 全 stage gate 绿；self-host gen1==gen2 **5/5**（gated → 不动点保持）。
- [ ] generator 集成测试（Augment/Replace/AddSource 端到端 VM run）全过；E0447 负测过。
- [ ] `xtask test bootstrap`：本 PR 零新语法/格式（纯 semantics + 新 stdlib-side 契约类在 z42c.syntax）→ 确认无越界。
      ⚠️ `Generation.z42` 进 z42c.syntax（编译器自身构成）→ 非 stdlib API 轴，但仍过 bootstrap check 确认。

### 体量提示 / 中断点

- **大 PR**：double-bind + 三 sink 落地 + partial 脱糖 + DeclId 反查 + union 接线。实施中若 union 重编触发
  增量缓存连锁、或某块明显超 tasks 估计 1.5×（workflow 阶段 6.5 中断条件 7）→ 停下与 User 重议是否再拆
  （如「引擎+Augment」先合、Replace/AddSource 后续）。

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

## PR5 子任务（`[Deprecated]` directive，🟡 进行中）

> 端到端模板 = `sealed` 位（每层皆有先例）；字段级是唯一全新形状（字段 descriptor 当前无 flags 字节）。
> spec 见 `specs/deprecated-directive/spec.md`；实现原理见 design.md「PR5 落地」。

### 阶段 A：分类 + IR 模型
- [ ] A.1 `HandlerRegistry`：`IsDeprecatedDirective(name)=="Deprecated"` 折进 `IsDirectiveAttr`（不进 `IsNativeDirective`）
- [ ] A.2 `IrModule`：`IrFunction` 复用 `MethodFlags`（+bit3 常量注释）；`IrClassDesc.Flags`（+deprecated bit）+ `DeprecationMsg`；`IrFieldDesc` 新增 `Deprecated`/`DeprecationMsg`
- [ ] A.3 IrGen：`IrGenFacts._methodFlags` 后按 attr OR bit3 + 取 msg（仿 `StubEmitter._nativeIntrinsic` 读 attr 串）；`ClassDescBuilder` 类级；字段级在字段 descriptor 构建处

### 阶段 B：格式 bump + 序列化（zbc 1.36→1.37 / zpkg 0.41→0.42）
- [ ] B.1 `ZbcFormat.z42` Minor 36→37（注释）；`ZpkgWriter.z42` Minor 41→42（注释）
- [ ] B.2 `ZbcWriter`：SIGS 写 method_flags bit3 + msg idx；TYPE 写 class flag bit + msg idx + **实例/静态字段循环各加 field-flags u8 + msg idx**
- [ ] B.3 读端 3 处 lockstep：① Rust `zbc_reader.rs`（read_type 实例+静态字段 / SIGS）+ `bytecode.rs`（FieldDesc/MethodDesc/ClassDesc 加字段）② z42c `ZbcReader.ReadTypeAt` + SIGS ③ `ZpkgReader` SIGS
- [ ] B.4 Rust 4 个版本常量（ZBC/ZPKG major+minor）+ changelog 注释行
- [ ] B.5 `docs/design/runtime/zbc.md` + `zpkg.md` changelog 各加一行；`.claude/rules/version-bumping.md` 版本表刷新（stale 1.35/0.40 → 1.37/0.42）

### 阶段 C：跨包传播（仿 sealed）
- [ ] C.1 `ExportedTypes`：`ExportedMethodZ`/`ExportedClassZ`/`ExportedFieldZ` 加 `IsDeprecated`/`DeprecationMsg`
- [ ] C.2 `TsigReconcile`：从新 flag bit + msg 池设 Exported*Z（method `&8`、class bit、field-flags）
- [ ] C.3 `Symbol`（`MethodSymbol`/`FieldSymbol`）+ `Z42Type`（`Z42ClassType`）加 `IsDeprecated`/`DeprecationMsg`
- [ ] C.4 `ImportedSymbolLoader`：仿 `IsSealed` 传播三处

### 阶段 D：use-site 检测 + 治理
- [ ] D.1 `AccessChecker.CheckDeprecated(sym, span)`：收集 hit 到 `_tc` 上的 accumulator（不直接发，留治理）
- [ ] D.2 调用点接线：`MemberResolver`（field/method/property 各 resolve 处）+ `ExprTyper`（setter/`new`）+ `CheckTypeRef`（类型引用）
- [ ] D.3 内建 `DiagRule`（id `"deprecated"`，category Usage，DefaultSeverity=Warning，EnabledByDefault=true）
- [ ] D.4 `_runDeprecation` pass（`PackageCompile`，**always-run**）：per-cu 建 SuppressionSet（抽 `AnalyzerDriver` 的 builder 复用）+ 用 `inp.Lints` → 逐 hit `DiagSinkImpl.Report(DEPRECATED_RULE, span)` → 治理统一

### 阶段 E：测试 + 验证
- [ ] E.1 z42c 单测：`zbc_tests.z42` golden hex 重截 + method/class/field deprecated 位 round-trip
- [ ] E.2 use-site 告警 + 治理单测（`tests/access-control/` 或新 `tests/deprecated/`：抑制/WAE/#suppress）
- [ ] E.3 跨包 golden：`src/tests/cross-zpkg/deprecated_imported/`（克隆 `sealed_devirt_imported`）
- [ ] E.4 fixture regen：`zbc-format/*`（`xtask build test`）+ `zpkg-format/*`（手工）；Rust `zbc_compat`/`lazy_loader`
- [ ] E.5 GREEN：`xtask test`（全 stage）+ self-host 5/5 + `xtask test bootstrap`（格式 bump 两代自举，CI 权威）
- [ ] E.6 文档同步：book attribute 页 + 相关 README（触发矩阵）

## PR6 子任务（参数默认值统一常量表示，✅ Phase A/B/D/E 完成 + F GREEN；caller 宏拆 PR6b）

> spec 见 `specs/param-default-representation/spec.md`；实现原理见 design.md「参数默认值统一常量表示 + caller 宏」节。
> **零格式-bump**（骑 zbc 1.15 param attr-ref blob 通道 + `$Default`/`$Caller:*` 哨兵）。先例：PR5 `$Deprecated`。
> **状态（2026-08-24）**：PR6（阶段 A/B/D/E）已合 main #282（self-host 5/5 逐字节 + cross-zpkg + 反射）。
> **PR6b（阶段 C + D.4 + E.2 + F.4，2026-08-25）**：caller 宏 support+注入+误用诊断 E0450，**F2-安全**（parser 译成 `IdentExpr("$macro:*")` 哨兵、不新增 syntax→semantics 跨包类型），零 Rust 格式改。同/跨包 golden + 4 语义单测本地全过。

### 阶段 A：ConstBlob 编码 + 常量折叠扩展 ✅
- [x] A.1 `ConstBlob` 编解码 helper（`z42c.semantics/src/ConstBlob.z42`，自描述递归：null/bool/int/float/char/string/enum/struct/array，长度前缀 `_seg`）
- [x] A.2 折叠**复用 `ConstEval`**（非扩 `_foldDefault`——后者已退休删除）：标量/一元二元/字符串拼接/enum 成员经 `syms.EnumConsts` / struct·array 结构递归 → 产 ConstBlob 串
- [x] A.3 非常量默认值 → `ConstBlob.Encode` 返 null（不追加 `$Default`）；核实 stdlib/编译器同包默认值均可折叠（零破坏，GREEN 证）

### 阶段 B：持久化（$Default 哨兵，零格式-bump）✅
- [x] B.1 `ClassDescBuilder._paramAttrRefs`：每参有默认值且 Encode 成功 → append `IrAttrRef{TypeName="$Default", FactoryFunc=<ConstBlob 串>}`
- [x] B.2 SIGS per-param `default_kind` 字节 → `IrGenFacts._fillParamMeta` 恒写 0（vestigial 退休；删死代码 `DefaultFold`+`_foldDefault` 族；不动 wire layout）
- [x] B.3 无 writer/reader 格式改动（复用 zbc 1.15 param attr-ref blob）；`CompilerFingerprint` 3→4

### 阶段 C：caller 宏 support（新语法，support-先行）→ PR6b ✅
- [x] C.1 Parser：`caller_member!()`/`caller_line!()`/`caller_file!()`/`module_path!()`——复用 `Bang`+`()`，**不新增 AST 节点**：`ident!()` → 既有 `IdentExpr("$macro:"+name)` 哨兵（`$` 非法于真标识符→零撞名；**避 syntax→semantics 新跨包类型撞 F2 冷启动 stale-cache**，见 design「F2-安全」注）。语义层白名单认名 + 限参数默认值位（`CallerMacro` helper，semantics 包内）
- [x] C.2 Fold：caller 宏 → 持久化 `$Caller:<kind>` 哨兵（`ClassDescBuilder._paramAttrRefs`，FactoryFunc 空）
- [x] C.3 **核实 z42c/stdlib/xtask 源不使用** caller 宏（仅加 support）→ 单 PR 不触两-nightly（grep 证）
- [x] C.4 误用诊断 **E0450**（字面量发码，避 core→semantics F2）：未知宏名 / 参数类型不符（member/file/module→string、line→int，`DeclBinder._bindMethodBody` 定义侧校验）/ 非参数默认值位（`ExprTyper._bindIdent`）

### 阶段 D：跨包读回 + 调用点注入（含修 bug）✅
- [x] D.1 `Z42FuncType` + `MethodSymbol` 加 `ParamDefaults`（+caller infra `ParamCallers`/`CallerKind`，PR6b 用）
- [x] D.2 `ImportedSymbolLoader`（≥4 建 Z42FuncType 站点，含静态实例主路径 line 292）扫 param attr-ref → `$Default` → 填符号；**顺带修 `ZpkgReader.ReadModuleSigs` 方法/参数级 attr read-and-skip 潜伏 bug（PR5 `$Deprecated` 方法级跨包同款静默失效）**
- [x] D.3 `OverloadBinder` 跨包分支：`BoundDefault(T,-1)` 零值 → ConstBlob decode→AST→`_bindExpr` 重建（**修跨包塌零 bug**）
- [x] D.4 caller 哨兵注入（PR6b）：同包 `OverloadBinder._adaptArgs` 截 `CallerMacro.IsMacro` → `_callerLiteral`；跨包 `_crossPkgDefault` 读 `sig.ParamCallers[i]`（由 `ImportedSymbolLoader` 从 `$Caller:*` 填）→ `_callerLiteral`。`_callerLiteral(kind,pty,sp)`：member=`_curMemberName`（`DeclBinder._bindMethodBody` 设）/ line=`sp.Line` / file=`sp.File` / module=`_currentNs`；字符串走三引号 raw 免转义。**读消费方调用点上下文**（同/跨包对称，cross-zpkg golden 证 namespace=消费方 `Demo.CmApp`）
- [x] D.5 同包路径保留既有 `_adaptArgs` AST 重绑（与跨包 ConstBlob 语义一致，GREEN 证）

### 阶段 E：反射迁移 ✅
- [x] E.1 Rust `reflection.rs`：`DefaultValue` 改读 `$Default` param attr-ref → `decode_const_blob_scalar`（标量 n/b/i/c/f/s→Value；聚合 e/a/t→Null Deferred）；旧 kind 元组作 fallback
- [x] E.2 caller 宏参数 DefaultValue「无固定值」→ Null（PR6b，**零 Rust 格式改**）：caller 参数无 `$Default` → `param_default_from_blob` 返 None → fallback kind 0 → `Value::Null`（已满足）。**顺带 correctness 修**：`call_attribute_factories` 过滤 `$`-前缀哨兵（否则 `$Caller:*`/`$Default` 会给 param `GetCustomAttributes()` push 一个 Null 元素——PR5/PR6 既有潜伏 leak）

### 阶段 F：测试 + 验证（GREEN 完成，rebase+bootstrap 待）
- [x] F.2 同包默认值 e2e：标量/enum/struct/数组/命名常量省略实参 → 正确值
- [x] F.3 **跨包 golden**：`src/tests/cross-zpkg/param_default_cross_pkg/`（3 包 target/ext/main，9 断言：标量/string/enum/struct/负整数/常量表达式/char/bool/数组）——回归修复，12/0
- [x] F.4 caller 宏 e2e（PR6b）：同包 golden `src/tests/types/caller_macros.z42`（member/line-delta/file/module + 显式覆盖）+ 跨包 golden `src/tests/cross-zpkg/caller_macro_cross_pkg/`（注入读消费方上下文）+ 4 语义单测 `analyzer_tests.z42`（valid 无码 / wrong-type / unknown-name / body-misuse 皆 E0450）
- [x] F.5 反射 e2e：`fold_param_defaults.z42`（含字符串拼接现折叠 "ab"）+ `z42.core reflection.z42` DefaultValue
- [x] F.6 GREEN：`xtask test` 全 stage + self-host 5/5 逐字节（base 072ca0fd）；⏭ rebase origin/main 后重跑 + `xtask test bootstrap` 无越界
- [x] F.7 文档：design.md（SoT）「caller 宏语法」节同步 F2-安全实现（IdentExpr 哨兵 + E0450）；book attribute 页补 caller 宏一节

## 备注

- 每 PR 合并前并入 main 最新 + 重跑 GREEN（parallel-development §3）。
- **语义耦合自查**：并发 worktree `z42-record [add-record-attribute]`（record 用 attribute 式声明）、
  `z42-conv [add-user-conversions]`（动 semantics）与本 change 邻接——PR2（后缀迁移）/ PR3（analyzer 触
  semantics）开工前主动对表/知会（parallel-development §4）。
- **PR2 后缀约定是破坏性迁移**（改现有 attribute 类名 + 反转 `Attribute.z42`/`basic.z42` 已记录的"无后缀"
  决定），**非 byte-identical**；会破一代自举、warm 重建自愈（D7 式）。与纯重构 PR1a/1b 分开，不混。
- 带 bump 的 PR（PR5/6）走 bootstrap-seed 两阶段纪律，bump 前 `xtask test bootstrap`。
