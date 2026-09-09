# Proposal: lint 遍历补齐局部函数体

> change: `fix-analyzer-walk-local-function` ｜ scope: `compiler` ｜ 无格式 bump
> 来源: `ast-walker-completeness-gate`（#545）调研时发现的 latent bug

## Why

`AnalyzerDriver._walkStmt`（编译期 lint 框架的语句遍历，`z42c.semantics`）**从不递归
`LocalFunctionStmt` 体**——它把局部函数误归入「无子语句」那一档（`// 其余语句…LocalFunction…无子语句`）。
但局部函数**带方法体**（`LocalFunctionStmt.Decl.Body`）。后果：任何用户 analyzer（`z42.toml
[analyzers]` 声明、编译期加载运行）观察的节点（`WhileStmt`/`ForStmt`/`TryCatchStmt`/`CatchClause` …）
一旦出现在**局部函数内部**，就**永远不被遍历、静默漏掉**——lint 对局部函数内部完全失明。

这是一道 SILENT 缺口（漏了不报错、只是少看见节点），与 `ast-walker-completeness-gate` 修的那类
「漏一个节点即静默」同族，属**另一条**不变量（「递归每个带子语句的 Stmt」），故拆独立 change。

**爆炸半径 = 0（本仓）**：本仓自身的任何 project 都未在 `z42.toml` 声明 `[analyzers]`，故加载的
analyzer 数组为空、GREEN 不受影响。但该 lint 框架是**已接线的公共特性**（`PackageCompile.z42` 在
每个包编译期调 `AnalyzerDriver.Run`），下游任意 analyzer 都会踩这个洞。

## What Changes

`_walkStmt` 加一个 `LocalFunctionStmt` 分支，递归其 `Decl.Body`：

```z42
if (s is LocalFunctionStmt) {
    LocalFunctionStmt lf = s as LocalFunctionStmt;
    if (lf.Decl != null && lf.Decl.HasBody) { AnalyzerDriver._walkStmt(lf.Decl.Body, a, kinds, sink); }
    return;
}
```

**刻意最小**：只**下钻体**，不把局部函数自身当 `MethodDecl` **分派**（`_dispatch(MethodDecl, lf.Decl)`）
——「局部函数算不算方法级 lint 的观察对象」是独立语义取舍（局部函数常是 camelCase 私有 helper，
让 MethodDecl-lint 对它报错可能反而是噪声），不在本修范围。

同步把 `_walkStmt` 尾注里错误的 `LocalFunction` 从「无子语句」清单移除。

## Scope

- `src/compiler/z42c.semantics/src/AnalyzerDriver.z42`：`_walkStmt` 加 `LocalFunctionStmt` 分支 + 修尾注。
- `src/compiler/z42c.semantics/tests/analyzer/analyzer_tests.z42`：加一条回归门。

## Out of Scope

- 不把局部函数当 MethodDecl 分派（见上，独立取舍）。
- 不改任何文档契约（框架文档从未声明局部函数被跳过 → 本修只是让实现符合既有意图）。

## 验证

- 新门 `test_empty_catch_inside_local_function_reported`（`NoEmptyCatchAnalyzer` 观察 `CatchClause`，
  CU 里空 catch 藏在局部函数体内）：修后 **PASS**（analyzer 报 Z9002）。
- **退回对照**：只 stash 掉 `AnalyzerDriver` 的修（保留新测试）重建 → 新测试 **FAIL**（`values not
  equal`，exit 1）⇒ 证明门真的钉在这个修上、修前确实静默漏。
- `xtask test compiler`：z42c.semantics / analyzer 单元 36→37 passed, 0 failed。
- 完整 GREEN。纯 compiler 逻辑 + 测试，**无格式 bump**。
