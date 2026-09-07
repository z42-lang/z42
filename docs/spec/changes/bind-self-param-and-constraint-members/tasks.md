# Tasks: 形参位的 `Self` + 型参收者的约束成员绑定

> 状态图例：🟢 完成 ｜ 🟡 进行中 ｜ ⚪ 未开始

## 阶段 0 —— 事实核查（🟢）

- 🟢 0.1 实测洞 1：`IEq a = new P(1); IEq b = new Q("hello"); a.Same(b)` → 运行期 `other.V = null`，静默错值
- 🟢 0.2 实测洞 2：`bool badGen<T>(T a) where T : IEq { a.Same("nope"); }` → 运行期 `string has no field 'V'`
- 🟢 0.3 量洞 1 破坏面：全仓 `IEquatable`/`IComparable`/`INumber` **零处**作变量·形参静态类型；
  唯一命中 = `src/tests/cross-zpkg/self_type_cross_pkg/ext/src/Ext.z42:7` 的 `eqVia(IEq, IEq)`
- 🟢 0.4 🔴 **harness 校准**：`xtask build test` / `--emit-zbc` **不报编译错误**（`f("wrong")` 对
  `void f(int)` 照样 `OK`）⇒ 编译期断言一律走 `SemanticDump` 单测。见 proposal「实施期查明 ①」

## 阶段 1 —— Part A：型参收者按 where 约束解析成员（🟢）

- 🟢 1.1 `ExprTyper._constraintOperatorMethod` → `internal _constraintIfaceMethod`（`_ifaceOperator`
  → `_ifaceMethodClosure`）。**零逻辑改动**——它本来就只按方法名查，对「运算符」无特殊处理
- 🟢 1.2 `MemberResolver` 的 `Z42GenericParamType` 分支：Object 查不到 → 查约束接口，命中则拿真签名
  走 `BindArgsToSignature`。**Object 优先级不变**（反了会改派发键 → 撼动自举字节）
- 🟢 1.3 `MemberResolver._substSelfSig`：整条签名（形参 + 返回）做 `Self → T` 替换；
  `ParamsFrom` / `ParamDefaults` / `ParamCallers` 原样搬运（ctor 只设 `ParamsFrom = -1`）
- 🟢 1.4 量欠债：`build stdlib` 25/25、`build compiler` 全过 ⇒ **实测欠债 = 0**
- 🟢 1.5 8 条单测（`tests/typecheck/constraint_member/`）+ **两轮退回对照**：
  5 条真门退回全红、3 条无误报守卫两态同绿
  - ⚠️ 第一版 `test_self_return_..._substitutes_to_type_param` 断言 `body.Contains(":T")`
    **两态同绿 = 假测试**（形参标注也贡献 `:T`）；改钉在 call 节点 `Copy :T` 上才成真门
- 🟢 1.6 完整 GREEN

## 阶段 2 —— Part B：接口静态类型收者禁止 `Self` 形参（🟡）

- 🟢 2.1 `DiagnosticCodes.SelfParamThroughInterface = "E0454"`（语义层发**字面量**，同 E0449–E0453 手法）
- 🟢 2.2 `MemberResolver` 接口收者分支：`_sigHasSelfParam` → E0454 + 诊断消息给出替代写法
- 🟢 2.3 改写 cross-zpkg fixture 的 `eqVia` → 型参形态
  - 🔴 **实施中挖到既存缺陷**：**跨包**泛型自由函数今天调不了（`E0402: cannot assign Point to T`），
    与 `Self` / 约束无关——`int idOf<T>(T a)` 一样炸，显式类型实参也不救。⇒ `eqVia<T>` **放在 main
    同包**，跨包接口静态类型的覆盖由 `getVia` 保住（它同时升格为 Part B 的无误报守卫）。
    已登记 Deferred `imported-generic-func-type-param-fidelity`
- 🟢 2.4 8 条 Part B 单测 + 退回对照（3 真门全红 / 5 守卫两态同绿）
  - `_containsSelf` **比 `_substSelf` 多下钻一层 `Z42FuncType`**（`void Apply(Func<Self,int> f)`）：
    两者刻意不对称——本函数守的是一条**禁令**，漏一种形态就是漏一个洞；`_substSelf` 少覆盖一种形态
    只是少换一次。「禁止侧比替换侧更宽」是安全方向。单独做过退回对照（去掉 func 那一支即变红）
- 🟢 2.5 🔒 **真实构建面破坏性对照**（#528 那道最值钱的）：把 fixture 退回 `eqVia(IEq, IEq)` 形态，
  `test e2e --dir cross-zpkg` 立即 `FAIL self_type_cross_pkg (ext build)` ⇒ E0454 活在真实构建路径上，
  不只在单测 harness 里
- 🟡 2.6 完整 GREEN + `test stdlib --mode jit` + `test e2e --dir cross-zpkg --mode jit` + `test bootstrap`

## 阶段 3 —— 文档与归档（🟡）

- 🟢 3.1 `docs/book/src/language/generic-constraints.md`：`Self` 实现模型节改写形参位规则（禁止 + 替代
  写法）；新增「型参收者上的约束成员绑定」整节 + 「已知限制：形参本身是型参时仍不检查」
- 🟢 3.2 `docs/roadmap.md`：关掉两条（`Self` 形参位 / 型参收者约束成员）；**新登记两条**
  `tighten-bare-type-param-target-erasure` + `imported-generic-func-type-param-fidelity`
- ⚪ 3.3 归档 `changes/` → `archive/2026-09-07-bind-self-param-and-constraint-members/`

## 不在本轮（已登记 Deferred）

- `tighten-bare-type-param-target-erasure`：`Conversion` 分支 B 对「目标是裸型参、来源是具体类型」
  仍擦除放行（C# 的 CS1503 会报）。⇒ **Part A 的实参检查只覆盖具体形参类型**，型参 / `Self` 形参那半
  仍不检查。改它是动通用擦除规则，爆炸半径未量
- `imported-generic-func-type-param-fidelity`：跨包泛型自由函数（见 2.3）
- `where-constraint-future-type-arg-matching` / `assoc-type-crosspkg` / static-vs-instance 种类校验
