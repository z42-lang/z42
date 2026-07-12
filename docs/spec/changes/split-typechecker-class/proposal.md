# Proposal: 拆 TypeChecker God-Class（compiler-review P1-1）

## Why

`z42c.semantics/src/TypeChecker.z42` **1937 行**，超 500 行文件硬限 **+1437**——是 review §一
的 11 个超限文件之首、总评 #1 问题。z42 **无 partial class**，故满足「文件 500 行硬限」的
**唯一**途径是把类本身拆开：一个类 = 一个文件，类 >500 行 → 必须抽子类。

前两个 change（`split-typechecker-fns` / `split-typechecker-member-fns`）已把类内 4 个最大的
分派/接收者巨函数拆成 29 个命名清晰的私有 binder（函数级 60 行硬限已基本达标），但**文件级
1937 行硬限依然违规**——只有类级拆分能根治。

现类聚合五种职责（review §一）：① 顶层编排/声明绑定 ② 语句绑定 ③ 表达式类型检查
④ 成员解析 + 调用 ⑤ 重载决议 + 转换规则。

## What Changes

按 codegen 侧已验证的 **EmitContext 模式**（`FunctionEmitter`+`ExprEmitter` 共用 `EmitContext`，
正因 z42 无 partial class 而生）拆分 TypeChecker：

- 新 `TypeCheckContext`：持全部**共享状态**（`_diags` / `_loopDepth` / lambda 捕获四字段 /
  `_constraints` / `_currentNs`）+ 指向各子绑定器的引用（mediator，两段式 init 解 5-way 互递归）。
- 抽子类（各自独立文件，均 <500 行）：
  - `ExprTyper`（表达式：`_bindExpr` + 各逐节点 binder）
  - `StmtBinder`（语句：`_bindStmt` + 各逐语句 binder + `_varType`）
  - `MemberResolver`（成员/调用：`_bindMember*` / `_bindMemberCall*` / `_bindCall` / `_findField`/`_findMethod`）
  - `OverloadResolver`（重载：`_resolveOverload` / `_resolveParamsOverload` / `_collectOverloads` / `_with*` / `_adaptArgs` / `_*Key`）
  - `TypeFacts`（转换/基元规则：`_isAssignable` / `_isNumericPrim` / `_primWrapper` / `_isPrimKeyword` / `_commonType` / `_floatLitType` / `_requireBool` / `_checkOperand` / `_operatorMethodNameTc` / `_capFirst` / `_hasWordTc`）
- `TypeChecker` 降为 **Facade**：只保留 `Infer` 入口 + 顶层声明编排（`_bindClass` / `_bindFreeFunc` /
  `_bindImpl` / `_synthCtors` / `_bindMethodBody` / `_injectFieldInits` / `_checkDuplicate*`），
  构造并 wire ctx + 子绑定器。下游（IrGen / pipeline）**只见 `Infer`，无感知**。
- **纯代码搬移 + 调用改写**（`this._bindX` → `_ctx.ExprTyper._bindX` 等），**零语义变化**；
  自举不动点 gen_a==gen_b byte-identical 是硬安全网。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/TypeCheckContext.z42` | NEW | 共享状态 + 子绑定器引用（mediator） |
| `src/compiler/z42c.semantics/src/ExprTyper.z42` | NEW | 表达式类型检查子类 |
| `src/compiler/z42c.semantics/src/StmtBinder.z42` | NEW | 语句绑定子类 |
| `src/compiler/z42c.semantics/src/MemberResolver.z42` | NEW | 成员解析 + 调用子类 |
| `src/compiler/z42c.semantics/src/OverloadResolver.z42` | NEW | 重载决议子类 |
| `src/compiler/z42c.semantics/src/TypeFacts.z42` | MODIFY/NEW | 转换/基元规则（若已存在 TypeFacts.z42 则并入，否则新建 TypeFactsTc.z42） |
| `src/compiler/z42c.semantics/src/TypeChecker.z42` | MODIFY | 降为 Facade（Infer + 顶层声明编排 + wire ctx） |
| `src/compiler/z42c.semantics/README.md` | MODIFY | 核心文件表 + 功能索引同步 |
| `docs/compiler_review.md` | MODIFY | §一 God-Class 项 + P1-1 状态 |
| `docs/spec/changes/ACTIVE.md` | MODIFY | 声明/释放 compiler(semantics) 锁 |

**只读引用**：`EmitContext.z42` / `ExprEmitter.z42` / `FunctionEmitter.z42`（借鉴 wiring 模式）；
`docs/agent/rules/doc-system.md` §5.1（是否需 book 机制页）。

## Out of Scope

- **不改任何绑定语义 / 诊断文案 / IR 输出**——byte-identical 是验收线。
- **不拆 Parser**（P1-2，`syntax` 子系统，另开 change）。
- **不动 AST/Bound 分派机制的 kind-tag 设计决策**（§二讨论点，独立评估）。
- 不做 §二/§三 表驱动化（P2 系列）。

## Open Questions

- [ ] `TypeFacts.z42` 是否已存在可并入？（Scope 表已留两种落点，实施首步确认）
- [ ] 执行需长时间持 `compiler`(semantics) 锁——与队列中 `converge-z42c-onto-z42-project`（现持
      stdlib 锁、排队 compiler）、`add-partial-types`（🔴 DRAFT）的**先后顺序需 User 裁决**。
