# Proposal: validate-func-type-constraint

> 状态：🟢 已实施（User 裁决：E0422+E0423，变性照 spec） | 创建：2026-09-10 | 类型：lang（新发诊断）

## 一句话

让**函数类型约束**（`where T : Func<int,int>` / `Action<int>` / `Predicate<U>` /
`(int) -> R` / 用户 `delegate`）真正参与校验——发出早已留号、却从未有代码路径发出过的
**E0422**（实参签名不符）与 **E0423**（func 约束与其它约束并置）。

## 为什么（这是「认得但不校验」的最后一格）

`restore-emit-zbc-diagnostics` 那条线的共同形状是「binder 不认 / 元数据抹掉，emitter 却照常发码」。
函数类型约束是同一族里剩下的一格，而且**方向相反地危险**：

```z42
void Run<T>(T handler, int x) where T: Action<int> { handler(x); }   // 体内按约束签名推断
...
Func<string, int> g = s => 1;
Run(g, 3);            // ← 今天：编译干净。约束等于没写。
```

- `_fillBundle` 认出 `Z42FuncType` 后**什么都不存**（2026-09-10 `fix-func-constraint-reported-unknown`
  只把它从「误报 E0443」摘出来，校验仍延后）。
- 但 binder **相信这条约束**：`MemberResolver` 的 `Z42GenericParamType` 分支按约束签名把
  `handler(x)` 绑成 `CallIndirect` 并推断结果类型。
- ⇒ 约束是「只被相信、从不被检查」的元数据：签名不符时按错签名发间接调用。

`docs/book/src/language/generic-constraints.md` 的七项表里，这是**唯一**一行还是 ❌。

## 怎么做

### 1. 模型（`GenericConstraint.z42`）

`ConstraintBundle` 加两个字段（与既有七项正交）：

```z42
public bool     HasFuncType;   // where T : Func<int,R> / Action<int> / (int)->R / delegate
public Z42Type  FuncType;      // 已解析的 Z42FuncType，**可含型参**（如 Func<int,R> 的 R）
```

### 2. 声明期（`_fillBundle` 的 `Z42FuncType` 分支）

- 存进 bundle（今天是「认出来 → 什么也不做」）。
- 同一型参上 func 约束与**任何**其它约束并置（`where T : Func<int,int> + IDisposable`）→ **E0423**。
  依据是 2026-05-11 `add-generic-func-constraint` 的 spec：v1 仅允许单一 func 签名存在。

### 3. 调用点（`_checkBundle` 新分支）

先把约束里的型参用**本次调用已解析的类型实参**代换（复用 `MethodTypeArgSubst.ByName`，
它已有 `Z42FuncType` 分支），再与实参类型结构比对：

| 情况 | 判定 |
|------|------|
| 实参是 GenericParam / Error / Unknown | 放行（与既有七项「信息不足不误报」同口径） |
| 实参不是 `Z42FuncType` | **E0422**（`where T: Action<int>` 却传了 `int`） |
| arity 不同 | **E0422** |
| 代换后**仍是型参**的位（类级型参等） | 放行（通配） |
| 其余位 | 形参位**逆变**、返回位**协变**；类类型走 `IsSubclassOf`，其余按 `CanonName()` 相等 |

变性方向照 2026-05-11 spec：约束 `Func<Cat,int>` 接受实参 `Func<Animal,int>`（形参逆变）；
约束 `Func<int,Animal>` 接受 `Func<int,Cat>`（返回协变）；反向 reject。

### 4. 文档 / Deferred

- 关闭 Deferred `where-constraint-future-func-constraint`。
- `docs/book/src/language/generic-constraints.md`：七项表最后一行 ❌ → ✅；「### 4. 函数类型约束
  从未发出诊断」整节改写为「已校验 + 残留边界」。

## 实测基线（`evidence/baseline.md`，用**本树自建**的 main 编译器，非旧 nightly）

- 四种违反今天 `--emit-zbc` **REAL_EXIT=0、零诊断**（开门之后测的，不是被吞）。
- 有**运行期后果**：`Run(g, 3)`（`g: Func<string,int>`）编译干净，运行
  `uncaught exception: VCall: expected object, got I64(3)` —— `int 3` 被喂进 `string` 形参；
  换成字符串拼接则**不崩、静默打印 `got: [3]`**。
- 可达性：`Run(inc, 7)` / `All(isEven, xs)` 这类**推断成功**的调用点确实走到 `CheckMethod`；
  只有 `Apply<T,R> where T: Func<int,R>` 因 `R` 推不出而整体失败、不可达。

> ⚠️ 我第一轮用 `.z42/` 的旧 nightly z42c 得出过「函数类型实参推断失败 ⇒ 校验路径根本不可达」，
> **是假结论**（旧种子早于推断阶段 C）。已用当前 main 自建编译器推翻。

## 🔴 顺带订正一条过期断言（本 change 一并做）

book 的「### 3. 顶层函数的 `where` 不校验」+ Deferred `where-constraint-future-toplevel-func`
**与事实不符**：实测顶层泛型函数的声明期与调用点校验**两半都跑**
（`TakesEnum<Plain>(p)` → E0402、`TakesNew<NeedsArg>(null)` → E0402）。
又一条「没有东西盯着的断言变成谎言」，随本轮订正。

## 明确不做（边界）

1. **跨包 class 级 func 约束不校验**——zbc 约束 bundle 的 flag 位里没有 func 签名槽（要双格式
   bump）。导入 bundle 恒无 func 约束 ⇒ 天然跳过，**不会假红**（与关联类型跨包同款处理）。
2. **推断失败的调用点仍完全不校验**——`R Apply<T,R>(T f, int x) where T: Func<int,R>` 的 `R`
   不出现在任何形参位 ⇒ `TypeArgInference` 边界 ① 整体失败 ⇒ `CheckMethod` 根本不跑。
   这条边界**本轮不动**（动它=改推断，另一件事）。
3. **声明级诊断仍按调用点重复**（`Run` 调 2 次 → E0423 报 2 条）。
   Deferred `constraint-decl-diag-per-callsite` 不在本轮；它还有个孪生症状：**从不被调用**的
   泛型方法，其 where 子句的声明级错误**一条都不报**。
4. lambda 直接作实参（`Run(v => {}, 7)`）——实参类型是延迟位（Unknown）⇒ 推断跳过 ⇒ 不校验。

## Gate（必须自带，因为全仓违反数 = 0）

⭐ 全仓只有 5 个文件用 func 约束（全在 `src/tests/generics/func_constraint_*.z42`），
且**全部合法** ⇒ 新诊断在现有代码上违反数恒为 0 ⇒ **不自带 fixture 就是又一道从不响的门**。

- **负例单测**：新建 `src/compiler/z42c.semantics/tests/typecheck/func_constraint/`，
  用 `bodyDiags` 口径断言**码 + 条数**（照 `generic_inference_tests.z42`）：
  非函数实参 / arity 不符 / 形参类型不符 / 返回类型不符 / 变性方向错 / E0423 并置。
- **每条负例做退回对照**（撤掉消费端 → 必须变绿）。
- **正例守卫**：5 个既有 `func_constraint_*.z42` 继续绿；通配位（`Predicate<U>`）、
  字面量形态（`(int)->int`）、0-arity（`() -> void`）各一条正例断言**零诊断**。
- **自举字节不动点 3/3**：`IsEmpty()` / `AnyNonEmpty()` 的语义变化会改变「类级 bundle 是否登记
  进 `ClassConstraints`」，而 `ClassDescBuilder` 按登记情况写 zbc TYPE 段 ⇒ 必须证明零漂移。
  不收敛则退化方案：只在**方法级** bundle 存 func 约束，类级维持现状。

## 风险

| 风险 | 缓解 |
|------|------|
| 假红（用户 delegate 别名 / params / 默认值等变体） | 判定保守：拿不准一律放行；全 GREEN + 全 stdlib 冷扫 |
| zbc 字节漂移（登记面变化） | 不动点 3/3 作硬门；不收敛就退化到方法级 |
| 变性判定与运行期 `validate_type_arg_constraint` 不一致 | 运行期**没有** func 约束这一项（七项里无）⇒ 无双判风险；本轮只在编译期立规则，并在 book 写明 |
