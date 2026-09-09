# Proposal: AST walker 完备性 gate

> change: `ast-walker-completeness-gate` ｜ scope: `toolchain` + `docs` ｜ 无格式 bump
> 来源: [[add-associated-types-program]] 第五批 (#536) 的 Deferred `ast-walker-completeness-gate`

## Why

z42c 里有若干**手写穷举** AST walker——一串 `if (x is SomeExpr) { ... }` 覆盖某个 AST 节点族的
全部子类。它们的**全部价值就是完备**：漏处理一个子类就是一个静默洞。

- `MethodTypeParamUse`（#536 阶段 D 建）判定「方法体是否运行期消费方法级型参」。漏一种消费形态
  → 省略 `<T>` 的调用运行期读到空 `frame.method_type_args` → **静默错值**。
- `ExprTyper._bindExpr` / `StmtBinder._bindStmt` / `PatternBinder.Bind` 是主编译路径的穷举分派。
  漏一类会撞各自的 **LOUD fallback**（`Error(...) + BoundError`），但那只在编译器**运行期**才炸，
  不是编译期保障——加了新节点类而忘了接一个分支，本地/CI 全绿，直到有人恰好构造出该节点。

**问题**：这些 walker 的覆盖以前靠**从源码机械枚举**建立，**没有任何门盯着**。往 `Ast.z42` /
`Stmt.z42` / `Pattern.z42` / `TypeExpr.z42` 加一个新子类，漏进 walker，**不会让任何测试变红**。

**证据（本 change 开工时实测坐实）**：`MethodTypeParamUse.z42` 抬头第 ③ 条白纸黑字写着
「节点全集有配套的完备性门（见 tests/typecheck/generic_inference/），AST 加新节点类会变红」——
**这句是假的**。`generic_inference_tests.z42` 只有 5 种消费形态 + 若干嵌套递归的**行为**用例，
**没有一条枚举节点类全集**。更甚：该文件自称「TypeExpr.z42 6 个子类」，实际是 **5 个**——
硬编码计数本身已在漂。这正是本项目反复撞到的「没有测试盯着的断言迟早变成谎言」。

## What Changes

新增一道 GREEN gate stage **`xtask test walkers`**（`scripts/test/xtask_test_walkers.z42`），
**活体对账**：

1. 从 `src/libraries/z42c.syntax/src/*.z42` 扫出节点类全集——`public sealed class NAME : BASE`
   其中 `BASE ∈ {Expr, Stmt, Pattern, TypeExpr}`。**不信任何硬编码计数**（那种计数就在漂）。
2. 一张**登记表** `_walkerRegistry()`：每个登记 walker 声明「覆盖哪几族」+「故意跳过的白名单」。
3. 逐 (walker, 节点类) 对账：节点类既不被该 walker `is`-匹配、又不在其白名单里 → **红**（exit 1）。

**硬门，非棘轮**：所有登记 walker 今天都 100% 覆盖，故无基线文件；出现 gap 直接红。

### 登记表 v1（4 个 walker，依据下方分类）

| walker | 覆盖节点族 | 白名单（故意跳过） |
|---|---|---|
| `MethodTypeParamUse.Consumes` | Expr / Stmt / Pattern / TypeExpr | 8 个 Expr 叶子（Int/Float/String/Char/Bool/Null/Ident/EmptyBrace，无子节点无类型位，fall-through） |
| `ExprTyper._bindExpr` | Expr | — （36/36） |
| `StmtBinder._bindStmt` | Stmt | — （16/16） |
| `PatternBinder.Bind` | Pattern | — （10/10） |

### walker 分类（哪些登记、哪些不登记）

调研了 7 个候选（`git show origin/main` 读源，逐个看 fallback 与设计意图）：

| 文件 | 分派对象 | 意图 | 漏节点时 | 登记？ |
|---|---|---|---|---|
| ExprTyper `_bindExpr` | Expr（全 36） | 穷举 | LOUD（Error+BoundError） | ✅ |
| StmtBinder `_bindStmt` | Stmt（全 16） | 穷举 | LOUD | ✅ |
| PatternBinder `Bind` | Pattern（全 10） | 穷举 | LOUD | ✅ |
| MethodTypeParamUse `Consumes` | 四族 | 穷举（过近似） | SILENT（静默错值） | ✅ |
| MemberResolver | 按 `Z42Type` kind / 目标 shape | 部分 | LOUD（type-kind） | ❌ 非 AST 族 walker |
| AssignTyper | 按赋值目标 shape | 部分 | 落通用 `_bindExpr` | ❌ 非 AST 族 walker |
| ConstEval `Eval` | Expr 子集（10/36） | **按设计部分** | SILENT `return null` | ❌ 但 `null→ConstNotConstantInit` 调用方兜底，新节点天然安全 |

> **顺带发现的 latent bug（本 change 不修，登记 Deferred）**：`AnalyzerDriver._walkStmt`（lint）
> **从不递归 `LocalFunctionStmt` 体** → lint analyzer 永远看不到局部函数内部。这是「递归每个
> 带子语句的 Stmt」这条**另一个不变量**，与本门（AST 族完备性）正交，拆独立 change。

### 顺带修正的假注释

`MethodTypeParamUse.z42` 抬头第 ③ 条改为指向本门（并改 `TypeExpr 6`→`5` 的计数漂移）。

## Scope（允许改动的文件）

- 新增 `scripts/test/xtask_test_walkers.z42`（gate 实现 + 登记表）
- `scripts/test/xtask_test.z42`：`_gateStageNames()` 加 `"walkers"` + `_testAll` 加 stage 调用
- `scripts/cli/xtask_cli_test.z42`：`test walkers` 子命令 + dispatch
- `docs/book/src/dev/test-gate.md`：gate-stages 区加 `walkers` + 描述段（doc-drift 门强制锁步）
- `src/compiler/z42c.semantics/src/MethodTypeParamUse.z42`：仅**注释**（修假注释 + 计数）

## Out of Scope

- 不修 AnalyzerDriver 的 LocalFunctionStmt 洞（Deferred，另立 change）。
- 不给 MemberResolver / AssignTyper 建「类型-lattice / 目标-shape」完备性门（不同 family，本轮不做）。
- 不改任何编译器行为逻辑 → **无格式 bump、无自举字节漂移**（仅 xtask 工具 + docs + 注释）。

## 验证

- 干净树：`xtask test walkers` 绿（67 节点类 × 4 walker = 129 对，0 gap）。
- 阳性对照 A：删 ExprTyper 的 `is TupleExpr` → `✗ ExprTyper._bindExpr: TupleExpr (Expr)` + exit 1。
- 阳性对照 B（**核心不变量**）：往 Ast.z42 注入 `FakeProbeExpr : Expr` → ExprTyper + MethodTypeParamUse
  **两个 Expr-覆盖 walker 同时报**、Stmt/Pattern-only walker 正确忽略；exit 1。两次均 `git checkout` 复原。
- 完整 GREEN（含 `_checkGateStageDoc` 锁步门 + 新 walkers stage）。
