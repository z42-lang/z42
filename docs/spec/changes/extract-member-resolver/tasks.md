# Tasks: 拆 TypeChecker God-Class 续（steps 2-4，refactor）

> 状态：🟢 已完成 | 完成：2026-07-12 | 子系统：compiler(semantics)
> 接 `split-typechecker-class` step1（OverloadBinder 已归档 2026-07-12）。EmitContext 式
> mediator（TypeChecker 自身作 mediator + 子绑定器持 `_tc` 反向引用）已验证不动点 7/7。

## 进度概览（每步独立不动点 + 单独 commit）
- [x] 2. 抽 `MemberResolver`（成员解析 + 调用 7 方法）—— ✅ 不动点 7/7 + golden 138/138
- [x] 3. 抽 `StmtBinder`（语句 13 方法）—— ✅ 不动点 7/7 + golden 138/138（TypeChecker 1419→1176）
- [x] 4. 抽 `ExprTyper`（表达式 22 方法）→ TypeChecker 收敛为 Facade —— ✅ 不动点 7/7 + golden 138/138（TypeChecker 1176→706；4 子绑定器全抽出）
- [x] 5. 叶子助手 → TypeFactsTc（9 纯谓词）+ 声明编排 → DeclBinder + _bindLambda→StmtBinder；**7 文件全 <500** ✅ 不动点 7/7 + golden 138/138

## step 2 结果
- 新 `MemberResolver.z42`（306 行）：`_bindMember`/`_bindInstanceMember`/`_bindClassMemberAccess`/
  `_bindMemberCall`/`_bindInstanceMemberCall`/`_bindCall`/`_findField`，持 `_tc` 反向引用。
- 跨界经 `_tc._bindExpr`/`_tc._overload`/`_tc._diags`/`_tc._currentNs`；prim 谓词走 TypeChecker 静态。
- TypeChecker 1703→1419；`_overload`/`_currentNs`/`_primWrapper`/`_isPrimKeyword` 改 public 供 `_tc` 触达。

## 备注
- 纯搬移 + 调用改写，零语义变化；byte-identical 不动点是硬安全网。
- 命名先查 collision（step1 踩过 OverloadResolver 覆写坑）。

## 收官结果
- TypeChecker **1937→257**（协调器 Facade：Infer + wiring + _requireBool/_checkOperand）。
- 7 文件全 <500：TypeChecker 257 / OverloadBinder 259 / MemberResolver 306 / StmtBinder 349 / ExprTyper 429 / TypeFactsTc 123 / DeclBinder 371。
- **P1-1（拆 TypeChecker God-Class + 文件 500 行硬限）完成**。EmitContext 式 mediator（TypeChecker 作 mediator + 子绑定器持 _tc）全程不动点 byte-identical 兜底。
