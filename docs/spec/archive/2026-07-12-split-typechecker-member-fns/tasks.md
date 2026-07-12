# Tasks: TypeChecker 成员调用/访问接收者分派抽子函数（refactor）

> 状态：🟢 已完成 | 完成：2026-07-12

**变更说明：** compiler-review P1 续——`_bindMemberCall`(98) / `_bindMember`(80) 两个按接收者
类型分派的巨函数，把「实例分派尾」（`if (rt/tt is X)` 链）纯搬移抽成 `_bindInstance*`，
`_bindInstanceMember` 再抽 class 分支保守 60 行硬限。前导静态/枚举分派保留内联。
**原因：** code-organization.md 函数 60 行硬限；接续 split-typechecker-fns（那只做了 _bindExpr/_bindStmt）。
**文档影响：** compiler_review.md §一 函数超限项状态。
**验证：** 编译器隔离（build compiler 不动点 7/7 + 聚焦 golden 138/138；flat31 清洁 stdlib）。

## 进度概览
- [x] 1. _bindMemberCall：抽 `_bindInstanceMemberCall`（实例分派尾：Class/Interface/Error/Instantiated/GenericParam/prim-wrapper）
- [x] 2. _bindMember：抽 `_bindInstanceMember`（同上）+ 其 class 分支再抽 `_bindClassMemberAccess`

## 结果
- `_bindMemberCall` 98 → **48**（保留 3 个前导 IdentExpr 静态/ns 分派 + preamble + 委托）。
- `_bindMember` 80 → **23**（保留静态字段/枚举常量分派 + preamble + 委托）。
- 新 `_bindInstanceMemberCall`(~58) / `_bindInstanceMember`(~48) / `_bindClassMemberAccess` 均 <60。
- 纯代码搬移；不动点 gen_a==gen_b **byte-identical 7/7** + golden **138/138**
  （basic/operators/control_flow/refs/strings/types/classes/inheritance/interfaces/generics/delegates）。

## 备注
- 子系统：compiler。
- **遗留（另开 change）**：TypeChecker 内仍有 4 个非分派 >60 行内聚函数——
  `_bindClass`(66) / `_synthCtors`(65) / `_bindLambda`(65) / `_resolveParamsOverload`(61)。
  这些无「集中 if-is 接收者分派」结构，拆分需按逻辑子段抽取，判断性更强，归后续 review。
  文件本身仍远超 500 行文件硬限 → God-Class 拆分（EmitContext 式，另开 change）。
