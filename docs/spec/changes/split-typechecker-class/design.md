# Design: 拆 TypeChecker God-Class

## Architecture

镜像 codegen 侧已验证的 **EmitContext 模式**（`EmitContext` + `FunctionEmitter`/`ExprEmitter`，
`src/compiler/z42c.semantics/EmitContext.z42` 头注释即写明「z42 无 partial class → 抽公共状态到
EmitContext 拆文件」）。TypeChecker 的互递归比 codegen 更密（5-way），故用 **mediator context**
变体：ctx 既持共享状态，也持各子绑定器引用，任一子绑定器经 `_ctx.<Peer>` 触达同侪。

```
                    ┌─────────────────────────────────────────┐
                    │  TypeChecker (Facade)                    │
                    │  · Infer(cu, symbols) —— 唯一公开入口      │
                    │  · 顶层声明编排：_bindClass/_bindFreeFunc/ │
                    │    _bindImpl/_synthCtors/_bindMethodBody/  │
                    │    _injectFieldInits/_checkDuplicate*      │
                    │  · 构造 + wire ctx 与 4 子绑定器            │
                    └───────────────┬──────────────────────────┘
                                    │ new + two-phase wire
                    ┌───────────────▼──────────────────────────┐
                    │  TypeCheckContext (mediator)              │
                    │  共享状态(public 字段)：                    │
                    │   Diags / LoopDepth / LambdaActive /       │
                    │   LambdaLocals / LambdaCaps / LambdaCapCount│
                    │   Constraints / CurrentNs                  │
                    │  叶子助手(方法)：_isAssignable/_isNumericPrim│
                    │   /_primWrapper/_isPrimKeyword/_commonType/ │
                    │   _floatLitType/_requireBool/_checkOperand/ │
                    │   _operatorMethodNameTc/_capFirst/_hasWordTc│
                    │  子绑定器引用：Expr / Stmt / Member / Overload│
                    └──┬─────────┬──────────┬──────────┬─────────┘
              ┌────────▼──┐ ┌────▼─────┐ ┌──▼────────┐ ┌▼──────────────┐
              │ ExprTyper │ │StmtBinder│ │MemberResolver│ │OverloadResolver│
              │ _bindExpr │ │_bindStmt │ │_bindMember* │ │_resolveOverload│
              │ +节点binder│ │+语句binder│ │_bindMemberCall│ │_resolveParams* │
              │           │ │_varType  │ │_bindCall     │ │_collectOverloads│
              │           │ │          │ │_findField/Method│ │_with*/_adaptArgs│
              └───────────┘ └──────────┘ └─────────────┘ └───────────────┘
                每个子绑定器持 `_ctx`；跨界调用 = `_ctx.Stmt._bindStmt(...)` 等
```

## Decisions

### Decision 1：mediator context vs 成对 on-demand（codegen 用后者）

**问题**：codegen 侧 `FunctionEmitter` 持久持 `_expr`，`ExprEmitter` 按需 `new FunctionEmitter`
——2-way 够用。TypeChecker 是 5-way 密互递归（Expr↔Stmt↔Member↔Overload↔ctx 助手）。

**选项**：
- A. 每个子绑定器持所有同侪引用字段（N² 布线，构造顺序死锁）。
- B. **mediator**：ctx 持所有子绑定器引用，子绑定器只持 `_ctx`，经 `_ctx.<Peer>` 触达。两段式
  init（先 `new` 各子绑定器传 ctx，再回填 `ctx.Expr=…` 等；子绑定器仅 bind 期用引用、构造期不用，
  故安全）。
- C. 全部按需 `new`（每次 bind 建子绑定器——浪费 + 丢共享状态一致性）。

**决定**：选 **B**。5-way 下 mediator 是唯一无死锁且调用面统一（`_ctx.X._bindY`）的形态；两段式
init 是 z42 public 字段后赋值的既有能力（EmitContext 亦用可变 public 字段）。

### Decision 2：叶子转换/基元助手落点

**问题**：`_isAssignable`/`_isNumericPrim`/`_primWrapper` 等被 Expr/Member/Overload 三处调用。

**决定**：**方法挂到 `TypeCheckContext`**（镜像 EmitContext 持 `Alloc`/`Emit`/`TrackLine` 等共享
助手）。纯谓词（`_isNumericPrim(Z42Type)`/`_primWrapper(string)` 无状态）也放 ctx 保调用面统一
（`_ctx._isXxx(...)`），避免再引一个 `static class`。若 ctx 因此 >500 行，则把纯谓词子集下沉到
`static class TypeFactsTc`（实施期按行数裁决；Scope 已含该文件占位）。

### Decision 3：共享状态全部上移 ctx，子绑定器无自有状态

**问题**：lambda 捕获四字段（`_lambdaCaps`/`_lambdaCapCount`/`_lambdaActive`/`_lambdaLocals`，~70
处引用）在 Expr（`_bindIdent` 记捕获）、Stmt（`_bindLocalFunction`/`_bindTryCatch` 存取）、Facade
（`_bindMethodBody` 存档/恢复 lambda 栈）间共享。

**决定**：**全部 8 个字段上移 ctx（public）**，所有存取改 `_ctx.LambdaCaps` 等。子绑定器仅持
`_ctx`，无自有可变状态 → 状态单源、不会因分散而漂移。`_bindMethodBody` 的 save/restore lambda
栈逻辑随之读写 `_ctx.*`（语义不变）。

### Decision 4：字节不变量 = 唯一验收线

**问题**：搬移 + 调用改写有引入语义偏差的风险。

**决定**：每步用 **`build compiler` 自举不动点 gen_a==gen_b byte-identical 7/7** 兜底
（编译 z42c 全源码会走遍所有 binder 路径），外加聚焦 golden（basic/operators/control_flow/refs/
strings/types/classes/inheritance/interfaces/generics/delegates）。不动点字节相等 = 语义零漂移。

## Implementation Notes

**互递归调用改写映射**（实施机械替换）：
| 原 | 改写后 |
|----|--------|
| `this._bindExpr(...)`（在非-Expr 类内） | `this._ctx.Expr._bindExpr(...)` |
| `this._bindStmt(...)`（在非-Stmt 类内） | `this._ctx.Stmt._bindStmt(...)` |
| `this._bindMemberCall(...)` / `this._bindMember(...)` | `this._ctx.Member._bindMemberCall(...)` |
| `this._resolveOverload(...)` / `_withDefaults(...)` 等 | `this._ctx.Overload._resolveOverload(...)` |
| `this._isAssignable(...)` / `_requireBool(...)` 等叶子 | `this._ctx._isAssignable(...)` |
| `this._diags` / `this._loopDepth` / `this._lambda*` / `this._currentNs` / `this._constraints` | `this._ctx.Diags` / `_ctx.LoopDepth` / `_ctx.Lambda*` / `_ctx.CurrentNs` / `_ctx.Constraints` |
| 类**自己**的方法（同子绑定器内） | 不变（`this._bindX`） |

**wiring（Facade 构造）**：
```
// TypeChecker.Infer 头部（或构造器）
var ctx = new TypeCheckContext(this._diags);   // Facade 仍持 _diags 用于顶层诊断
ctx.Expr     = new ExprTyper(ctx);
ctx.Stmt     = new StmtBinder(ctx);
ctx.Member   = new MemberResolver(ctx);
ctx.Overload = new OverloadResolver(ctx);
this._ctx = ctx;
// 顶层 _bindClass/_bindMethodBody 经 this._ctx.Stmt._bindStmt / this._ctx.Expr._bindExpr 下钻
```

**边界归属**（哪个方法进哪个类，按上面 architecture 图）：Facade 保留 Infer + 8 个顶层声明/去重/
方法体编排方法；其余 ~67 方法按职责入 4 子类 + ctx 叶子助手。

## Testing Strategy

- **每步不动点**：`build compiler`（flat31 清洁 stdlib）gen_a==gen_b **byte-identical 7/7**。
- **聚焦 golden**：genB 编 11 类目 golden（现 138/138 基线）。
- **行数验收**：拆后 6 个新/改文件均 <500 行；无函数 >60 行；ctx 类 <200 行（超则按 Decision 2 下沉）。
- 编译器隔离（不重建他人在途 stdlib WIP）。

## 迁移顺序（每步独立 GREEN + 单独 commit，叶子优先降风险）

0. **建 ctx + 上移共享状态**：TypeChecker 仍持全部 binder，但状态读写改走 `_ctx.*`（内部零行为变化）。不动点。
1. **抽 OverloadResolver**（最叶子：被 Member/Call 调，仅回调 ctx 助手 + `_ctx.Expr` 于 `_adaptArgs`）。不动点。
2. **抽 MemberResolver**（调 `_ctx.Expr` 绑 recv/args + `_ctx.Overload`）。不动点。
3. **抽 StmtBinder**（调 `_ctx.Expr`）。不动点。
4. **抽 ExprTyper**（调 `_ctx.Stmt` 绑 lambda/局部函数体、`_ctx.Member` 绑成员调用）。不动点。
   → TypeChecker 收敛为 Facade，6 文件全 <500 行。

## Deferred / Future Work

- **AST/Bound 分派 kind-tag**（review §二讨论点）：本 change **不引入**；拆分后各子绑定器的
  `if-is` 分派保持原样。是否 kind-tag 化留独立设计决策（roadmap Deferred 索引）。
