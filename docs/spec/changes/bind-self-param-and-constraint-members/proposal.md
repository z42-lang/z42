# Proposal: 形参位的 `Self` + 型参收者的约束成员绑定

> 类型：lang（类型检查语义）｜ 创建：2026-09-07 ｜ 状态：**DRAFT（待 User 裁决 Q1/Q2）**
> 前置：`add-associated-types` PR-2 落 `Self`（#506）、`apply-self-to-core-protocols` 落 use（#525）、
> `self-return-type-substitution` 落返回位（#527）、`add-argument-type-check` 接线实参检查（#523）

## Why

`self-return-type-substitution`（#527）只补了 `Self` 的**返回位**，其 proposal 明确把**形参位**留作
独立议题。本 change 是那个口子——但**开工前的实测把它扩成了两个洞**，第二个比第一个大。

### 洞 1：接口静态类型收者，`Self` 形参零检查 → 运行期静默错值

```z42
interface IEq { bool Same(Self other); }
class P : IEq { public int V; public bool Same(P other) { return this.V == other.V; } }
class Q : IEq { public string S; public bool Same(Q other) { return this.S == other.S; } }

IEq a = new P(1);
IEq b = new Q("hello");
a.Same(b);        // 编译期零诊断
```

**实测（本 worktree，e2e harness 真跑）**：编译零诊断，运行期派发到 `P.Same(P)`，
`other.V` 从一个 `Q` 对象上读出 —— 结果是 **`null`**，**没有任何报错**，静默返回 `false`。

```
actual: P.Same got other.V = null
```

机制不是 proposal #527 记的「`ConstraintChecker` 把裸型参当假定满足」，而是
[`Conversion.z42:126`](../../../../src/compiler/z42c.semantics/src/Conversion.z42) 的**分支 B**：
形参类型是裸 `Z42GenericParamType("Self")`，实参是具体类型 ⇒ 恰一侧含型参 ⇒ `GenericErase` 放行。
（#527 引的 `ConstraintChecker:256/267/309` 是 where 约束满足性判定，是另一条路。本 proposal 顺带更正。）

### 洞 2（更大）：型参收者上的约束接口方法**根本没有签名**

```z42
bool badGen<T>(T a) where T : IEq { return a.Same("nope"); }   // 编译期零诊断
```

**实测**：编译零诊断；运行期 `Error: string has no field 'V'`。

根因在 [`MemberResolver.z42:142-153`](../../../../src/compiler/z42c.semantics/src/MemberResolver.z42)：
`Z42GenericParamType` 收者分支**只查 `Object` 类**（`GetHashCode` / `ToString` 那批）。查不到就
`BindArgsToSignature(..., sig = null, ...)` 松绑 —— 而 `OverloadBinder.CheckArgTypes` 的第一行就是
`if (sig == null) { return; }`。⇒ **泛型代码里对约束接口方法的调用，实参一律不检查、返回类型一律
`Unknown`**。这不限于 `Self`：`where T : IComparable` 的 `a.CompareTo(x)`、`where T : INumber` 的
`a.op_Add(x)` 全都如此，而 `PriorityQueue` / `SortedSet` / `Dictionary` 正走这条路。

洞 2 是**洞 1 的解药所在**：Rust 对形参位 `Self` 的答案是「别用 `dyn Trait`，用泛型参数」。
z42 今天的「泛型参数」这条路**同样零检查**，所以只堵洞 1 而不补洞 2，等于把用户从一个无检查的写法
赶到另一个无检查的写法。

## What Changes

### Part A —— 型参收者按 where 约束解析成员（洞 2）

`MemberResolver` 的 `Z42GenericParamType` 分支，在 `Object` 查找失败后，追加一步「按该型参的 where
约束接口（含父接口闭包）找同名方法」，命中则拿真实 `MethodSymbol.Signature` 走
`BindArgsToSignature` ⇒ 实参进入 `CheckArgTypes` 的正常门。

**查找机器已经现成**：`ExprTyper._constraintOperatorMethod` / `_ifaceOperator`
（[`ExprTyper.z42:464-521`](../../../../src/compiler/z42c.semantics/src/ExprTyper.z42)）——
方法级 `WhereClause` 现读 + 类级 `ConstraintSet`（走 `ConstraintKey`）+ 父接口闭包递归，
参数已经是任意方法名（`opMethod` 只是个 string）。**本 Part 主要是把它从 `_bindBinary` 私有提升为
可复用，接到 `MemberResolver`**，不是新写查找算法。

**签名里的 `Self` 在这条路上精确替换为该型参 `T`** —— 不是上界，是等号：约束 `T : IEq` 意味着
运行期 `T` 就是那个实现类型，`Self ≡ T`。这比洞 1 的接口上界强得多。

### Part B —— 接口静态类型收者的形参位 `Self`（洞 1）

见下方 **Q1**，两个候选语义要 User 裁决。

## 🔴 待 User 裁决

### Q1：洞 1 取哪种语义？

| | 做法 | 能否堵住实测的类型混淆 | 破坏面 |
|---|---|---|---|
| **1a 上界替换**（与 #527 返回位对称） | `Self` 形参 → 接收者的静态接口类型 | ❌ **不能**。`P` 和 `Q` 都是 `IEq`，`a.Same(b)` 照样过 | 0 |
| **1b Rust 式禁止**（推荐） | 经**接口静态类型**调用「形参位含 `Self`」的方法 → 新错误码，提示改用型参写法 | ✅ 彻底 | **实测 1 处**（见下） |

**我推荐 1b**，理由：

1. **1a 是假保障**。它把「全放行」收紧成「只放行实现了该接口的」，而 `Self` 方法的实参**本来就
   几乎总是**该接口的实现者 —— 我实测的那个混淆例子 1a 一个字都拦不住。做完之后文档要写
   「已收紧」，实际上这条线（[[audit-silent-gates-program]]）正在清的就是这种东西。
2. **1b 有可用的迁移路径，且 Part A 正好把它铺通**：`bool eqVia(IEq a, IEq b)` 改写成
   `bool eqVia<T>(T a, T b) where T : IEq` —— 改完不但健全，还**真的被检查**（Part A）。
   这正是 Rust 的 object-safety 分工。
3. **破坏面实测 = 1 处**：全仓 grep，`IEquatable` / `IComparable` / `INumber` **零处**被用作
   变量 / 形参的静态类型（只出现在 `where` 约束位、`: I` 实现位、`BuiltinTypeDefs` 表里）。
   唯一命中是测试 fixture
   [`src/tests/cross-zpkg/self_type_cross_pkg/ext/src/Ext.z42:7`](../../../../src/tests/cross-zpkg/self_type_cross_pkg/ext/src/Ext.z42)
   的 `bool eqVia(IEq a, IEq b) { return a.Same(b); }` —— 而它恰好就是**不健全写法的样本**。
   改写它同时保住该 fixture 的原意（跨包导入侧 `Self` 还原）：改成泛型版仍走 imported 接口签名。

> 若 User 选 1a 或「本轮不动洞 1」，Part A 单独成立、依然值得做。

### Q2：Part A 的返回类型要不要一起改？

今天型参收者查不到方法 ⇒ 返回 `Z42UnknownType`。Part A 拿到真签名后，返回类型自然应替换为
`Self→T` 后的真实类型。**但这会改变 Bound 树的类型标注，有漂 zbc 字节的风险**
（`generic_inumber.z42` 的 `a.op_Multiply(b).op_Multiply(c)` 链式今天靠 `Unknown` 松绑跑通）。

- **Q2-a（保守，推荐先做）**：本轮**只接实参检查**，返回类型仍留 `Unknown`。零字节漂移风险。
- **Q2-b**：连返回类型一起换真。收益是泛型代码里链式调用有真类型；风险是字节漂移 + 需重跑不动点。

建议 **Q2-a**：先把「零检查」这个真洞堵上并拿到 GREEN，返回类型另立一条（可在同 PR 追加，视
实测字节是否漂移决定）。

## 不在本轮

- **约束接口的类型实参匹配**（Deferred `where-constraint-future-type-arg-matching`）：Part A 沿约束
  接口链找方法时仍按裸名匹配，不动这条。
- **跨包关联类型**（Deferred `assoc-type-crosspkg`）：双格式 bump，与本轮正交。
- **static-vs-instance 种类校验**（#528 留的口子）：卡在 `MethodSymbol` 无 `IsAbstract` 槽。

## 验证（计划）

- 🔒 **真门 + 退回对照**（[[add-associated-types-program]] 的硬教训：新负例一律先做退回对照）：
  - Part A：`badGen` 那条负例在改动前**必须实测是绿的**（即它今天确实不报），改动后变红。
  - Part B（若选 1b）：`eqVia(IEq, IEq)` 形态改动前绿、改动后红。
- 🔒 **真实构建面破坏性对照**（#528 最值钱那道）：Part A 会给**大量既有泛型 stdlib 代码**首次接上
  实参检查 ⇒ **必须先量欠债**（`build stdlib` + 全仓），零欠债不能当默认假设。
- 对照组：具体类收者路径不经过任何新逻辑，应两态同绿。
- 完整 GREEN + `test stdlib --mode jit` + `test e2e --dir cross-zpkg --mode jit` + `test bootstrap`
  + 自举字节不动点。
