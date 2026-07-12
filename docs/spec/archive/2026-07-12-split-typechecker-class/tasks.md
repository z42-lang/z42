# Tasks: 拆 TypeChecker God-Class（refactor）

> 状态：🟢 已完成（step 1 里程碑：验证拆法 + 抽 OverloadBinder）| 完成：2026-07-12
> **本 change 收窄归档为「第一步」**：验证 EmitContext 式 mediator 拆法可行（不动点 7/7）+
> 抽出 OverloadBinder。**步骤 2–4（MemberResolver / StmtBinder / ExprTyper 收敛 Facade）+ 叶子助手
> 抽 TypeFactsTc 转后续 change**（P1-1 未完，TypeChecker 仍 1703 行 > 500 硬限）。因 User 将优先级
> 转向 P2-2 / P3-5（同占 semantics 锁），故在此干净里程碑释放锁。

## ⚠️ 执行期方案修订（design integrity：择更简路径）
- **不引入独立 `TypeCheckContext`**：改用 **TypeChecker 自身作 mediator**——子绑定器持 `_tc`
  （TypeChecker 反向引用），跨界经 `_tc._bindExpr`/`_tc._isAssignable`/`_tc._diags`；共享状态
  暂留 TypeChecker（被 `_tc` 触达的成员改 public）。比「独立 ctx + 上移全部状态」churn 小、
  风险低，且已**不动点 7/7 验证**可行。文件 500 行硬限的收口靠后续把叶子助手抽 `static
  class TypeFactsTc`（+ 必要时 decl 编排抽 `DeclBinder`）达成。
- **命名踩坑**：`OverloadResolver` 名已被既有静态算法工具占用（`OverloadResult`/`MangleKey`/
  `Resolve`，SymbolCollector 也用）→ 新的绑定层子类命名 **`OverloadBinder`**（`OverloadBinder.z42`）。

## 进度概览（每步独立不动点 + 单独 commit）
- [x] 0+1. `OverloadBinder`（backref `_tc` mediator）——验证状态穿线 ✅ 不动点 7/7 + golden 138/138
- [ ] 2. 抽 `MemberResolver`
- [ ] 3. 抽 `StmtBinder`
- [ ] 4. 抽 `ExprTyper` → TypeChecker 收敛为 Facade（+ 叶子助手 → `TypeFactsTc`）
- [ ] 5. 文档同步 + 行数验收 + 归档

## 步骤 0：TypeCheckContext + 共享状态上移
- [ ] 0.1 新建 `TypeCheckContext.z42`：8 个 public 共享状态字段（Diags/LoopDepth/LambdaActive/
      LambdaLocals/LambdaCaps/LambdaCapCount/Constraints/CurrentNs）+ lambda-caps 增长数组管理助手
- [ ] 0.2 叶子转换/基元助手方法搬入 ctx（`_isAssignable`/`_isNumericPrim`/`_primWrapper`/
      `_isPrimKeyword`/`_commonType`/`_floatLitType`/`_requireBool`/`_checkOperand`/
      `_operatorMethodNameTc`/`_capFirst`/`_hasWordTc`）
- [ ] 0.3 TypeChecker 持 `_ctx`，全部 `this._field`→`this._ctx.Field`、叶子 `this._isX`→`this._ctx._isX`
- [ ] 0.4 ctx 子绑定器引用字段（Expr/Stmt/Member/Overload）声明（暂 null，步骤 1-4 逐个回填）
- [ ] 0.5 不动点 7/7 + golden 138/138

## 步骤 1：OverloadResolver
- [ ] 1.1 新建 `OverloadResolver.z42`：`_resolveOverload`/`_resolveParamsOverload`/`_collectOverloads`/
      `_withDefaults`/`_withParamsExpansion`/`_adaptArgs`/`_overloadKey`/`_ctorKey`/`_findMethod`
- [ ] 1.2 wire `ctx.Overload = new OverloadResolver(ctx)`；调用点 `this._resolveOverload`→`_ctx.Overload._resolveOverload`
- [ ] 1.3 不动点 7/7 + golden 138/138 + commit

## 步骤 2：MemberResolver
- [ ] 2.1 新建 `MemberResolver.z42`：`_bindMember`/`_bindInstanceMember`/`_bindClassMemberAccess`/
      `_bindMemberCall`/`_bindInstanceMemberCall`/`_bindCall`/`_findField`
- [ ] 2.2 wire + 调用改写（`_ctx.Expr` 绑 recv/args、`_ctx.Overload`）
- [ ] 2.3 不动点 7/7 + golden 138/138 + commit

## 步骤 3：StmtBinder
- [ ] 3.1 新建 `StmtBinder.z42`：`_bindStmt` + 11 语句 binder + `_varType`
- [ ] 3.2 wire + 调用改写（`_ctx.Expr`）
- [ ] 3.3 不动点 7/7 + golden 138/138 + commit

## 步骤 4：ExprTyper（收官）
- [ ] 4.1 新建 `ExprTyper.z42`：`_bindExpr` + 全部逐节点 binder（`_bindLiteral`/`_bindIdent`/
      `_bindAssign`/`_bindBinary`/`_bindUnary`/`_bindIndex`/`_bindSwitchExpr`/`_bindTernary`/
      `_bindRefArg`/`_bindArray*`/`_bindCast*`/`_bindAs*`/`_bindTypeof*`/`_bindIs*`/
      `_bindNullCondMember`/`_bindLambdaArg`/`_bindInterpolatedStr`/`_bindLambda`/`_bindDefault`/`_bindNew`）
- [ ] 4.2 wire + 调用改写（`_ctx.Stmt` 绑 lambda/局部函数体、`_ctx.Member`）
- [ ] 4.3 TypeChecker 只剩 Infer + 顶层声明编排（Facade）
- [ ] 4.4 不动点 7/7 + golden 138/138 + commit

## 步骤 5：收尾
- [ ] 5.1 行数验收：6 文件全 <500、无函数 >60、ctx <200（超则按 design Decision 2 下沉纯谓词）
- [ ] 5.2 `z42c.semantics/README.md` 核心文件表 + 功能索引同步
- [ ] 5.3 `compiler_review.md` §一 God-Class + P1-1 状态更新
- [ ] 5.4 归档 + 释放 compiler 锁

## 备注
- **纯代码搬移 + 调用改写，零语义变化**；byte-identical 不动点是硬安全网。
- 遗留（本 change 不做）：Parser 拆分（P1-2）、IrGen/ExprEmitter/FunctionEmitter 拆分（P1-3）、
  AST kind-tag 分派（§二讨论点）。
