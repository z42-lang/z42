# Tasks: fix-func-constraint-reported-unknown

> 状态：🟢 已完成 | 创建：2026-09-10 | 完成：2026-09-10 | 类型：fix

**变更说明：** 修复**函数类型约束被当成「约束名拼错了」报 E0443**，合法代码编不过：

```z42
void Run<T>(T handler, int x) where T: Action<int> { handler(x); }
//                                     ^^^^^^ E0443: unknown constraint type `Action` on `Run`
```

## 根因

`ConstraintChecker._fillBundle` 依次问「是型参？→ 是接口？→ 是类？」，都否就报
「多半是拼错了」。而 `Action` / `Func` / `Predicate` / 用户 `delegate` 经
`SymbolTable.ResolveTypeP` 解析成**结构化 `Z42FuncType`**，**不进 `Classes` / `Interfaces` 表**
⇒ 三问全否 ⇒ 掉进 else。

这与本文件（`GenericConstraint.z42`）抬头自己写的设计相矛盾：那里明确把
「func-type 约束的**校验**」列为 Deferred（`where-constraint-future-func-constraint`，
E0422/E0423 已留号）—— 意思是**认得但不校验**，而不是「不认得、直接判错」。

**为什么当年的探针没抓到**：那条 error 分支落地时以 warning 跑全仓实测「0 条」，
但三个受害文件（`src/tests/generics/func_constraint_{action,predicate,captured}.z42`）
全走 `z42c --emit-zbc` —— 那条路径当时**丢弃全部诊断**。
⭐ **探针看不见的地方，"0 条" 不构成证据**。

## 修复

在报错前先认出函数类型：`symbols.ResolveTypeP(con.Type, pnames, pc) is Z42FuncType` → 不报错。
顺带把用户 `delegate` 类型的约束也一并认了（同样解析成 `Z42FuncType`）。

**校验仍延后**，Deferred 保持不变。为什么不顺手补上：`func_constraint_captured.z42` 里有
`R Apply<T, R>(T f, int x) where T : Func<int, R>` —— 约束类型里含**另一个型参 `R`**，
判定得把约束里的型参当通配去 unify，不是加一次 `IsAssignableTo` 能了事的，属独立一件事。

## 附带发现（未修，已登记）

`ConstraintChecker.CheckMethod` **每个调用点都重建一遍 bundle**，于是 `_fillBundle` 里这类
**声明级**诊断会按调用次数重复 —— `Run` 被调用 2 次就报 2 条一模一样的 E0443（实测）。
真正的修法是把声明级约束诊断挪到声明期 pass。Deferred：`constraint-decl-diag-per-callsite`。

## 验证

- [x] 三个 `func_constraint_*.z42` 修前各报 2–5 条 E0443，修后**编译干净且运行通过**
- [x] `z42c build` 路径同样复现与修复（不只 `--emit-zbc`）
- [x] `xtask test` 全绿
