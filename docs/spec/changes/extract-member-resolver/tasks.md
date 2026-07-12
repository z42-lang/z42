# Tasks: 拆 TypeChecker God-Class 续（steps 2-4，refactor）

> 状态：🟡 进行中 | 创建：2026-07-12 | 子系统：compiler(semantics)
> 接 `split-typechecker-class` step1（OverloadBinder 已归档 2026-07-12）。EmitContext 式
> mediator（TypeChecker 自身作 mediator + 子绑定器持 `_tc` 反向引用）已验证不动点 7/7。

## 进度概览（每步独立不动点 + 单独 commit）
- [x] 2. 抽 `MemberResolver`（成员解析 + 调用 7 方法）—— ✅ 不动点 7/7 + golden 138/138
- [ ] 3. 抽 `StmtBinder`（语句 13 方法）
- [ ] 4. 抽 `ExprTyper`（表达式 ~20 方法）→ TypeChecker 收敛为 Facade
- [ ] 5. 叶子助手 → TypeFactsTc（收口文件 500 行硬限）+ 文档 + 归档

## step 2 结果
- 新 `MemberResolver.z42`（306 行）：`_bindMember`/`_bindInstanceMember`/`_bindClassMemberAccess`/
  `_bindMemberCall`/`_bindInstanceMemberCall`/`_bindCall`/`_findField`，持 `_tc` 反向引用。
- 跨界经 `_tc._bindExpr`/`_tc._overload`/`_tc._diags`/`_tc._currentNs`；prim 谓词走 TypeChecker 静态。
- TypeChecker 1703→1419；`_overload`/`_currentNs`/`_primWrapper`/`_isPrimKeyword` 改 public 供 `_tc` 触达。

## 备注
- 纯搬移 + 调用改写，零语义变化；byte-identical 不动点是硬安全网。
- 命名先查 collision（step1 踩过 OverloadResolver 覆写坑）。
