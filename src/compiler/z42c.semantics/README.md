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
| `src/IrOptInfo.z42` | **IR 优化基石**：逐 opcode 的写寄存器 `DstId`/`AddDef` / 读寄存器 `AddReads`+`AddTermReads`（镜像 ZbcWriter._regtInstr 保完整）/ 可删性 `IsPure`（白名单，未知 opcode 保留）/ retarget `SetDst`（copy-prop）/ `TryConstFold`（const-fold 规则表，可扩展）。optimization-pipeline |
| `src/OptSet.z42` | **可独立开关的具名优化位集**（`Opt` static class；add-compiler-inlining）：`ConstFold=1/CopyProp=2/Dce=4/Inline=8/All=15` + `Has`/`ByName`/`ProfileDefault(isRelease)`（debug=None/-O0、release=All）/`Resolve`（CLI>toml>profile）|
| `src/IrOptPipeline.z42` | **编译期 IR 优化管线**（IrGen.Generate 末尾）：`Run(m, optSet)` 按 `Opt.Has` 门控每 pass（`None`→整体跳过=-O0）。先模块级 inline（`IrInline.Run`，靠前产更多下游机会），再逐函数 const-fold → copy-prop → temp-DCE（各自重算 reads/defs → 单独开也正确，design D2）。interp-first，见 book optimization-pipeline |
| `src/IrInline.z42` | **函数内联 pass**（`Opt.Inline`；Phase 2）：模块级逐 caller 展开合格直接调用点。v1 curated 集（const/copy/算术/比较/位·一元/convert/field_get），callee 须单块+RetTerm/非递归/无异常表·varargs/精确 arity；offset=caller.MaxReg 重映射寄存器 + reg_types 同步扩 + 稳定序（自举不动点）。**只读形参直代入实参寄存器**（`InlineCtx`/`_writtenParams`，免 param copy，clean-inline-copies）。资格/展开见 design D4/D5 |

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
