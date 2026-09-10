# Tasks: validate-func-type-constraint

> 状态：🟡 实施中 | 创建：2026-09-10 | 类型：lang（新发诊断）

**变更说明：** 让函数类型约束真正参与校验——发出留号已久、却从未有代码路径发出过的
**E0422**（调用点签名不符）与 **E0423**（func 约束与其它约束并置）。
提案与实测基线见 [`proposal.md`](proposal.md) / [`evidence/baseline.md`](evidence/baseline.md)。

## 落地

- [x] `GenericConstraint.z42`：`ConstraintBundle` 加 `HasFuncType` / `FuncType`（存已解析的
      `Z42FuncType`，**可含型参**），并入 `IsEmpty()`
- [x] `ConstraintChecker._fillBundle`：func 分支从「认出来什么也不做」改成**存进 bundle**；
      并置检测 → E0423
- [x] `_fillBundle` 补 **非 `NamedType`** 分支：字面量 `(int) -> R`（`FuncTypeExpr`）此前
      整条 if 链**落空**，是一条彻底的空约束
- [x] `ConstraintChecker._checkFuncConstraint`：代换 → arity → 逐位变性比对 → E0422
- [x] 单测门 `tests/typecheck/func_constraint/`（**19 条**：10 负例 + 9 正例，含用户 `delegate` 两条）
- [x] book `generic-constraints.md` / `generics.md` 同步
- [x] GREEN + 自举字节不动点

## 关键决策

1. **不需要 unify**（推翻了 `fix-func-constraint-reported-unknown` 当时的判断）：调用点已经有一份
   解析好的类型实参（`CheckMethod` 的 `args`），直接 `MethodTypeArgSubst.ByName` 代换即可；
   代换不掉的位（类级型参 / `Self`）当通配放行。而 `Apply<T,R> where T: Func<int,R>` 那条
   **根本走不到校验**（`R` 不在任何形参位 ⇒ 推断整体失败）。
2. **数值拓宽不放行**：间接调用按槽位传值，`Func<int,long>` 与 `Func<int,int>` 不是一回事。
3. **零字节漂移由构造保证**：`IsEmpty()` 纳入 `HasFuncType` 会改变「类级 bundle 是否登记进
   `ClassConstraints`」，但 `ClassDescBuilder._constraintDescs` 对只有 func 约束的 bundle
   产出的 `IrConstraintDesc` 与「压根没登记」**逐字段相同**（wire 上没有 func 槽）⇒ 写出的
   zbc 字节不变。不动点 3/3 是这条推理的硬门。

## 门的强度（三道退回对照，全部实测）

| 撤掉什么 | 预期 | 实测 |
|---------|------|------|
| `_checkBundle` 里 `_checkFuncConstraint` 的挂接 | 8 条 E0422 负例全红、9 条正例全绿 | ✅ 正好 8 红 |
| `MethodTypeArgSubst.ByName` 那一步代换 | **只有** `test_wildcard_position_mismatch` 红 | ✅ 正好 1 红 |
| E0423 判据 | **只有** 两条 E0423 负例红 | ✅ 正好 2 红 |

⭐ 必须自带 fixture：全仓只有 5 个文件用 func 约束且全部合法 ⇒ 新诊断在真实代码上违反数恒为 0。

## 顺带订正的两条过期断言

1. **book「顶层函数的 `where` 不校验」是错的**——实测顶层泛型函数声明期与调用点两半都跑
   （`TakesEnum<Plain>(p)` → E0402）。Deferred `where-constraint-future-toplevel-func` 关闭。
2. **`generics.md` 把 func 约束的 zbc 编码写成「flag bit 0x40 + VM `verify_constraints` 已实现」**
   ——wire 上没有这个位（`0x20` 是 `RequiresEnum`），VM 侧也只有七项。已按该页自己声明的 SoT
   （`generic-constraints.md`）订正为「纯编译期、不跨包」。

## 附带发现（未修，已记进 book「已知限制 5」）

- 方法级 `where` 的**声明级**诊断在调用点即席重建 bundle ⇒ ①按调用次数重复 ②**从不被调用的
  泛型方法一条都不报**（`where T : IFooo` 拼错名字等于没写约束，而没人告诉你）。
- 类级声明诊断在 `--emit-zbc` 这条路上报**两遍**（`where T : class + struct` 实测 2 条）——
  `Resolve` 在该管线里跑了两次。**既有行为，非本轮引入**（修前同样双报）。

两条都归 Deferred `constraint-decl-diag-per-callsite`。

## 验证

- [x] 四种违反修前 `--emit-zbc` REAL_EXIT=0 零诊断 → 修后各报 1 条 E0422，消息含具体位次
- [x] 变性四格：形参逆变 ✅ / 返回协变 ✅ / 形参协变 ❌E0422 / 返回逆变 ❌E0422
- [x] 5 个既有 `src/tests/generics/func_constraint_*.z42` 修后仍零诊断
- [x] `xtask test compiler` 全绿（新增 17 条全部执行到）
- [x] 用户 `delegate` 约束（独立解析路径 `SymbolTable.Delegates`）：同形 `Func<int,int>` 放行、
      `Notify` 报 E0422
- [x] `xtask test` **全绿**（13 stages / 3m25s）+ **自举不动点 3/3**（gen1==gen2 逐字节，
      证实「零字节漂移」那条推理）
