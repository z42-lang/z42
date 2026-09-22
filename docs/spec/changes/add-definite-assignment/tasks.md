# Tasks: definite assignment

> 状态：🟡 规范就绪，实施中 | 创建：2026-09-22
> 类型：`lang` —— 纯语义层，无运行期 / 无格式变化
> 参考实现：`git show f8ff73d59^:src/compiler/z42.Semantics/TypeCheck/FlowAnalyzer.cs`（528 行）

## 进度概览
- [ ] 阶段 1: `AlwaysReturns`（正常结束分析）
- [ ] 阶段 2: DA 状态机 + reads 遍
- [ ] 阶段 3: 接到 `_bindMethodBody`
- [ ] 阶段 4: **摸底**（Q1）—— 逐条判读，误报零容忍
- [ ] 阶段 5: 用例 + GREEN + 文档 + PR

## 阶段 1: AlwaysReturns
- [ ] 1.1 `FlowAnalyzer.z42` 新建；`AlwaysReturns(BoundStmt)` —— return / throw / 两支都退出的 if / 必定退出的 block
- [ ] 1.2 对照老实现的 "Never-return cases"（while 恒 false 等）逐条核

## 阶段 2: DA
- [ ] 2.1 状态：`uninit` / `assigned` 两个名字集合
- [ ] 2.2 语句遍：VarDecl / ExprStmt / Return / If / While / DoWhile / For / Foreach / Switch / Try / Block / Throw / Break / Continue / LocalFunction / DeconstructDecl
- [ ] 2.3 表达式 reads 遍：Ident 命中 uninit 且不在 assigned → E0407；其余节点递归子表达式
- [ ] 2.4 赋值收集：`BoundAssign` 的目标是 `BoundIdent` → 记入 assigned（**先查 RHS 的 reads 再记**）
- [ ] 2.5 join 规则六条（design §D1）
- [ ] 2.6 未知节点保守当「已赋值」（design §D4）
- [ ] 2.7 lambda 体只做 reads、不回传赋值（design §D5）

## 阶段 3: 接线
- [ ] 3.1 `DeclBinder._bindMethodBody` 绑定完调用；形参预置 assigned

## 阶段 4: 摸底（Q1）
- [ ] 4.1 跑**全量**（`xtask test all`，不是只 `build stdlib`）
- [ ] 4.2 逐条判读命中：真 bug / 需改写 / **误报（→ 回头补规则，零容忍）**
- [ ] 4.3 结论写回 proposal Q1

## 阶段 5
- [ ] 5.1 用例覆盖 spec 全部场景
- [ ] 5.2 `xtask test all` + `cargo test --lib` + docs
- [ ] 5.3 `error-codes.md` 的 E0407 从「⚠️ 零发射点」改实装
- [ ] 5.4 PR + auto-merge
