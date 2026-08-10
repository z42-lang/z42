# Proposal: const 关键字（编译期常量）+ 常量传播优化

## Why

z42 目前没有"编译期常量"这一层：具名常量只能用 `public static int X = 100;` 表达——它有存储、
每处引用都是一次运行期静态字段加载（`StaticGetInstr`），且值不参与优化管线的常量折叠。编译器自身的
`TokenKind` / `DiagnosticCodes` 全是这种 `static` 字段，语言使用者也缺一个表达"这就是个编译期常量"的机制。

`const` 补上这一层，并且是"用语法机制喂优化管线"系列（`readonly` #124 → `pure` #137 → **`const`**）里
**优化最干净的一环**：`const` 引用在 emit 时直接替换为字面量指令，喂给**已有的 ConstFold pass**，
无需任何新 pass 即得常量传播 + 算术折叠。在此之上再加一个**常量条件死分支消除** pass，让
`if (const false) { … }` 这类分支在编译期整块消除。

## What Changes

- **新关键字 `const`**（词法 + 语法）：
  - 静态常量字段：`class C { const int Max = 100; }`（隐式 static，无存储）
  - 局部常量：`const int x = 5;`（方法体内）
- **常量表达式初始化器**：初始化器可为字面量，或**已定义 const** 的常量表达式（`const int Y = X * 2;`），
  按声明序求值，不允许前向引用 / 环。
- **编译期求值 + 字面量替换**：`const` 引用在 codegen 时替换为对应字面量指令（`ConstI64Instr` /
  `ConstBoolInstr` / `ConstCharInstr` / `ConstF64Instr` / `ConstStrInstr` / `ConstNullInstr`），
  自动喂现有 ConstFold pass。
- **强制诊断**：const 必须有常量初始化器；非常量初始化器报错；不可对 const 赋值。
- **新优化 pass `dead-branch`**：`br.cond` 的条件寄存器是已知常量 bool（`ConstBoolInstr`）时折成
  无条件 `br`；`ExcCount==0` 的函数进一步做可达性分析移除因此不可达的块。
- **两-nightly 纪律**：本 change 只落"支持"（parser 接受 `const`）；z42c / stdlib / xtask **源码本轮不使用**
  `const`（support-first，晚一个 nightly 再 use）。测试 fixture / stdlib bench 可立即用（由当前自建 z42c 编译）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.syntax/src/TokenKind.z42` | MODIFY | 新增 `Const` int 常量 |
| `src/compiler/z42c.syntax/src/Lexer.z42` | MODIFY | 注册 `const` 关键字 |
| `src/compiler/z42c.syntax/src/DeclParser.z42` | MODIFY | `_isModifier` 接受 `Const`（字段级修饰，进 `FieldDecl.Mods`） |
| `src/compiler/z42c.syntax/src/StmtParser.z42` | MODIFY | `_isVarDeclStart` / `_parseVarDecl` 识别前导 `const` → `VarDeclStmt.IsConst` |
| `scripts/install/xtask_install_vscode.z42` | MODIFY | `_kwModifier` 加 `const`（vscode grammar ↔ Lexer 关键字一致性 gate） |
| `src/toolchain/devtools/vscode/syntaxes/z42.tmLanguage.json` | MODIFY | 重生成：storage.modifier 组含 `const`（`xtask deps install vscode` 产物） |
| `src/compiler/z42c.syntax/src/Stmt.z42` | MODIFY | `VarDeclStmt` 加 `IsConst` 字段 + `Dump` |
| `src/compiler/z42c.core/src/DiagnosticCodes.z42` | MODIFY | 新增 const 违规诊断码（E04xx） |
| `src/compiler/z42c.semantics/src/ConstValue.z42` | NEW | 编译期常量值表示（Kind + IntVal + StrVal） |
| `src/compiler/z42c.semantics/src/ConstEval.z42` | NEW | 常量表达式求值器（字面量 + 一元/二元 + 已定义 const 引用；非常量报错） |
| `src/compiler/z42c.semantics/src/Symbol.z42` | MODIFY | `FieldSymbol` 加 `IsConst` + `ConstVal`；（局部 const 走 TypeEnv） |
| `src/compiler/z42c.semantics/src/SymbolCollector.z42` | MODIFY | 收集 const 字段，求值初始化器存 `ConstVal`；隐式 static；不进实例字段布局 |
| `src/compiler/z42c.semantics/src/TypeChecker.z42` | MODIFY | const 强制诊断（需常量初始化器 / 不可赋值） |
| `src/compiler/z42c.semantics/src/ExprTyper.z42` | MODIFY | 赋值检查覆盖 const（复用 readonly `_checkReadonlyAssign` 邻近路径）；局部 const 环境 |
| `src/compiler/z42c.semantics/src/TypeEnv.z42` | MODIFY | 局部 const 值环境（名 → ConstVal），块作用域继承 |
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` | MODIFY | const 引用（字段 / 局部）→ 发射对应字面量指令替代 StaticGet / 局部加载 |
| `src/compiler/z42c.semantics/src/FunctionEmitter.z42` | MODIFY | 局部 const 声明不发射存储；引用替换环境接入 |
| `src/compiler/z42c.semantics/src/EmitContext.z42` | MODIFY | 携带 const 替换环境（字段 ConstVal 表 + 局部 ConstVal 表） |
| `src/compiler/z42c.semantics/src/OptSet.z42` | MODIFY | 新增 `Opt.DeadBranch` 位；`All` 更新；`ByName`/`ProfileDefault` 纳入 |
| `src/compiler/z42c.semantics/src/IrDeadBranch.z42` | NEW | 常量条件死分支消除 pass |
| `src/compiler/z42c.semantics/src/IrOptPipeline.z42` | MODIFY | 接线 dead-branch pass（ConstFold 之后） |
| `src/compiler/z42c.semantics/src/IrDump.z42` | MODIFY | dump/golden 默认 optSet 减 `DeadBranch`（防既有 golden 漂移，同 Inline/StackAlloc） |
| `src/compiler/z42c.syntax/tests/decl/decl_tests.z42` | MODIFY | 解析测试：const 字段修饰（`test_const_field`） |
| `src/compiler/z42c.syntax/tests/stmt/stmt_tests.z42` | MODIFY | 解析测试：局部 const（`test_const_local`） |
| `src/compiler/z42c.semantics/tests/codegen/codegen_tests.z42` | MODIFY | const 替换 / 折叠 / dead-branch 的 codegen IR 断言单测 |
| `src/compiler/z42c.semantics/tests/typecheck/typecheck_tests.z42` | MODIFY | 类型检查期诊断：const 赋值 E0418 / 引用 E0419 / 合法无错 |
| `src/compiler/z42c.semantics/tests/collect/collect_tests.z42` | MODIFY | 收集期诊断：const 字段 E0416（缺初始化）/ E0417（非常量） |
| `src/tests/optimization/const_fold_propagation/` | NEW | golden e2e：const 传播 + 折叠 |
| `src/tests/optimization/const_dead_branch/` | NEW | golden e2e：死分支消除 |
| `src/tests/const/const_basic/` | NEW | golden e2e：const 字段 + 局部 const 语义 |
| `docs/book/src/language/const.md` | NEW | 语言参考：const 语义 |
| `docs/book/src/runtime/optimization-pipeline.md` | MODIFY | const 传播 + dead-branch pass 机制/实现 |
| `docs/book/src/SUMMARY.md` | MODIFY | 挂入 const 页 |
| `src/compiler/z42c.semantics/README.md` | MODIFY | 功能索引 + 核心文件（ConstEval / IrDeadBranch） |
| `src/compiler/z42c.syntax/README.md` | MODIFY | 功能索引（const token/修饰符） |
| `docs/roadmap.md` | MODIFY | 进度 / Deferred 索引（跨包 const） |

**只读引用**：

- `src/compiler/z42c.semantics/src/IrOptInfo.z42` — 理解现有 ConstFold / TryConstFold
- `src/compiler/z42c.semantics/src/IrGenFacts.z42` — 复用 `_foldDefault` 系列折叠算术语义
- `src/compiler/z42c.semantics/src/IrLicm.z42` — CFG 铁律先例（异常边不在 CFG）
- `src/libraries/z42.ir/src/IrModule.z42` / `IrTerminator.z42` — IR 结构（ExcTable / BrCondTerm）

## Out of Scope

- **跨 zpkg const**：v1 仅同模块（const 不 emit 为字段 → 别包看不到值）。跨包需格式 bump 把 const 值写进
  导出元数据 → Deferred。（现状印证：`z42.build/BuildKinds.z42` 注释"z42c 暂不支持 const 成员跨文件静态解析"。）
- **z42c / stdlib / xtask 源码使用 `const`**：两-nightly 纪律，本轮只落支持。
- **常量表达式的前向引用 / 循环依赖求值**：v1 按声明序，只允许引用"已定义"的 const。
- **`ExcCount>0` 函数的死块移除**：只折 `br.cond→br`，不移块（CFG 铁律，异常边不在终结子 CFG）。
- **const 引用 enum 成员 / const 数组 / const 对象**：v1 仅原始类型常量（int/bool/char/float/string/null）。

## Open Questions

- [ ] 无（设计点已由 User 在阶段 1 裁决：范围=静态字段+局部；优化=替换+死分支；初始化器=字面量+已定义 const 表达式）
