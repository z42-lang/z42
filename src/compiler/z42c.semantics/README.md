# z42c.semantics

## 职责
镜像 C# [z42.Semantics](../../compiler/z42.Semantics/README.md) 的**类型检查半**：`SymbolCollector`（Pass 0 符号收集）→ `TypeChecker`（Pass 1 绑定 + 类型检查）→ `Bound` 树（每节点携解析后 `Z42Type`）。codegen（Bound→IR）是另一半，待 z42c.ir map 后单独设计。首个硬子系统，dogfood 缺口高发段。

## 核心文件
| 文件 | 职责 |
|------|------|
| `src/Z42Type.z42` | 语义类型层次（Prim/Class/Func/Void/Error/Unknown）+ 数值拓宽 IsAssignableTo |
| `src/BinaryTypeTable.z42` | 运算类型规则表：OperandKind/ResultKind（int tag 替代 Func 委托）+ TypeFacts 数值谓词 + BinaryRule + Lookup/LookupUnary/ResultType |
| `src/Symbol.z42` | 符号模型（MethodSymbol / FieldSymbol）+ Z42FuncType 签名 |
| `src/StrMap.z42` | 非泛型 hashed map（string→object，开放寻址）—— 规避类字段泛型限制 |
| `src/SymbolTable.z42` | 类名→Z42ClassType / 顶层函数表 + `ResolveType`（TypeExpr→Z42Type 桥） |
| `src/SymbolCollector.z42` | Pass 0：两阶段建类 stub → 填字段/方法签名 + 顶层 func；**partial 类型跨碎片合并**（同名碎片并成单一 `Z42ClassType`/`Z42InterfaceType` + 校验 E0430–E0435 + partial method 配对/擦除）|
| `src/Bound.z42` | Bound 树节点（lit/ident/assign/call/binary/unary + decl/return/expr/block/if/while/break/continue），virtual Dump 出含类型注解 s-expr |
| `src/TypeEnv.z42` | 词法 scope 链（Vars StrMap）+ 全局符号表引用 |
| `src/TypeChecker.z42` | Pass 1：集中 if-is 调度 `_bindExpr`/`_bindStmt`，绑定方法体 + 类型检查 |
| `src/GenericConstraint.z42` | 泛型约束模型（2B）：ConstraintBundle（单型参）+ ConstraintSet（一类全型参，按声明序对齐 TypeArgs） |
| `src/ConstraintChecker.z42` | 泛型 where 约束（2B）：Resolve（声明期 where→ConstraintSet）+ Check（call-site `new Box<int>()` 校验）。隔离自 TypeChecker（镜像 C# `TypeChecker.Generics.cs`） |
| `src/SemanticModel.z42` | 类型检查产物：符号表 + 各方法/函数体 Bound 树（key="Class.Method"/func 名） |
| `src/SemanticDump.z42` | 纯函数工具：源 → bound s-expr / 诊断计数（[Test] + driver `--dump-bound`） |
| `src/EmitContext.z42` | **codegen 共享状态 + 低层助手**：寄存器分配 Alloc / Emit / 基本块 StartBlock·EndBlock / Fresh 标签 / 循环标签栈 PushLoop·PopLoop / Z42Type→IrType 映射（叶子 z42c.ir 不引用 Z42Type，映射在此）。FunctionEmitter 与 ExprEmitter 共用一个 ctx（z42 无 partial class，拆 helper 替代 C# 的 partial） |
| `src/ExprEmitter.z42` | **表达式 lowering**（CG-1A–1E 全）：集中 if-is Emit(BoundExpr)→TypedReg。字面量 / ident·字段 / 二元（算术·比较·位·拼接）/ 一元（!·-·~）/ 赋值 / 成员 / 调用 / new / 数组索引 / is·as / **块化：短路 &&·‖、三目 ?:、??**（中途分块 + 结果寄存器 copy 汇合）|
| `src/FunctionEmitter.z42` | **codegen 函数入口 + 语句 + 控制流**：EmitFunction（建 ctx + 形参/this/字段绑定 → 出 IrFunction）+ 集中 if-is EmitStmt；if/while/break/continue → 多块 + Br/BrCond（委托 _ctx 块管理 + _expr 表达式） |
| `src/IrGen.z42` | codegen 模块级驱动：遍历 cu + SemanticModel → 逐函数 FunctionEmitter + StringPool intern + IrClassDesc → IrModule；**partial 主碎片（min-path）发 1 条合并 TYPE record**（class + interface），非主碎片只发方法体 |
| `src/IrDump.z42` | 纯函数工具：源 → typecheck → IrGen → .zasm-like IR 文本（[Test] + driver `--dump-ir` 后续）。dump/golden 路径默认 optSet = `Opt.All - Opt.Inline`（本地优化不内联，既有 golden 逐字节不变）；`DumpFuncOpt`/`DumpModuleOpt` 传显式 optSet 供内联/独立性单测 |
| `src/IrOptInfo.z42` | **IR 优化基石**：逐 opcode 的写寄存器 `DstId`/`AddDef` / 读寄存器 `AddReads`+`AddTermReads`（镜像 ZbcWriter._regtInstr 保完整）/ **读操作数重写 `ReplaceReads`+`ReplaceTermReads`**（按 remap 改写读操作数，供 use-site copy-prop / CSE 复用）/ **CSE value-number `CseKey`+`DstReg`**（纯计算 op 的 `op|操作数ids` key + dst 提取）/ 可删性 `IsPure`（白名单，未知 opcode 保留）/ retarget `SetDst`（copy-prop）/ `TryConstFold`（const-fold 规则表，可扩展）。optimization-pipeline |
| `src/OptSet.z42` | **可独立开关的具名优化位集**（`Opt` static class；add-compiler-inlining）：`ConstFold=1/CopyProp=2/Dce=4/Inline=8/Cse=16/Licm=32/StackAlloc=64/LoopAllocReuse=128/ReadonlyLoad=256/PureCall=512/DeadBranch=1024/All=2047` + `Has`/`ByName`/`ProfileDefault(isRelease)`（debug=None/-O0、release=All）/`Resolve`（CLI>toml>profile）|
| `src/ConstValue.z42` | **编译期常量值**（add-const-keyword）：`ConstValue{Kind, IntVal, StrVal}`，Kind 区分 `Int/Bool/Char/Float(bits)/Str/Null`——供 codegen 把 const 引用替换成对应字面量指令时选对指令 |
| `src/ConstEval.z42` | **常量表达式求值器**（add-const-keyword）：AST `Expr` + 已定义 const 环境(`StrMap`) → `ConstValue`（非常量返回 null，调用方报诊断）。覆盖字面量 + 一元/二元 算术·比较·逻辑·位·串接 + 已定义 const 引用（镜像 `IrGenFacts._foldBinary` 语义） |
| `src/IrDeadBranch.z42` | **常量条件死分支消除 pass**（`Opt.DeadBranch`，add-const-keyword）：单赋值 `ConstBoolInstr` 条件的 `br.cond`→无条件 `br` 折叠 + `ExcCount==0` 时可达性 BFS 移不可达块。**`ExcCount>0` 只折不移**（CFG 铁律：异常隐式边不在终结子 CFG，镜像 IrLicm 跳过）。见 book optimization-pipeline |
| `src/IrPureFunctionTable.z42` | **纯函数推断**（add-pure-call-opt）：`PureTable`（funcName 集）+ `Compute(m)` 模块**单调不动点**（与 escape 相反：乐观全纯→发现副作用/读可变/抛/调非纯→降级→收敛；StrMap 无 Remove 故每轮重建）。`pure(f)`=每指令 IsPure∪对纯函数 call∪readonly-fget 且无 throw 终结。供 CSE/LICM 判纯调用可消重/外提。无体/imported→保守非纯 |
| `src/IrEscapeSummary.z42` | **跨过程参数逃逸摘要**（add-crossproc-escape-summary）：`ParamEscapeTable`（funcName→`ParamFlags(bool[ParamCount])`，参数槽含 this=槽0）+ `Compute(m)` 模块**单调不动点**（乐观全 false→逃逸的置 true→收敛）。供 IrEscapeAnalysis 把「传进静态调用的实参」从「一律逃逸」精确成「按 callee 摘要逐判」。无体 stub 不登记→调用点保守 |
| `src/IrEscapeAnalysis.z42` | **逃逸分析栈上分配 pass**（`Opt.StackAlloc`，入 All；add-escape-analysis-stack-alloc）：CFG-free 流不敏感 may-escape 过近似——`ComputeEscapedRegs(m,f,table)`（Pass A 角色感知逃逸汇点规则表 `_markEscaping` 入种子 + Pass B copy 传递闭包）。**跨过程摘要**（add-crossproc-escape-summary）：`CallInstr`(args[i]→槽 i)/`ObjNew`(args[i]→ctor 槽 i+1) 实参按 `ParamEscapeTable` 逐判；VCall/CallIndirect/builtin/闭包/跨包保守全标。对象合格前提 = ctor 摘要槽 0 不逃逸（原 `_ctorLeaksThis` 并入）。不逃逸+单赋值 `ObjNew`/`ArrayNew`/`ArrayNewLit`→`StackAlloc=true`。见 book escape-analysis-stack-alloc |
| `src/IrLoopUtil.z42` | **自然循环分析共享机件**（供 IrLicm + IrLoopAllocReuse）：`LoopCfg`（后继/前驱/支配）+ `BuildCfg`（<2 块 / 有异常表 → null）+ `Headers`（回边目标）+ `LoopBody`（并同 header 多回边体）+ `CleanPreheader`（唯一循环外 `br h` 前驱）+ `BlockIdx`。从 IrLicm 抽出，逐字节等价 |
| `src/IrLicm.z42` | **循环不变量外提 pass**（`Opt.Licm`，入 All）：CFG/循环机件复用 `IrLoopUtil` + 不变量（IsPure + 单赋值 dst + 操作数不在 **header 支配域**内定义）+ 外提。**跳过有异常表的函数**（`ExcCount>0`——CFG 不含异常隐式边）。**add-readonly-fields-opt**：`Run(f, optSet)` 增 `_isHoistableReadonlyFget` 分支（`Opt.ReadonlyLoad` 门控）——接收者 `this`（reg0 恒非空）的 readonly `field_get` + 循环体内该字段无 `field_set` → 外提。**add-pure-call-opt**：`Run(f, optSet, pureTable)` 增 `_isHoistablePureCall` 分支（`Opt.PureCall` 门控）——纯 `CallInstr`（callee 在纯表、含 no-throw）+ args 全循环不变 → 外提。见 book optimization-pipeline |
| `src/IrLoopAllocReuse.z42` | **循环内分配 hoist + 对象复用 pass**（`Opt.LoopAllocReuse=128`，入 All；escape 之后；add-loop-alloc-hoist-reuse）：复用 `IrLoopUtil`，把循环体内**迭代内可复用**的 `ObjNew`/`ArrayNew`（C1 StackAlloc + C2 前向 copy 闭包无多赋值 = 不跨迭代携带 + C3 数组 Size 循环不变 + C4 对象 ctor 单块/数组常量下标读前写全）hoist 到 pre-header 只分配一次 + 循环体重初始化（对象=空 ctor 名裸分配 + `Call ctor(%r,args)`；数组=整条移走）。无格式 bump。主正确性门=`--no-opt loop-alloc-reuse` 开/关对拍 |
| `src/IrOptPipeline.z42` | **编译期 IR 优化管线**（IrGen.Generate 末尾）：`Run(m, optSet)` 按 `Opt.Has` 门控每 pass（`None`→整体跳过=-O0）。先模块级 inline（`IrInline.Run`，靠前产更多下游机会），再逐函数 const-fold → **licm**（`IrLicm.Run` 循环不变量外提）→ **cse**（`_passCse` 块内 value-number 去重）→ copy-prop（producer-retarget + **use-site 级联** `_passCopyPropUse`，靠 `ReplaceReads`）→ temp-DCE（各自重算 reads/defs → 单独开也正确，design D2）。**add-readonly-fields-opt**：licm/cse 两 pass 的触发改为 `Licm||ReadonlyLoad` / `Cse||ReadonlyLoad`，readonly `field_get` 消重/外提分支由 `ReadonlyLoad` 位单独门控（CSE 侧 `_collectWrittenFields` 失效被写字段）。**add-pure-call-opt**：`Run` 在 per-函数 pass 前算 `IrPureFunctionTable.Compute(m)` 传下去；licm/cse 触发再加 `||PureCall`，纯 `CallInstr` 消重/外提由 `PureCall` 位门控（纯调用无需失效表）。interp-first，见 book optimization-pipeline |
| `src/IrInline.z42` | **函数内联 pass**（`Opt.Inline`；Phase 2）：模块级逐 caller 展开合格直接调用点。curated 集（const/copy/算术/比较/位·一元/convert/field_get），callee 非递归/无异常表·varargs/精确 arity；offset=caller.MaxReg 重映射寄存器 + reg_types 同步扩 + 稳定序（自举不动点）。**只读形参直代入实参寄存器**（`InlineCtx`/`_writtenParamsAll`，免 param copy，clean-inline-copies）。**Phase A 单块就地 splice + Phase B 多块 split+insert**（`InlineState`/`_spliceMultiBlock`/`_cloneCalleeBlock`，含控制流 callee 拆块内联、唯一 relabel、Ret→续延块，inline-multiblock）。资格/展开见 design D4/D5 |

## 入口点
`new TypeChecker(diags).Infer(cu, symbols)` → `SemanticModel`（先 `new SymbolCollector().Collect(cu)` 出 `SymbolTable`）。便捷封装见 `SemanticDump.DumpBody(src, key)` / `ErrorCount(src)`。

## 依赖关系
→ z42c.core（Diagnostic/Span/DiagnosticCodes）, z42c.syntax（AST：Expr/Stmt/Decl + TypeExpr）, z42c.ir（codegen 半，当前未消费）。stdlib 自动可用。

## 增量进度
1A 最小类型检查 ✅ / 1B 运算+控制流 ✅ / 1C 调用+receiver+继承 ✅ / 1D is·as·new·数组 ✅ / 1E 三目·?? ✅ / 2A 泛型类·方法·实例化 ✅ / **2B where 约束求解（可行子集：base-class/class/struct/型参引用+互斥；interface·enum·new()·func 延后）✅**。下一步 codegen（Bound→IR，需先 map z42c.ir，单独 design）。

## ExportedTypeExtractor（port-z42c-tsig，2026-06-10）
TSIG 导出面提取：用户类/函数按 **CU 声明序**（hashed StrMap 不可迭代）+ 编译器级固定内建面静态表
（Object 四方法前置 / 11 接口 / GCHandleType / Action·Func·Predicate 委托——镜像 C# SymbolCollector
prelude 注入，C# 同源字节实测校准）。
