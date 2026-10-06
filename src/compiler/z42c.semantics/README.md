# z42c.semantics

## 职责
语义分析 + 代码生成：`SymbolCollector`（Pass 0 符号收集）→ `TypeChecker`（Pass 1 绑定 + 类型检查）→ `Bound` 树（每节点携解析后 `Z42Type`）→ codegen（Bound→IR：`ExprEmitter` / `FunctionEmitter` / `IrGen` / `IrDump`）。出 IR 后交给 [`z42c.optimization`](../z42c.optimization/README.md) 的 IR 优化管线。后端最大的子系统，dogfood 缺口高发段。

## 功能索引
命名空间 `Z42.Semantics`。

| 功能 | 入口 |
|------|------|
| 符号收集（Pass 0） | `new SymbolCollector().Collect(cu)` → `SymbolTable` |
| 绑定 + 类型检查（Pass 1） | `new TypeChecker(diags).Infer(cu, symbols)` → `SemanticModel` |
| Bound dump | `SemanticDump.DumpBody(src, key)` / `ErrorCount(src)` |
| 源 → IR / zbc | `IrDump.ZbcBytes*` / `BuildPackage*`（包编译门面，pipeline / driver 依赖）|

## 目录结构

源码按功能分子目录。命名空间不随目录变，全部仍是 `Z42.Semantics`。
同一个 partial 类的各碎片必须在**同一个目录**：碎片按路径序合并，主碎片（路径最小者）决定合并后的字段 / 成员顺序。

| 目录 | 内容 |
|------|------|
| `src/Symbols/` | 符号表与收集：`SymbolTable`（4 碎片）、`SymbolCollector` 及各簇 pass、继承解析、导入符号加载、命名空间作用域、attribute handler 注册表、声明修饰符助手 `DeclFacts` |
| `src/Types/` | 类型模型：`Z42Type`、类型代换、转换分类、基元模型、泛型约束、类型实参推断、struct 布局 |
| `src/BoundTree/` | Bound 树节点（表达式 / 语句 / 模式）、`SemanticModel` |
| `src/Binding/` | 绑定 + 类型检查：`TypeChecker`、各 `*Typer`、`MemberResolver`（7 碎片）、重载决议、语句 / 声明 / 模式绑定、访问检查、穷尽检查、常量求值与常量 blob、编译期宏 |
| `src/Validation/` | 声明期约束（`DeclEnforcer`）、流分析（`FlowAnalyzer`）、声明位类型引用补录、多余 `using` 告警、`[ModuleInit]` 校验 |
| `src/Emission/` | Bound → IR：`IrGen*`、`EmitContext`、各 `*Emitter`、`ClassDescBuilder` |
| `src/Lowering/` | IR 级合成：record / 模块初始化 / 接口桥合成、测试索引 |
| `src/Exports/` | 导出签名（TSIG）提取、编译产物束 `CompiledModuleZ` |
| `src/Generators/` | 源码生成器与 AST 级脱糖：契约、驱动、多轮拓扑、生成上下文、内建 `[Forward]`、attribute 工厂合成、benchmark 脱糖 |
| `src/Analyzers/` | 分析器：驱动、加载、`[lints]` 决策、局部抑制 |
| `src/Compilation/` | 单 CU / 包编译编排：`CuCompile`、`CuPreprocess`、`IrDump` / `SemanticDump` 门面、包级 `global using` 告警、`ParallelFor` |

**分层**：语义层（Types / Symbols / BoundTree / Binding / Validation / Analyzers / Generators）**不引用**
emission 层（Lowering / Emission / Exports）与编排层（Compilation）；反方向随意。代码生成用到的声明 / 常量判定
放在语义层（如 `DeclFacts`、`ConstBlob`），由代码生成反过来引用。这是把 emission 层拆成独立包的前提。

## 如何测试验证
```bash
./xtask test compiler        # tests/<unit>/ 全部 [Test] 单元 + 自举不动点；全部 PASS 即通过
```
`tests/` 按子系统分单元目录（typecheck / codegen / overload / pattern / generator / analyzer / zbcreader 等），每个单元自带 `*.z42.toml`。

## 关联文档
- 设计 / 机制：[architecture.md](../../../docs/internals/src/compiler/architecture.md)、[binder-hierarchy.md](../../../docs/internals/src/compiler/binder-hierarchy.md)、[generics.md](../../../docs/internals/src/compiler/generics.md)、[access-control.md](../../../docs/internals/src/compiler/access-control.md)、[self-hosting.md](../../../docs/internals/src/compiler/self-hosting.md)

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/Types/Z42Type.z42` | 语义类型层次（Prim/Class/Func/Void/Error/Unknown）+ 数值拓宽 IsAssignableTo |
| `src/Analyzers/AnalyzerDriver.z42` | **编译期 analyzer 驱动**：`Run(analyzers,cu,bag,cfg)` 遍历 CompilationUnit AST，对命中 `ObservedKinds` 的节点调 `Analyzer.OnSyntaxNode`，诊断经 `DiagSinkImpl` 按 `LintConfig`决策 severity 后映射进 `DiagnosticBag`（抑制则丢弃；visitor 模型，无 delegate）。`Run` 从 `cu.SuppressRegions`（`#suppress` 区间）建 `SuppressionSet` + walk 期间 push/pop 声明的 `[Suppress]`（活跃栈）；`DiagSinkImpl.Report` 在 severity 决策前查 `SuppressionSet.IsSuppressed` 命中即丢弃。契约在 z42c.syntax `Analysis.z42` |
| `src/Analyzers/SuppressionSet.z42` | **局部抑制集**：`AddRegion(id,start,end)`（`#suppress` 字节区间）+ `PushActive/PopActive`（`[Suppress]` 声明活跃栈，规避 decl.Span 只覆盖首 token）+ `IsSuppressed(id,pos)`（活跃栈含 id 或 pos∈某匹配区间）。精确 Id 匹配、无通配 |
| `src/Analyzers/LintConfig.z42` | **`[lints]` severity 决策器**：`Resolve(rule)→AnalyzerSeverity`（-1=抑制）——`EnabledByDefault` 门 + `[lints]` 覆盖（精确 Id 优先于 `pkg.*` 前缀通配；`"none"`=抑制）+ `warnings-as-errors`（Warning→Error）。由 z42.project `[lints]` 中性字段（driver 侧）构造 |
| `src/Analyzers/AnalyzerLoader.z42` | **外部 analyzer zpkg 编译期加载**：`Load(zpkgPaths)→Analyzer[]`——两路组合：Path-A `AssemblyLoadContext.Default().Load→GetTypes→过滤 GetInterface("Analyzer")` 发现 FQN（reflect-only）+ Path-B 自绑 `[Native("__load_module")]` 使可调 → `Type.GetType(FullName)`+`Activator.CreateInstance`+`as Analyzer`。消费方 `[analyzers]` 段声明；PackageCompile gated 调用 |
| `src/Generators/Generation.z42` | **generator 契约**：`Generator`（applied：`AppliedName()` 触发名 + `Generate(GenTarget,GenSink)`）/ `ModuleGenerator`（module：`Generate(GenContext,GenSink)`）/ `GenSink`（`AddSource`/`Replace`/`Augment`）/ `GenTarget`（被贴声明 AST + 解析 `Z42ClassType` + 触发 `Attr`）/ `GenContext`（module 用：`TypesWith<T>()`/`MethodsWith<T>()` 强类型查询 + `Resolve(fqn)`）/ 结果类 `GenType`·`GenMethod`。放 semantics（非 syntax）——GenTarget/GenContext 暴露解析符号；无 delegate。类名须 `*Generator`（E0447，覆盖 `Generator`+`ModuleGenerator`） |
| `src/Generators/GeneratorDriver.z42` | **generator 引擎**：`Run(...)→GenOutcome`（applied）——按 `AppliedName()` 找 `[X]@类型` → `Generate` → 三 sink 落地（**Augment=脱糖 synthetic partial + 自动标原类型 partial** / **Replace=AST 按 DeclId 替换** / **AddSource=新 CU**）+ **ordering fixup**（剥触发 attr + 删 AttributeSynth 预合成 store-meta 工厂）。`RunModules(...)→GenOutcome`——建 `GenContextImpl`（`TypesWith<T>`/`MethodsWith<T>` 靠 `typeof(T)` + 剥 `Attribute` 后缀扫 AST）→ 每 module gen 跑一次 → AddSource 聚合 CU。护栏 E0448（applied 只改 trigger 命中 decl / module 仅可 AddSource）。gated（无 generator→原样）→ z42c 自举不动点不变。PackageCompile 内 double-bind（provisional CollectAll→driver→union 重编，applied+module 同 gated 块） |
| `src/Types/Conversion.z42` | **统一类型转换分类器**：`Classify(from,to,symbols)→ConvResult{Kind,Method}` 把转换分类为 Identity/ImplicitNumeric/ExplicitNumeric/Boxing/Unboxing/ImplicitRef/ExplicitRef/**UserImplicit·UserExplicit**/… 内建 None 时回退 `_classifyUser`（查 op_Implicit/op_Explicit，精确 (源,目标) 匹配）。隐式数值矩阵较严 + 用户自定义转换（`(T)x` 走用户转换 + ② 声明期冲突检测 E0440 + ③ 走中间类型诊断）。机制见 [book 类型转换](../../../docs/reference/src/language/conversions.md) |
| `src/Types/BinaryTypeTable.z42` | 运算类型规则表：OperandKind/ResultKind（int tag 替代 Func 委托）+ TypeFacts 数值谓词 + BinaryRule + Lookup/LookupUnary/ResultType |
| `src/Symbols/Symbol.z42` | 符号模型（MethodSymbol / FieldSymbol）+ Z42FuncType 签名 |
| `src/Binding/CallParams.z42` | **按名字找形参的唯一出处**：`CanName` / `Count` / `IndexOf`——本地看 `MethodDecl`、导入看 `Z42FuncType.ParamNames`。`OverloadResolver.Map`（重载决议）与 `OverloadBinder._adaptArgs`（实参归位）共用。机制见 [book 命名实参](../../../docs/reference/src/language/named-arguments.md) |
| `src/Symbols/TypeIntern.z42` | **名义身份 intern 表**：稠密 id ↔ `Z42Type`。目的不是加速查表，是让 B3/B4 把 `InterfaceNames`/`BaseNames`/`BaseName` 从**字符串**换成句柄后，「往符号表塞一个裸名」**写不出来**。id 从 1 起、0=未登记；状态按引用共享（per-file 视图各拷计数会撞 id） |
| `src/Symbols/SymbolTable.z42` | 类名→Z42ClassType / 顶层函数表 + `ResolveType`（TypeExpr→Z42Type 桥） |
| `src/Symbols/SymbolCollector.z42` | Pass 0 **hub**：3 编排入口（Collect / CollectWithImports / CollectAll）顺序调各簇 pass + imported 种子 + 共享辅助（_unwrap/_vis/_hasWord/_chkTypeRef/_methodSymbol + 静态 IsProtocolExempt/_isConvOp）+ partial 状态。实际 pass 分入下列 4 簇。机制见 [book sealed](../../../docs/reference/src/language/sealed.md) |
| `src/Symbols/StubCollector.z42` | Pass A 骨架簇：interface / enum(+常量) / class stub（arity-mangle + partial 碎片合并）/ delegate 注册——建符号表骨架使成员类型可解析兄弟类 |
| `src/Symbols/MemberCollector.z42` | Pass B 成员填充簇：字段/方法/属性/索引器签名 + regKey mangle+ **const 收集** + **转换运算符**（op_Implicit/op_Explicit RegKey 附 `$to$<ret>` 消歧 + 声明期冲突 E0440） |
| `src/Symbols/InheritanceResolver.z42` | 基链解析簇（成员填充后）：override regKey 对齐+ **sealed 语义强制**（继承 sealed 类 E0427 / override sealed E0428 / 无基 virtual E0429，`sealed`==`sealed override` 简写）+ 继承字段合并 + impl-block 合并 |
| `src/Validation/DeclEnforcer.z42` | 声明良构约束簇：**D8 类名后缀强制**（attribute E0444 / analyzer E0445 / generator E0447）+ **partial 类型/方法校验**（全标 partial / Kind 一致 / 嵌套 partial / method 配对，E0430–E0435） |
| `src/BoundTree/BoundExpr.z42` | Bound 树**主/取值表达式节点**：base `BoundExpr` + 字面量 / 插值 / ident / func-ref / 成员·索引访问 / static-get / 捕获 / default / error，virtual Dump 出含类型注解 s-expr |
| `src/BoundTree/BoundExprOp.z42` | Bound 树**运算/调用/转换/构造表达式节点**：switch-expr / seq / assign / binary·unary / conditional / call·method-group·indirect-call / new / ref-arg / cast·convert·box·is·typeof / array-new·lit / lambda |
| `src/BoundTree/BoundStmt.z42` | Bound 树**语句节点**：base `BoundStmt` + decl / return / expr-stmt / local-fn / block / if / try·catch·finally / throw / while·do-while / switch / for / foreach / break / continue；BoundSwitchCase/BoundSwitchArm 持 BoundPattern + Guard |
| `src/BoundTree/BoundPattern.z42` | Bound 树**模式节点**：base `BoundPattern` + Wildcard/Constant/Type/Binding/Positional(record 解构)/Property；携 resolved 类型 + 字段名/类型 + 绑定名 |
| `src/Binding/PatternBinder.z42` | **模式绑定**：syntax Pattern → BoundPattern。裸名两级歧义消解（类型 vs 常量 vs 绑定）+ record 位置模式 `IsRecord`/arity 校验 + 字段类型递归 + 绑定注册 TypeEnv。switch/is 共用。机制见 [book 模式匹配](../../../docs/reference/src/language/pattern-matching.md) |
| `src/Emission/PatternEmitter.z42` | **模式 lowering**：BoundPattern 递归下降 → 既有 IR（IsInstance/Eq/FieldGet/BrCond），`EmitMatch(subj,pat,matchL,failL)` 短路。**常量模式 byte-identical 旧 Eq 链（自举不动点）；位置/属性字段 field_get 直读禁 as_cast（jit 硬约束）**。switch(_emitSwitch/_emitSwitchExpr) 与 is(_emitIsPattern) 共用 |
| `src/Types/TypeEnv.z42` | 词法 scope 链（Vars StrMap）+ 全局符号表引用 |
| `src/Binding/TypeChecker.z42` | Pass 1：集中 if-is 调度 `_bindExpr`/`_bindStmt`，绑定方法体 + 类型检查 |
| `src/Binding/ForeachProtocol.z42` | **`foreach` 可迭代协议判定**：静态类型 → 走哪条发射路径（数组 / 整数索引面 / `GetEnumerator()`）。判定只看**成员面**（`MethodsOf`/`FieldsOf` 两张表），不看类型走哪条继承线——类与接口在 z42 里是两条独立继承线。计数成员两档（`Count`→`Length`，`CountOf`）也收在这里：路径选择与发射参数共用这一份。用户面见 [book 迭代](../../../docs/reference/src/language/iteration.md) |
| `src/Symbols/InterfaceClosure.z42` | **接口成员的父接口闭包查找**：`Find`（连归属接口一起给）/ `FindMethod` / `BaseAt` 沿父接口链上溯。`BaseAt` 是「父接口在**当前视角**下的形态」的唯一出口 —— 解析 `Z42InterfaceType.BaseRefs`（父接口的带实参声明形态）并把子接口的实参逐层代下去，满足性校验（`InheritanceResolver`）与成员访问（`MemberResolver`）共用它；只认 `BaseNames` 裸名的话，`interface ILeaf : IMid<int>` 上溯拿到未实例化的 `IMid`，`T` 永远换不掉（满足性校验误报、形参位漏报）。接口的 `Methods` **只装自己声明的成员**（没有「父接口方法合进子接口」那一 pass），本包与跨包都靠这份闭包找继承来的成员 —— 跨包侧的 `BaseNames` 由 TSIG 承载（`ExportedInterfaceZ.BaseNames`，存 FQ；`ImportedSymbolLoader` 恢复时截泛型实参 + 剥短名，与本包 `StubCollector._addIfaceBases` 的裸名约定对齐）。成员解析（`MemberResolver` 方法 + 属性 getter）与运算符重载（`ExprTyper._ifaceMethodClosure`）共用这一份遍历。⚠️ 只管**成员可见性**；接口之间的**赋值关系**仍不走继承链（`Z42InterfaceType.IsAssignableTo` 只比名字），见 [book 接口](../../../docs/reference/src/language/interfaces.md) |
| `src/Binding/AccessChecker.z42` | 访问权限强制：`CheckAccess` 对 private/protected/internal 成员访问 emit E0404；机制见 [book](../../../docs/internals/src/compiler/access-control.md) |
| `src/Types/GenericConstraint.z42` | 泛型约束模型：ConstraintBundle（单型参）+ ConstraintSet（一类全型参，按声明序对齐 TypeArgs） |
| `src/Types/ConstraintChecker.z42` | 泛型 where 约束：Resolve（声明期 where→ConstraintSet）+ Check（call-site `new Box<int>()` 校验）。隔离自 TypeChecker |
| `src/BoundTree/SemanticModel.z42` | 类型检查产物：符号表 + 各方法/函数体 Bound 树（key="Class.Method"/func 名） |
| `src/Compilation/SemanticDump.z42` | 纯函数工具：源 → bound s-expr / 诊断计数（[Test] + driver `--dump-bound`） |
| `src/Emission/EmitContext.z42` | **codegen 共享状态 + 低层助手**：寄存器分配 Alloc / Emit / 基本块 StartBlock·EndBlock / Fresh 标签 / 循环标签栈 PushLoop·PopLoop / Z42Type→IrType 映射（`z42.package` 的 IR 层不引用 Z42Type，映射在此）。FunctionEmitter 与 ExprEmitter 共用一个 ctx（z42 无 partial class，用拆 helper 代替） |
| `src/Emission/ExprEmitter.z42` | **表达式 lowering**：集中 if-is Emit(BoundExpr)→TypedReg。字面量 / ident·字段 / 二元（算术·比较·位·拼接）/ 一元（!·-·~）/ 赋值 / 成员 / 调用 / new / 数组索引 / is·as / **块化：短路 &&·‖、三目 ?:、??**（中途分块 + 结果寄存器 copy 汇合）。**sealed 去虚化**（`_emitCall` instance 分支，receiver 静态类型是**本地或 imported** 类（`DevirtReceiverClass`，含泛型——解包 `Z42InstantiatedType.Def`、短名走 `_classShortName` 的 `$N` 条件 arity-mangle）+ `EmitContext.ResolveSealedTarget(…, classSealed)` 在 declClass 处门控 `classSealed ‖ ms.IsSealed`（整类 sealed **或** sealed override 方法）解出目标 → `VCall` 降级直接 `Call`，`Opt.Devirt` 门控 → 解锁 `IrInline`。imported 定义类经 `Deps.Statics` 校验 FQ 真实发射，排除 TSIG 展平的继承方法）|
| `src/Emission/FunctionEmitter.z42` | **codegen 函数入口 + 语句 + 控制流**：EmitFunction（建 ctx + 形参/this/字段绑定 → 出 IrFunction）+ 集中 if-is EmitStmt；if/while/break/continue → 多块 + Br/BrCond（委托 _ctx 块管理 + _expr 表达式） |
| `src/Emission/IrGen.z42` | codegen 模块级驱动：Layouts / 接口表 / 静态初始化 → `IrGenTypeEmitter` / `IrGenAuxEmitter` 按固定顺序往 `IrGenSink` 追加 → lifted → TIDX → IrModule + IrOptPipeline |
| `src/Emission/IrGenSink.z42` | 模块级累加器（Funcs / Classes + AddFunc / AddClass）；发射顺序 = byte-identical 的唯一约束点 |
| `src/Emission/IrGenTypeEmitter.z42` | 类 / struct：**partial 主碎片（min-path）发 1 条合并 TYPE record**，成员循环（→ MemberEmitter），合成隐式 ctor、struct blob-equals、**`[Record]` 值语义**（`RecordSynth`）；impl 块方法 |
| `src/Emission/IrGenMemberEmitter.z42` | 成员 → IR 函数：方法（体 / extern 桩 / abstract 桩）、属性（计算 getter / extern getter / auto-prop 桩）、索引器 get_Item / set_Item |
| `src/Emission/IrGenAuxEmitter.z42` | 接口最小 TYPE 条目、enum 类型、delegate 类型 + Invoke 桩、自由函数 |
| `src/Lowering/RecordSynth.z42` | **`[Record]` 值语义合成**：`RecordSynthEmitter` 为 record 类型直接搭 EmitContext 发 IR（无 SemanticModel body）。class → member-wise `Equals`（**裸名** + type-exact `GetType().FullName` 门 + 逐字段短路，字段直读 other 不经 as_cast）/ `GetHashCode`（h·31+field.hash）/ 记录式 `ToString`；struct → 只 `ToString`（相等/哈希用既有 blob/native）。裸名 + 无-as_cast 是 **jit 派发/编码正确性**硬约束（见 [book record-attribute 实现原理](../../../docs/reference/src/language/record-attribute.md)）。record-class `==`/`!=` 由 `OperatorEmitter` 拦截脱糖 |
| `src/Compilation/IrDump.z42` | **IR dump/emit + 包编译门面**：源 → typecheck → IrGen → .zasm-like IR 文本 / .zbc 字节 / `CompiledModuleZ` 束。**跨包公开 API 全留本类**（`DumpFunc`/`DumpModule`/`Dump*Opt`/`DumpZbcHex*`/`ZbcBytes*`/`BuildModule`/`BuildPackage`/`BuildPackageCus`/`ParseAll`/`BuildModuleD`/`UsingsOf`/`ExtractUsings`/`ExtractExports`——pipeline/driver 依赖的接口冻结，保自举稳定）。dump/golden 路径默认 optSet = `Opt.All - Opt.Inline`（本地优化不内联，既有 golden 逐字节不变）；`DumpFuncOpt`/`DumpModuleOpt` 传显式 optSet 供内联/独立性单测。私有 CU 编译内核分出 `CuCompile`、CU 预处理/ns 遮蔽分出 `CuPreprocess`（均同包内消费→member-compile 内解析，非跨 zpkg） |
| `src/Compilation/CuCompile.z42` | **单 CU / partial 编译内核**：per-file typecheck+codegen（`_compileCu`）+ partial 碎片合并（`_buildMergedPartial`/`_mergeFragments`，主碎片发合并 TYPE record）。全私有 `internal`，仅 IrDump 门面同包内调用 |
| `src/Compilation/CuPreprocess.z42` | **CU 预处理 + 命名空间遮蔽/作用域解析辅助**：global using 注入（`_injectGlobalUsings`）、file-scoped using 强制（`_enforceFileScope`→E0436）、活跃 ns 集（`_activeNamespaces`，static call using-scoped 消歧）、local-wins 遮蔽剔除（`_filterShadowed`/`_filterShadowedFuncs`）、包内类名/ns 图（`_pkgLocalClasses`/`_pkgClassNs`）、文件 stem（`_stem`） |
| `src/Symbols/NsScope.z42` | **外围命名空间规则的唯一 SoT**：`Chain`（`A.B.C` → 由内到外的外围链）/ `IsEnclosing`（按段判外围）。解析器 ①、E0456 歧义、自由函数候选、E0436、包激活、static call 消歧都走它 |
| `src/Symbols/NsUseRecorder.z42` | 每文件「用到的命名空间」集合：`TypeChecker.Infer` 新建并挂在本文件视图（`SymbolTable.UseRecorder`），`ResolveTypeP` 外壳与表达式位绑定点往里记声明 ns；E0436 的判据 |
| `src/Symbols/DepRef.z42` | 依赖引用条目编码 `ns` / `ns#pkg`：`UsedDepNs` 的每一项；带 `#pkg` = 符号实际来自的包，DEPS 只记它。编进字符串是为了让增量 meta 经 driver 原样搬运、driver 零改动 |
| `src/Validation/DeclTypeUses.z42` | 声明位类型引用的补录：`Infer` 末尾把本文件全部声明里的 TypeExpr 在挂了记录器的视图上再解析一遍（成员签名 / 基类列表 / 接口 / delegate / 约束 / impl），只记录、不发诊断 |
| `src/Validation/UsingLint.z42` | 多余 `using` 告警：W0607 不必要（没用到 / prelude / 外围）、W0608 重复（同文件 / 已有 global using）；`CuCompile` 代码生成后调用，判据与 E0436 同一份用法集合 |
| `src/Exports/CompiledModuleZ.z42` | 带依赖编译的产物束：`Module`/`Exported`/诊断/`Namespace`/`Usings`/`UsedDepNs`/`ErrorCount`。irdump-pipeline-split 从 IrDump 抽出为独立数据文件（同包 FQN 不变，跨包消费透明） |
| `src/Binding/ConstValue.z42` | **编译期常量值**：`ConstValue{Kind, IntVal, StrVal}`，Kind 区分 `Int/Bool/Char/Float(bits)/Str/Null`——供 codegen 把 const 引用替换成对应字面量指令时选对指令 |
| `src/Binding/ConstEval.z42` | **常量表达式求值器**：AST `Expr` + 已定义 const 环境(`StrMap`) → `ConstValue`（非常量返回 null，调用方报诊断）。覆盖字面量 + 一元/二元 算术·比较·逻辑·位·串接 + 已定义 const 引用（镜像 `IrGenFacts._foldBinary` 语义） |
| `src/Binding/ExprTyper.z42` / `AssignTyper.z42` / `CollectionTyper.z42` / `ConstructTyper.z42` / `TypeOpTyper.z42` | 各类表达式的绑定与类型推断（`TypeChecker` 的 `*Typer` 分工：一般表达式 / 赋值 / 集合与索引 / new·构造 / is·as·typeof·cast）|
| `src/Binding/MemberResolver.z42`（+ `.Bare` / `.Func` / `.Prim` / `.Static` / `.Subst` / `.TypeParam` 碎片） | 成员解析：实例 / 静态 / 基元 / 型参 / 函数类型成员，含泛型代换 |
| `src/Binding/OverloadResolver.z42` / `OverloadBinder.z42`（+ `.Candidates`） | 重载决议与实参归位 |
| `src/Binding/DeclBinder.z42` / `StmtBinder.z42` | 声明（类 / 方法体 / 属性）与语句绑定 |
| `src/Binding/ExhaustCheck.z42` / `MacroRegistry.z42` / `RefArgCheck.z42` / `VarFieldInfer.z42` | switch 穷尽性检查 / 编译期宏注册 / `ref`·`out` 实参校验 / `var` 字段类型推断 |
| `src/Types/TypeSubst.z42` / `TypeArgInference.z42` / `MethodTypeArgSubst.z42` / `MethodTypeParamUse.z42` / `TypeFactsTc.z42` / `PrimModel.z42` / `StructLayout.z42` | 类型代换 / 类型实参推断 / 方法级型参代换与使用分析 / 类型谓词 / 基元模型 / struct 布局 |
| `src/Symbols/SymbolTable.Functions.z42` / `.Nominal.z42` / `.Origins.z42` | `SymbolTable` 碎片：自由函数 / 名义类型 / 符号来源 |
| `src/Symbols/ImportedSymbolLoader.z42`（+ `.Resolve`） | 把依赖 zpkg 的 TSIG 恢复成符号表条目 |
| `src/Symbols/CtorInheritance.z42` / `NestedFlatten.z42` / `PreludeNs.z42` | 构造器继承 / 嵌套类型展平 / prelude 命名空间集 |
| `src/Validation/FlowAnalyzer.z42`（+ `.Reachability`） / `DeclEnforcer.AttrArgs.z42` | 流分析（可达性 / definite assignment）/ attribute 实参校验 |
| `src/Emission/CallEmitter.z42` / `AccessEmitter.z42` / `StmtEmitter.z42` / `OperatorEmitter.z42` / `TypeOpEmitter.z42` / `StubEmitter.z42` | 调用 / 成员访问 / 语句 / 运算符（含 record `==` 拦截脱糖）/ is·as·cast / 桩函数 的 IR 发射 |
| `src/Emission/ClassDescBuilder.z42`（+ `.GenericInst`）/ `ExprEmitter.GenericInst.z42` / `GenericBodySrc.z42` / `IrGenFacts.z42` | 类描述构建 / 泛型实例化发射 / 泛型体源 / 发射期 IR 助手（类型标签、方法标志、形参元数据）|
| `src/Binding/ConstBlob.z42` | 常量 blob 编解码（`ConstBlobReader`，重载决议 / 构造器继承读默认值）|
| `src/Generators/ForwardGenerator.z42` / `GenTopo.z42` | 内建 `[Forward]` 生成器 / 多轮 generator 拓扑序 |
| `src/Lowering/IfaceBridgeSynth.z42` / `ModuleInitSynth.z42` / `RecordSynth.z42` / `TestIndexBuilder.z42` | 接口桥合成 / 模块初始化合成 / record 值语义合成 / 测试索引（TIDX）|
| `src/Generators/AttributeSynth.z42` / `BenchmarkDesugar.z42` / `GenContext.z42` | parse 后、typecheck 前的 AST 级脱糖：attribute 工厂合成 / benchmark 脱糖；generator 上下文 |
| `src/Symbols/HandlerRegistry.z42` | attribute handler 注册表（`AttrKind` 三路判定、`DeclId`、内建 generator 先于 store-meta 合成） |
| `src/Symbols/DeclFacts.z42` | 声明修饰符 / 可见性（`_hasWord` / `_visCode` / `classVis*`）/ 字面量文本助手——只看 AST 与字符串，语义层与代码生成共用 |
| `src/Validation/ModuleInitScan.z42` | `[ModuleInit]` 合法性校验（E0485 包内第二个 / E0486 标注目标非法）与站点扫描 |
| `src/Compilation/GlobalUsingLint.z42` | 包级告警：全包没有文件用到的 `global using X;` ⇒ W0607（作用于编译产物，与 `CuPreprocess._enforceFileScope` 同层）|
| `src/Compilation/ParallelFor.z42` | 包内文件级并行编译的 `ParallelFor` 机件 |

### 导出面提取（ExportedTypeExtractor）
TSIG 导出面提取：用户类/函数按 **CU 声明序**（hashed StrMap 不可迭代）+ 编译器级固定内建面静态表
（Object 四方法前置 / 11 接口 / GCHandleType / Action·Func·Predicate 委托——prelude 注入）。

**结构**（静态类 hub+spoke，同 `DepScan` 变体）：

| 文件 | 职责 |
|------|------|
| `src/Exports/ExportedTypeExtractor.z42` | **hub**：提取编排入口 Extract*/ExtractFuncs + `_extractCore`（遍历 CU 声明序调各簇）+ 共享叶子 `_unwrap`/`_hasWord`/`_requiredCount`/`_im`（被 ≥2 簇用 → 留 hub） |
| `src/Exports/ClassExtractor.z42` | 类/结构/接口提取：`_extractClass`（base 链字段·方法合并、override 保留祖先位）/ `_extractInterface` / `_fromSymbol`·`_fromImportedMethod`（method→ExportedMethodZ）/ `_indexOf` |
| `src/Exports/FuncImplExtractor.z42` | 自由函数 + trait impl 提取：`_extractFunc` / `_extractImpls` + `_fqOf`/`_typeShortName`/`_visFromMods` |
| `src/Types/TypeNameResolver.z42` | 类型名解析（纯叶子）：`TsigTypeName`（TypeExpr→TSIG）/ `SurfaceTypeName`·`_resolvedTypeName`（Z42Type→表面拼写）/ `_hybridTypeName`/`_canonName`。**公开面**供 ClassExtractor/FuncImplExtractor 及 `MemberCollector` 调用 |
| `src/Symbols/BuiltinTypeDefs.z42` | 固定 prelude 内建面：`_builtinInterfaces`/`_builtinEnums`/`_builtinDelegates` + `_iface`/`_del`/`_ims`/`_t0`·`_t1`·`_t2` 构造快捷方式 |

**设计模型**：静态类变体——spoke 与 hub 均 `static class`，用限定静态调（`ClassExtractor._extractClass` / `ExportedTypeExtractor._unwrap`）单向委回，无实例 `_ref` 字段。唯一 spoke→spoke 边 = ClassExtractor / FuncImplExtractor → `TypeNameResolver`（单向进纯叶子工具，干净分层）；其余全经 hub。跨类方法 `private→internal`。

## 依赖关系
`z42c.core`（Diagnostic/Span/DiagnosticCodes）、`z42c.syntax`（AST：Expr/Stmt/Decl + TypeExpr）、`z42.package`（IR 模型 + zbc/zpkg 后端，codegen 消费）、`z42c.optimization`（`IrGen.Generate` 末尾调 `IrOptPipeline.Run`；`Opt` 开关供 devirt 门控与 dump 路径）。stdlib 自动可用。
