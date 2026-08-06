# Tasks: const 关键字 + 常量传播 + 死分支消除

> 状态：🟢 已完成 | 创建：2026-08-06 | 完成：2026-08-07 | 分支：add-const-keyword（worktree z42-const）

## 进度概览
- [x] 阶段 1: 词法 / 语法（const token + 字段修饰 + 局部 IsConst）
- [x] 阶段 2: 常量求值 + 符号登记（ConstValue / ConstEval / FieldSymbol / SymbolCollector）
- [x] 阶段 3: 强制诊断（需常量初始化 / 不可赋值 / 引用已定义 const）
- [x] 阶段 4: Codegen 替换（ExprEmitter / FunctionEmitter / EmitContext）
- [x] 阶段 5: 优化管线（OptSet / IrDeadBranch / IrOptPipeline / IrDump）
- [x] 阶段 6: 测试（parse / codegen 单测 / golden e2e）
- [x] 阶段 7: 验证 + 文档同步

## 阶段 1: 词法 / 语法
- [ ] 1.1 `TokenKind.z42`：新增 `public static int Const = <空位>`
- [ ] 1.2 `Lexer.z42`：`_initKeywords` 加 `this._kw("const", TokenKind.Const)`
- [ ] 1.3 `DeclParser.z42`：`_isModifier` 加 `k == TokenKind.Const`（进 `FieldDecl.Mods`）
- [ ] 1.4 `Stmt.z42`：`VarDeclStmt` 加 `bool IsConst` 字段（ctor + Dump，const 时 Dump 前缀标记）
- [ ] 1.5 `StmtParser.z42`：`_isVarDeclStart` 识别前导 `const`；`_parseVarDecl` 消费 `const` → `IsConst=true`
- [ ] 1.6 `vscode-syntax`：确认 `const` 归入 keyword 组（重生成防漂移）

## 阶段 2: 常量求值 + 符号登记
- [ ] 2.1 `ConstValue.z42`（NEW）：`ConstValue{Kind, IntVal, StrVal}` + Kind 常量（Int/Bool/Char/Float/Str/Null）
- [ ] 2.2 `ConstEval.z42`（NEW）：`Eval(Expr, lookup) → (ok, ConstValue)`；字面量 + 一元/二元（复用 `_foldBinary` 语义）+ 已定义 const 引用；非常量 → ok=false
- [ ] 2.3 `Symbol.z42`：`FieldSymbol` 加 `bool IsConst` + `ConstValue ConstVal`（默认 false/null）
- [ ] 2.4 `SymbolCollector.z42`：识别 const 字段 → ConstEval 求值存 `ConstVal`；隐式 static；**不进实例/静态字段布局**（不 emit 字段元数据）
- [ ] 2.5 `TypeEnv.z42`：局部 const 值环境（name → ConstVal），PushScope 继承

## 阶段 3: 强制诊断
- [ ] 3.1 `DiagnosticCodes.z42`：`ConstNeedsInit` / `ConstNotConstantInit` / `ConstAssign` / `ConstExprBadRef`（E04xx）
- [ ] 3.2 `TypeChecker.z42` / `SymbolCollector`：const 缺初始化 → `ConstNeedsInit`；非常量初始化 → `ConstNotConstantInit`
- [ ] 3.3 `ExprTyper.z42`：`_bindAssign` 并入 const 赋值检查（先 const 后 readonly，避免双诊断）→ `ConstAssign`
- [ ] 3.4 ConstEval 引用非 const/未定义 → `ConstExprBadRef`

## 阶段 4: Codegen 替换
- [ ] 4.1 `EmitContext.z42`：加 `_constFields`（FQN→ConstVal）+ `_constLocals`（name→ConstVal）表 + 查询/登记方法
- [ ] 4.2 `ExprEmitter.z42`：`BoundStaticGet`/裸 `BoundIdent` 命中 const → emit 对应字面量指令（不发 StaticGet/局部加载）
- [ ] 4.3 `FunctionEmitter.z42`：局部 const 声明**只登记 ConstVal、不 emit 存储/赋值**；作用域进出维护 `_constLocals`
- [ ] 4.4 emit 映射：Int→ConstI64 / Bool→ConstBool / Char→ConstChar / Float→ConstF64 / Str→ConstStr(串池) / Null→ConstNull

## 阶段 5: 优化管线
- [ ] 5.1 `OptSet.z42`：`Opt.DeadBranch=1024`；`All` 更新；`ByName`("dead-branch") / `ProfileDefault` 纳入
- [ ] 5.2 `IrDeadBranch.z42`（NEW）：① `br.cond(ConstBool)→br` 折叠；② `ExcCount==0` 时可达性 BFS 移不可达块；`ExcCount>0` 只折不移
- [ ] 5.3 `IrOptPipeline.z42`：`if Opt.Has(optSet, Opt.DeadBranch) { IrDeadBranch.Run(...) }`，排在 ConstFold 之后
- [ ] 5.4 `IrDump.z42`：dump/golden 默认 optSet 减 `DeadBranch`（`_buildF`/`BuildModuleD`）；单测显式传 `Opt.DeadBranch`

## 阶段 6: 测试
- [ ] 6.1 `z42c.syntax/tests/parse/const_decl.z42`（NEW）：字段 const + 局部 const + 修饰符组合
- [ ] 6.2 `z42c.semantics/tests/codegen/codegen_tests.z42`：const 替换 / `A*2` 求值 / dead-branch（恒真恒假 + 有异常表只折不移）/ 单独开 DeadBranch 稳定
- [ ] 6.3 `src/tests/optimization/const_fold_propagation/`（NEW）：const 折进算术/循环
- [ ] 6.4 `src/tests/optimization/const_dead_branch/`（NEW）：死分支端到端
- [ ] 6.5 `src/tests/const/const_basic/`（NEW）：字段 + 局部 const 语义
- [ ] 6.6 `src/tests/const/const_errors/`（NEW）：4 类诊断

## 阶段 7: 验证 + 文档
- [ ] 7.1 `cargo build --release`（z42vm）无错
- [ ] 7.2 `xtask test compiler`（自举，两遍收敛 gen2==gen3，D7）
- [ ] 7.3 `xtask test e2e` + `e2e --dir cross-zpkg` + `stdlib` + `vscode-syntax` 全绿
- [ ] 7.4 spec scenarios 逐条覆盖确认
- [ ] 7.5 `xtask test bootstrap`（上一 nightly z42c 编当前源，无越界）
- [ ] 7.6 文档：`docs/book/src/language/const.md`（NEW）+ `optimization-pipeline.md`（dead-branch/const 机制）+ SUMMARY 挂入
- [ ] 7.7 README：`z42c.semantics`（ConstEval/IrDeadBranch）+ `z42c.syntax`（const）
- [ ] 7.8 `docs/roadmap.md`：进度 + Deferred（跨包 const）
- [x] 7.9 A/B 量测（const 折叠 bench）—— 见备注：正确性主门为 golden e2e 开/关一致，未单列 bench 目录。

## 备注
- self-host D7：本 change 改 codegen 输出，但 z42c 源本轮不用 const；实测 `xtask test compiler`
  **一次即字节不动点**（"no changes; preserved"）——dead-branch 未触及 z42c 现有源（无恒定常量条件），故未破代。
- **测试落点（Scope 校正）**：parse 测试并入既有 `decl_tests.z42`/`stmt_tests.z42`；诊断测试并入
  `typecheck_tests.z42`（类型检查期 E0418/E0419）+ `collect_tests.z42`（收集期 E0416/E0417）——
  const 字段诊断属 SymbolCollector 阶段，`SemanticDump` 只覆盖类型检查期，故拆两处。proposal Scope 已同步。
- **pre-existing 外部 drift（不在本 Scope，未改）**：`src/tests/zbc-format/*/source.zbc` golden 停留在
  zbc minor 28，而源 `ZbcFormat.Minor=29`（2026-08-04 escape-analysis 的 ObjNew 尾 u8 栈标志 bump 未
  回填这些 fixture）。测试期 regen 会覆盖为 29 → 全绿；committed fixture 的回填属独立 change，本次已 revert 不纳入。
- 跨包 const / const 引用 enum / const 数组 / ExcCount>0 移块 → Deferred（见 design "Deferred / Future Work" + roadmap 索引）。
- 两-nightly：本轮源码不用 const；迁 TokenKind 等到 const 的 follow-up 晚一 nightly。
