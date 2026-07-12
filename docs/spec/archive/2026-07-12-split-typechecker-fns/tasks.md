# Tasks: TypeChecker 超 60 行函数拆子函数（refactor）

> 状态：🟢 已完成 | 完成：2026-07-12

**变更说明：** compiler-review P1——TypeChecker 内超 60 行硬限的分派巨函数（_bindExpr 278 /
_bindStmt 218）按表达式/语句大类抽私有子函数（同类内，不抽类，无状态穿线，最低风险）。
**原因：** code-organization.md 函数 60 行硬限；fixpoint 字节对账兜底（纯重构，零语义变化）。
**文档影响：** compiler_review.md §一 函数超限项状态。
**验证：** 编译器隔离验证（build compiler 不动点 + e2e goldens；用 flat31
清洁 stdlib，不重建他人在途 stdlib WIP）。

## 进度概览（每步不动点验证）
- [x] 1. _bindExpr：抽 _bindLiteral（6 字面量）
- [x] 2. _bindExpr：抽 _bindIdent / _bindAssign
- [x] 3. _bindExpr：抽逐节点 binder（is/typeof/as/cast/array/index/switch/ternary/nullcond/refarg/lambda-arg）
- [x] 4. _bindStmt：按 block/localfn/vardecl/return/if/try/while/dowhile/switch/for/foreach 抽逐语句 binder

## 结果
- `_bindExpr` 278 → **43 行**（纯分派）；抽出 15 个逐节点 binder（`_bindLiteral` / `_bindIdent` /
  `_bindAssign` / `_bindIsExpr` / `_bindTypeofExpr` / `_bindAsExpr` / `_bindCastExpr` / `_bindArrayNew` /
  `_bindArrayInit` / `_bindRefArg` / `_bindTernary` / `_bindSwitchExpr` / `_bindIndex` /
  `_bindNullCondMember` / `_bindLambdaArg`）。
- `_bindStmt` 218 → **38 行**（纯分派）；抽出 11 个逐语句 binder（`_bindBlock` / `_bindLocalFunction` /
  `_bindVarDecl` / `_bindReturn` / `_bindIfStmt` / `_bindTryCatch` / `_bindWhile` / `_bindDoWhile` /
  `_bindSwitchStmt` / `_bindFor` / `_bindForeach`）。ExprStmt/Throw/Break/Continue 保留内联（各 ≤5 行）。
- 验证：`build compiler` 不动点 **gen_a==gen_b byte-identical 7/7**（每批次）；聚焦 golden
  子集（basic/operators/control_flow/refs/strings/types）**83/83** 用 genB + flat31 清洁 stdlib。

## 备注
- 子系统：compiler。z42 无 partial class → 只做「同类内抽私有子函数」（不做跨类 God-Class 拆分，那需 EmitContext 式状态抽取，另开 change）。
- **本 change scope 严格限于两个分派巨函数**（_bindExpr / _bindStmt）。
- **遗留（另开 change，非本 scope）**：TypeChecker 内仍有 6 个 >60 行的**非分派**内聚函数——
  `_bindMemberCall`(98) / `_bindMember`(80) / `_bindClass`(66) / `_bindLambda`(65) / `_synthCtors`(65) /
  `_resolveParamsOverload`(61)。这些非「集中 if-is 分派」结构，拆分需按逻辑子段抽取（判断性更强，
  与本 change 的机械式分派抽取不同性质），归入后续 review 迭代。文件本身 1925 行远超 500 行文件硬限，
  属 God-Class 拆分范畴（EmitContext 式，另开 change）。
