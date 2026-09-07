# Proposal: 形参位的 `Self` + 型参收者的约束成员绑定

> 类型：lang（类型检查语义）｜ 创建：2026-09-07 ｜ 状态：**IMPL（Q1/Q2 已裁决）**
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

## ✅ User 裁决（2026-09-07，勿重问）

**Q1 = 1b（Rust 式禁止）**｜**Q2 = Q2-b（返回类型一并换真）**。

## 🔴 实施期查明的两件事（都推翻了本 proposal 起草时的假设）

### ① 「golden 用例能当编译错误的门」是假的 —— 我最初的探针全部无效

起草阶段我用 `xtask test e2e` / `--emit-zbc` 跑探针，看到「编译零诊断」就下了结论。**校准实验推翻了它**：

```z42
void f(int x) { }
void Main() { f("definitely wrong"); }     // 放进 src/tests/generics/ → `OK: zzprobe_gp`，build test exit 0
```

`xtask build test` 走 `z42c --emit-zbc`（`scripts/test/xtask_test_targets.z42:157`），而 `--emit-zbc`
**至今丢弃全部编译诊断、exit 0 照写产物** —— 这正是 [[restore-emit-zbc-diagnostics-program]] 的未修项
`emit-zbc-no-error-gate`，不在本 change 范围。⇒ **本 change 的编译期断言一律走 `SemanticDump` 单测**
（`src/compiler/z42c.semantics/tests/`），golden 那条路只用来观察**运行期**行为。

> 教训：探针跑出「没报错」时，先证明这个 harness **有能力报错**。我在洞 1/洞 2 上各浪费了一轮。
> （运行期证据与源码阅读独立成立，故两个洞的结论不受影响，但过程是错的。）

### ② `Self → T` 精确替换**不能**恢复形参位的实参检查（诚实记账）

Part A 把约束接口方法签名里的 `Self` 换成型参 `T` 后，形参类型是**裸 `Z42GenericParamType`**，于是
`Conversion._classifyBuiltin` 的**分支 B**（`_hasGenericParam(from) != _hasGenericParam(to)` →
`GenericErase`）照旧放行：

```z42
bool bad<T>(T a) where T : IEq { return a.Same("nope"); }   // Part A 之后**仍然**零诊断
```

⇒ **Part A 的实参检查收益只覆盖「形参类型是具体类型」的约束接口方法**（`void Add(int)` 这类，
已由 3 条真门守住）；形参位是型参 / `Self` 的那半仍被擦除放行。**返回类型换真那半是完整生效的。**

要连这半也堵上，得收紧分支 B「目标是裸型参、来源是具体类型 → 不可隐式转」（C# 的 CS1503 就是这条
规则）。那是对**通用擦除规则**动刀，爆炸半径未量，**已登记为独立 Deferred
`tighten-bare-type-param-target-erasure`，不在本轮**。本 proposal 不把 Part A 写成「泛型代码的实参
检查已补齐」——它没有。

## 已裁决的原始选项（存档）

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

## 验证（已做）

- 🔒 **退回对照，两 Part 各做一轮**：Part A 撤掉后 5 条真门全红（3×E0402 + 2 条返回类型）、
  3 条无误报守卫两态同绿；Part B 撤掉后 2 条 E0454 真门全红、4 条守卫两态同绿。
  - ⚠️ **第一版有一条假测试被退回对照当场抓出**：`test_self_return_..._substitutes_to_type_param`
    断言 `body.Contains(":T")` —— 形参标注 `(ident a :T)` 也贡献 `:T` ⇒ 两态同绿。改成钉在
    call 节点上（`Copy :T` ↔ 退回态 `Copy :<unknown>`）才成真门。
- 🔒 **真实构建面破坏性对照**（#528 那道最值钱的）：把 cross-zpkg fixture 退回
  `bool eqVia(IEq a, IEq b)` 形态，`test e2e --dir cross-zpkg` 立即
  `FAIL self_type_cross_pkg (ext build)` ⇒ **E0454 活在真实构建路径上**，不只在单测 harness 里。
- 🎯 **欠债实测 = 0**：Part A 给既有泛型代码首次接上实参检查 + 返回类型换真，
  `build stdlib` 25/25、`build compiler` 全过、GREEN 全绿、自举字节不动点 3/3。
- 对照组：具体类收者路径不经过任何新逻辑 → 两态同绿（已在用例注释里写明它不是门）。
- 完整 GREEN + `test stdlib --mode jit` + `test e2e --dir cross-zpkg --mode jit` + `test bootstrap`。

## 🔴 顺带挖出的既存缺陷（已登记，不在本轮修）

改写 cross-zpkg fixture 时发现：**跨包泛型自由函数今天根本调不了**。

```z42
// 包 A（ext）
int idOf<T>(T a) { return 1; }
// 包 B（main）
idOf(p);        // E0402: cannot assign Point to T (argument) —— 显式 idOf<Point>(p) 也一样
```

与 `Self` / 约束**完全无关**（上面这个形态既无 `Self` 也无 `where`）。导入侧型参退化成普通类
⇒ `_hasGenericParam` 判 false ⇒ 落结构比对。同族于 #523 修的 `ImportedSymbolLoader` 四条类型
保真度，但那批修的是**方法**、自由函数漏网；#523 接上实参检查后它才从静默变成可见的红。

⇒ 本 change 的 `eqVia<T>` 因此放在 **main 同包**而非 ext；跨包接口静态类型的解析路径覆盖由
`getVia(IBox<int>)` 保住（它同时升格为 Part B 的无误报守卫）。
Deferred：`imported-generic-func-type-param-fidelity`。
