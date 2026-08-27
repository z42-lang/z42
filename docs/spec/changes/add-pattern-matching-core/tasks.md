# Tasks: Rust 风格模式匹配核心（A1）

## 进度概览

| 阶段 | 内容 | 状态 |
|------|------|------|
| 0 | DRAFT + 6.5 确认 scope | ⬜ 待 User |
| 1 | Pattern AST + PatternParser（syntax 层） | ⬜ |
| 2 | switch/is 节点字段迁移 + 接线解析 | ⬜ |
| 3 | BoundPattern + PatternBinder（bind 层） | ⬜ |
| 4 | PatternEmitter（lowering，含 byte-identical 常量路径） | ⬜ |
| 5 | switch-stmt / switch-expr / is 三位点 emit 接线 | ⬜ |
| 6 | e2e 测试（三位点 × 各模式，interp+jit 双验） | ⬜ |
| 7 | 验证 + 文档同步（book 机制页 + README） | ⬜ |

## 阶段 1: Pattern AST + PatternParser（syntax 层）

- [ ] `Pattern.z42`（NEW）：`Pattern` 抽象基 + Wildcard/Constant/Type/Positional/Property/Binding 子类（含嵌套子模式数组、growable 用 `_push` 非 `List`——见 [[z42c-no-cross-pkg-delegates]]）。
- [ ] `PatternParser.z42`（NEW）`_parsePattern`：字面量→Constant；`_`→Wildcard；点分名→Constant(MemberAccess)；名后 `(`→Positional、`{`→Property、后跟 ident→Type(bind)、单名→Binding。
- [ ] `syntax/README.md`：功能索引加两文件。

## 阶段 2: switch/is 节点字段迁移 + 接线解析

- [ ] `Stmt.z42`：`SwitchCase.pattern` `Expr`→`Pattern`；加 `Expr Guard`（可空/哨兵）。
- [ ] `Ast.z42`：`SwitchArm` 同上；`IsExpr` 改持 `Pattern`。
- [ ] `StmtParser._parseSwitch`（`:74-76`）：`case` 后 `_parsePattern` + 可选 `if` 守卫。
- [ ] `ExprParser`：switch-expr arm（`:57-81`）`_parsePattern` + 守卫；`is`（`:104-113`）`_parsePattern`。
- [ ] `./xtask build compiler` 通过（AST/parser 层先自洽）。

## 阶段 3: BoundPattern + PatternBinder（bind 层）

- [ ] `BoundPattern.z42`（NEW）：镜像层级，携 resolved `Z42Type` / 字段索引 / 绑定名 / 常量 `BoundExpr`。
- [ ] `BoundStmt.z42`/`BoundExprOp.z42`：`BoundSwitchCase`/`BoundSwitchArm` `pattern`→`BoundPattern` + `Guard`；`BoundIsExpr` 持 `BoundPattern`。
- [ ] `PatternBinder.z42`（NEW）：类型解析；裸名歧义消解（类型名 vs 绑定）；位置模式校验 `IsRecord` + arity==`OwnFieldCount` + 字段类型递归；绑定注册进子 `TypeEnv`。
- [ ] `StmtBinder._bindSwitchStmt`（`:66`）/ `ExprTyper._bindSwitchExpr`（`:190`）/ `TypeOpTyper._bindIsExpr`（`:70`）：改经 `PatternBinder` bind 模式 + 守卫，绑定入对应 scope。
- [ ] `semantics/README.md`：功能索引加三文件。

## 阶段 4: PatternEmitter（lowering）

- [ ] `PatternEmitter.z42`（NEW）`Emit(subj, boundPattern, onFail)` 递归下降：Wildcard/Binding 恒真；Constant→`Eq`；Type→`IsInstance`(+bind)；Positional/Property→`IsInstance`+**直读 `FieldGet [owner,field]`**（**禁 `as_cast`+`field_get`**）+ 递归 + `BrCond` 短路。
- [ ] **⚠️ ConstantPattern byte-identical**：复刻 `_emitSwitchExpr`/`_emitSwitch` 现有常量链的指令序 + 寄存器顺序，一字不差。
- [ ] 绑定寄存器/局部分配 + 写入，供 body/guard 读。

## 阶段 5: 三位点 emit 接线

- [ ] `StmtEmitter._emitSwitch`（`:211`）：case 链改用 `PatternEmitter`，match→guard→body→end 分支编排。
- [ ] `OperatorEmitter._emitSwitchExpr`（`:141`）：arm 改用 `PatternEmitter`，match→guard→写结果寄存器→end。
- [ ] `TypeOpEmitter._emitIs`：`x is Pattern` 用 `PatternEmitter`，退化 `T`/`T x` **保持现发码**。

## 阶段 6: e2e 测试

- [ ] `src/tests/pattern-matching/pattern_core.z42`（NEW，`Assert` 范式空 stdout）：通配/常量(含枚举限定名·null)/类型/位置/属性/嵌套/守卫/绑定作用域，× switch-stmt + switch-expr + is 三位点。
- [ ] `./xtask test e2e --file pattern_core --no-build`（interp + **jit 双验**）。
- [ ] 现有 `switch*` / `is` 回归全绿。

## 阶段 7: 验证 + 文档同步

- [ ] `./xtask build compiler`（z42c 自建，catch z42 错）。
- [ ] `xtask test bootstrap`（上一 nightly 编当前源 → 无语法/格式越界）。
- [ ] `docs/book/src/language/pattern-matching.md`（NEW）：文法表、裸名歧义规则、record 位置解构原理、lowering 数据流（mermaid + 伪代码）、byte-identical/jit 双验坑。
- [ ] full `xtask test` gate 交 CI（本机 z42vm 退出期挂起，见记忆）；盯 CI 自举不动点 + test-vm-jit 绿。
- [ ] 归档 change + 更新记忆（A1 完成、A2 待推）。

## 备注

- **无格式 bump、无 runtime 改动、无新关键字**。若 IMPL 期发现需改 runtime/格式 → 停下与 User 对齐（超 scope）。
- 每个逻辑单元单独 commit（AST/parser、bind、emit、测试、docs 可分提）；PR body 附验证段（含 CI 不动点证据）。
