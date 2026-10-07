# z42c.emission

## 职责
代码生成 + 编译编排：把 [`z42c.semantics`](../z42c.semantics/README.md) 产出的 `Bound` 树降成 IR（`IrGen` /
各 `*Emitter` / IR 级合成），出 IR 后交 [`z42c.optimization`](../z42c.optimization/README.md) 的 IR 优化管线；
并把「前端 + 代码生成」串成单文件 / 包编译的门面（`IrDump` / `CuCompile`），产出 `.zbc` 字节与 `CompiledModuleZ` 束。
**只处理通过检查的程序**：本 CU 或包的收集期有错误就不进代码生成（`CuCompile.GenerateIfClean`）。

**不做**：符号收集、绑定、类型检查、声明期校验、TSIG 导出面提取（都在 `z42c.semantics`）；IR → IR 优化
（`z42c.optimization`）；清单解析、依赖扫描、workspace 与增量（`z42c.pipeline` / `z42c.driver`）。

## 功能索引
命名空间 `Z42.Emission`。

| 功能 | 入口 |
|------|------|
| 源 → IR 文本 dump | `IrDump.DumpFunc` / `DumpModule` / `DumpFuncOpt` / `DumpModuleOpt` |
| 源 → zbc | `IrDump.ZbcBytes*` / `DumpZbcHex*` / `BuildModuleD*`（driver `--emit-zbc` / `--dump-ir`）|
| 包编译门面 | `IrDump.BuildPackage` / `BuildPackageCus` → `CompiledModuleZ[]`（pipeline / driver 依赖）|
| 有错不生成 | `CuCompile.GenerateIfClean(gen, cu, model, optSet)` |
| Bound → IR 模块 | `new IrGen().Generate(cu, model, optSet)`（末尾调 `IrOptPipeline.Run`）|
| 单态化不收敛诊断 | `SpecializationGuard`（E0503）|
| 包级 `global using` 告警 | `GlobalUsingLint.Check`（W0607）|

## 目录结构

源码按功能分子目录。命名空间不随目录变，全部是 `Z42.Emission`。
同一个 partial 类的各碎片必须在**同一个目录**：碎片按路径序合并，主碎片（路径最小者）决定合并后的字段 / 成员顺序。

| 目录 | 内容 |
|------|------|
| `src/Emission/` | Bound → IR：`IrGen*`、`EmitContext`、各 `*Emitter`、`ClassDescBuilder`、`SpecializationGuard` |
| `src/Lowering/` | IR 级合成：record / 模块初始化 / 接口桥合成、测试索引 |
| `src/Exports/` | 编译产物束 `CompiledModuleZ` |
| `src/Compilation/` | 单 CU / 包编译编排：`CuCompile`、`CuPreprocess`、`IrDump` 门面、包级 `global using` 告警 |

**分层**：本包单向依赖 `z42c.semantics`；语义层零引用本包。代码生成用到的声明 / 常量判定放在语义层
（如 `DeclFacts`、`ConstBlob`），由本包反过来引用。

## 如何测试验证
```bash
./xtask test compiler        # tests/<unit>/ 全部 [Test] 单元 + 自举不动点；全部 PASS 即通过
```
`tests/` 按子系统分单元目录，每个单元自带 `*.z42.toml`（单元名 `z42c.emission.test.<unit>`）：

| 单元 | 覆盖 |
|------|------|
| `tests/codegen/` | 源 → IR 断言（含按 `Opt.*` 逐位开关对拍、泛型特化、prim 模型标签）|
| `tests/zbc/` | 源 → zbc 字节 |
| `tests/zbcreader/` | zbc 回读（含约束 bundle）|

## 关联文档
- 设计 / 机制：[architecture.md](../../../docs/internals/src/compiler/architecture.md)、[source-compile.md](../../../docs/internals/src/compiler/source-compile.md)、[generics.md](../../../docs/internals/src/compiler/generics.md)、[self-hosting.md](../../../docs/internals/src/compiler/self-hosting.md)
- 优化管线：[optimization-pipeline.md](../../../docs/internals/src/runtime/optimization-pipeline.md)

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/Emission/PatternEmitter.z42` | **模式 lowering**：BoundPattern 递归下降 → 既有 IR（IsInstance/Eq/FieldGet/BrCond），`EmitMatch(subj,pat,matchL,failL)` 短路。**常量模式 byte-identical 旧 Eq 链（自举不动点）；位置/属性字段 field_get 直读禁 as_cast（jit 硬约束）**。switch(_emitSwitch/_emitSwitchExpr) 与 is(_emitIsPattern) 共用 |
| `src/Emission/EmitContext.z42` | **codegen 共享状态 + 低层助手**：寄存器分配 Alloc / Emit / 基本块 StartBlock·EndBlock / Fresh 标签 / 循环标签栈 PushLoop·PopLoop / Z42Type→IrType 映射（`z42.package` 的 IR 层不引用 Z42Type，映射在此）。FunctionEmitter 与 ExprEmitter 共用一个 ctx（z42 无 partial class，用拆 helper 代替） |
| `src/Emission/ExprEmitter.z42` | **表达式 lowering**：集中 if-is Emit(BoundExpr)→TypedReg。字面量 / ident·字段 / 二元（算术·比较·位·拼接）/ 一元（!·-·~）/ 赋值 / 成员 / 调用 / new / 数组索引 / is·as / **块化：短路 &&·‖、三目 ?:、??**（中途分块 + 结果寄存器 copy 汇合）。**sealed 去虚化**（`_emitCall` instance 分支，receiver 静态类型是**本地或 imported** 类（`DevirtReceiverClass`，含泛型——解包 `Z42InstantiatedType.Def`、短名走 `_classShortName` 的 `$N` 条件 arity-mangle）+ `EmitContext.ResolveSealedTarget(…, classSealed)` 在 declClass 处门控 `classSealed ‖ ms.IsSealed`（整类 sealed **或** sealed override 方法）解出目标 → `VCall` 降级直接 `Call`，`Opt.Devirt` 门控 → 解锁 `IrInline`。imported 定义类经 `Deps.Statics` 校验 FQ 真实发射，排除 TSIG 展平的继承方法）|
| `src/Emission/FunctionEmitter.z42` | **codegen 函数入口 + 语句 + 控制流**：EmitFunction（建 ctx + 形参/this/字段绑定 → 出 IrFunction）+ 集中 if-is EmitStmt；if/while/break/continue → 多块 + Br/BrCond（委托 _ctx 块管理 + _expr 表达式） |
| `src/Emission/IrGen.z42` | codegen 模块级驱动：Layouts / 接口表 / 静态初始化 → `IrGenTypeEmitter` / `IrGenAuxEmitter` 按固定顺序往 `IrGenSink` 追加 → lifted → TIDX → IrModule + IrOptPipeline |
| `src/Emission/IrGenSink.z42` | 模块级累加器（Funcs / Classes + AddFunc / AddClass）；发射顺序 = byte-identical 的唯一约束点 |
| `src/Emission/IrGenTypeEmitter.z42` | 类 / struct：**partial 主碎片（min-path）发 1 条合并 TYPE record**，成员循环（→ MemberEmitter），合成隐式 ctor、struct blob-equals、**`[Record]` 值语义**（`RecordSynth`）；impl 块方法 |
| `src/Emission/IrGenMemberEmitter.z42` | 成员 → IR 函数：方法（体 / extern 桩 / abstract 桩）、属性（计算 getter / extern getter / auto-prop 桩）、索引器 get_Item / set_Item |
| `src/Emission/IrGenAuxEmitter.z42` | 接口最小 TYPE 条目、enum 类型、delegate 类型 + Invoke 桩、自由函数 |
| `src/Emission/SpecializationGuard.z42` | **单态化不收敛 → E0503**：值类型实例化工作表的两道判据（类型实参嵌套深度超 `DepthCap` / 工作表总量超 `IrGen.SpecializationCap`），诊断报在泛型声明处；本包唯一报用户诊断的地方——这个事实只在展开时才存在 |
| `src/Lowering/RecordSynth.z42` | **`[Record]` 值语义合成**：`RecordSynthEmitter` 为 record 类型直接搭 EmitContext 发 IR（无 SemanticModel body）。class → member-wise `Equals`（**裸名** + type-exact `GetType().FullName` 门 + 逐字段短路，字段直读 other 不经 as_cast）/ `GetHashCode`（h·31+field.hash）/ 记录式 `ToString`；struct → 只 `ToString`（相等/哈希用既有 blob/native）。裸名 + 无-as_cast 是 **jit 派发/编码正确性**硬约束（见 [book record-attribute 实现原理](../../../docs/reference/src/language/record-attribute.md)）。record-class `==`/`!=` 由 `OperatorEmitter` 拦截脱糖 |
| `src/Compilation/IrDump.z42` | **IR dump/emit + 包编译门面**：源 → typecheck → IrGen → .zasm-like IR 文本 / .zbc 字节 / `CompiledModuleZ` 束。**跨包公开 API 全留本类**（`DumpFunc`/`DumpModule`/`Dump*Opt`/`DumpZbcHex*`/`ZbcBytes*`/`BuildModule`/`BuildPackage`/`BuildPackageCus`/`ParseAll`/`BuildModuleD`/`UsingsOf`/`ExtractUsings`/`TypeFieldName`——pipeline/driver 依赖的接口冻结，保自举稳定）。dump/golden 路径默认 optSet = `Opt.All - Opt.Inline`（本地优化不内联，既有 golden 逐字节不变）；`DumpFuncOpt`/`DumpModuleOpt` 传显式 optSet 供内联/独立性单测。私有 CU 编译内核分出 `CuCompile`、CU 预处理/ns 遮蔽分出 `CuPreprocess`（均同包内消费→member-compile 内解析，非跨 zpkg） |
| `src/Compilation/CuCompile.z42` | **单 CU / partial 编译内核**：per-file typecheck+codegen（`_compileCu`）+ partial 碎片合并（`_buildMergedPartial`/`_mergeFragments`，主碎片发合并 TYPE record）+ 有错不生成的唯一闸口 `GenerateIfClean`。内核 `internal`，仅 IrDump 门面同包内调用 |
| `src/Compilation/CuPreprocess.z42` | **CU 预处理 + 命名空间遮蔽/作用域解析辅助**：global using 注入（`_injectGlobalUsings`）、file-scoped using 强制（`_enforceFileScope`→E0436）、活跃 ns 集（`_activeNamespaces`，static call using-scoped 消歧）、local-wins 遮蔽剔除（`_filterShadowed`/`_filterShadowedFuncs`）、包内类名/ns 图（`_pkgLocalClasses`/`_pkgClassNs`）、文件 stem（`_stem`） |
| `src/Exports/CompiledModuleZ.z42` | 带依赖编译的产物束：`Module`/`Exported`/诊断/`Namespace`/`Usings`/`UsedDepNs`/`ErrorCount`。独立数据文件，跨包消费透明 |
| `src/Emission/SigTypeNames.z42` | 已解析类型 → SIGS 签名拼写（类 / 接口 / enum 写 FQN，构造泛型与实例化接口带实参）；方法签名与 extern / abstract 桩共用 |
| `src/Emission/CallEmitter.z42` / `AccessEmitter.z42` / `StmtEmitter.z42` / `OperatorEmitter.z42` / `TypeOpEmitter.z42` / `StubEmitter.z42` | 调用 / 成员访问 / 语句 / 运算符（含 record `==` 拦截脱糖）/ is·as·cast / 桩函数 的 IR 发射 |
| `src/Emission/ClassDescBuilder.z42`（+ `.GenericInst`）/ `ExprEmitter.GenericInst.z42` / `GenericBodySrc.z42` / `IrGenFacts.z42` | 类描述构建 / 泛型实例化发射 / 泛型体源 / 发射期 IR 助手（类型标签、方法标志、形参元数据）|
| `src/Lowering/IfaceBridgeSynth.z42` / `ModuleInitSynth.z42` / `RecordSynth.z42` / `TestIndexBuilder.z42` | 接口桥合成 / 模块初始化合成 / record 值语义合成 / 测试索引（TIDX）|
| `src/Compilation/GlobalUsingLint.z42` | 包级告警：全包没有文件用到的 `global using X;` ⇒ W0607（作用于编译产物，与 `CuPreprocess._enforceFileScope` 同层）|

## 依赖关系
`z42c.core`（Diagnostic/Span/DiagnosticCodes）、`z42c.syntax`（AST）、`z42.package`（IR 模型 + zbc/zpkg 写出）、
`z42c.optimization`（`IrGen.Generate` 末尾调 `IrOptPipeline.Run`；`Opt` 开关供 devirt 门控与 dump 路径）、
`z42c.semantics`（符号表 / `Bound` 树 / `FrontEnd` / TSIG 导出面提取）。stdlib 自动可用。
层次（自下而上）：`z42.package` → `z42c.optimization` → `z42c.semantics` → **`z42c.emission`** → `z42c.pipeline` → `z42c.driver`；`z42c.semantics` 不依赖 `z42c.optimization`，二者都被本包依赖。
被依赖：`z42c.pipeline`（`PackageCompile` 调 `IrDump.BuildPackageCus`）、`z42c.driver`（`--emit-zbc` / `--dump-ir`）。
