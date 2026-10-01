# tasks: fix-paramref-constraint-iface

> 类型：**fix**（`where U : T` 在上界是接口时拒掉合法代码）｜ 创建：2026-09-27
> 出身：结构审计 2026-09 的 R2「判据复制」——`Z42Type.IsAssignableTo` ↔ `Conversion.Classify`
> 两套可赋性判据并存。审计把它记成重构项；实测它在这一格上是**正确性 bug**。

## Why

`class Pair<T, U> where U : T` 实例化成 `Pair<IBase, C>`（`class C : IBase`）**编不过**：

```
E0402: type argument `C` for `U` does not satisfy constraint `T` on `Pair`
```

而同一个满足关系写成直接的接口约束（`class Box<T> where T : IBase` + `Box<C>`）一直是过的。

### 根因：谓词不带符号表，漏掉的恰好是「需要类表」的那几条

`ConstraintChecker._satisfiesParamRef` 显式处理了「上界是 class」（走 `symbols.IsSubclassOf`），
**接口那格没有**，落到兜底的 `arg.IsAssignableTo(other)`。而
[`Z42ClassType.IsAssignableTo`](../../../../src/compiler/z42c.semantics/src/Types/Z42Type.z42) 里
**根本没有 `other is Z42InterfaceType` 这一格** —— 它只判三件事：

1. 同名 class；
2. 内建 canonical 归一（`int` ≡ `Int32`）；
3. 两侧都是 Scalar 值类型时的数值拓宽。

⇒ 类→接口**恒 false** ⇒ 合法代码被拒。

这与 `OverloadResolver._assignable`（`fix-overload-ref-conversion`）踩的是**同一个根因**，
那条的注释已经把话说全了：

> `Z42Type.IsAssignableTo` **不带符号表** …… 于是被漏掉的恰好是**需要类表**的三条：
> 派生→基、类→接口、实例化泛型→基/接口；而数值加宽与 object 不需类表，所以它们一直过得去
> —— 这就是「数值和 object 行、用户类层次不行」这个反直觉现象的全部原因。

那刀修了**重载决议**这一路，本刀修**约束满足**这一路。同一个根因的第二个消费点。

### 顺带暴露的第二格：接口→父接口

`Pair<IBase, IDerived>`（`interface IDerived : IBase`）修前也报 E0402 ——
`Z42InterfaceType.IsAssignableTo` 只认 `SameInterface`（**同一个**接口），不走父接口链。
本刀改走 `_satisfiesInterface` → `InterfaceClosure.IsInterfaceSubtypeByName` 后一并认了。

### ✅ 运行期一直是放行的 —— 偏离唯一真相源的是编译期

`generic-constraints.md` 抬头立着一条规矩：**判定规则的唯一真相源是运行期
`validate_type_arg_constraint`，编译期照抄同一套，两边不各判各的**。放宽编译期之前必须先查
运行期，否则会造出一个「编得过、跑起来拒」的裂口。查的结果正好相反：

- 运行期型参引用约束走 `type_name_assignable` → `is_subclass_or_eq_td`
  → `is_subclass_or_eq_td_walk`（`src/runtime/src/interp/dispatch.rs:148`），
  那个 walk **既走 base 链也走 `cur_td.interfaces()`**，且接口是**传递**匹配的
  （`add-reflection-transitive-interfaces`）。
- ⇒ `Pair<IBase, C>` 在运行期**一直满足**。

**修前是编译期比运行期更严** —— 偏离唯一真相源的是编译期那一侧。本刀是把它拉回来，
不是放宽语义。这条比「测试变绿」更硬：它说明修的方向由既有规矩唯一确定。

### 🔴 文档此前是超发的

`docs/reference/src/language/generic-constraints.md:38` 的「型参引用」行写着满足条件
「U 的实参可赋给 T 的实参」、编译期校验「✅」。**「可赋」在上界是接口时并不成立** ——
而同表「接口」行还明确承诺了「含接口继承链」。两行放在一起读，用户只会得出「型参引用当然
也认接口链」的结论。本刀让实现追上文档，并给那一行补了脚注说明判据出处。

## What Changes

`ConstraintChecker._satisfiesParamRef`：在兜底的 `IsAssignableTo` **之前**加一格，
上界是接口时复用本文件已有的 `_satisfiesInterface`。

```z42
if (other is Z42InterfaceType) { return this._satisfiesInterface(symbols, arg, other.Name()); }
```

### 为什么只加这一格、不顺手重写整个函数

| 想动的地方 | 为什么不动 |
|---|---|
| 把「上界是 class」那格换成复用 `_satisfiesBase`（两者代码逐字相同） | **会收紧**：原分支在 `arg` 既不是 class 也不是实例化泛型时**落到 `IsAssignableTo`**，而 `_satisfiesBase` 直接 `return false`。基元实参 + 内建类上界（`where U : T`，T→`object`/`Int32`）这条路会从过变成不过 —— 把一个误报换成另一个误报。 |
| 补上 `other is Z42InstantiatedType`（`where U : T`，T→`Box<int>`） | 唯一顺手的写法是取 `.Def.Name()` 走 `IsSubclassOf`，而那**会忽略类型实参** ⇒ `Pair<Box<int>, Box<string>>` 静默放行。**把误报换成静默错值，严格更坏** —— 本仓已有一条同款教训（`Z42InterfaceType.SameInterface` 的注释：只比 `Name()` 会把 `IBox<int>` 与 `IBox<string>` 判成同一个，实测是静默错值）。要做对得走带 `_sameTypeArgs` 的那条路，属另一刀。 |
| 整体迁到 `Conversion.Classify` | 审计原本的提法。但 `OverloadResolver` 那刀已经实测记录过：`Classify(...).ImplicitOk()` 的白名单含 ImplicitNumeric / Boxing / **UserImplicit**，换过去会把**用户 `op_Implicit`** 拉进约束满足性（语义扩张：约束是子类型关系，不是转换关系），并把数值门从 `CanWiden` 换成更严的 `_widensLossless`。**窄口只补需要类表的那几条**是本仓已经验证过的正确取舍。 |

⇒ 本刀对 `arg` 的每一格**只放宽不收紧**：类→接口 / 实例化泛型→接口 / 接口→父接口三格从
恒 false 变成查表，其余格（基元 / 数组 / func / void）照旧 false，与改动前逐位相同。

## 其余调用点：逐个扫过，没有第二处同形的可达缺口

审计把这条记成「12 个调用点的迁移」。逐个看过之后，**只有一处是可达的正确性缺口**：

| 调用点 | 目标类型 | 判定 |
|---|---|---|
| `ConstraintChecker:659` | 型参的上界 | 🔴 **本刀修的那处** |
| `OverloadResolver:396 / 409` | 形参类型 | ✅ 已修（`_refUpcast` 带符号表，`fix-overload-ref-conversion`） |
| `OverloadBinder` ×6（75/114/414/558/780） | **params 数组类型** | 接口盲区不适用（目标是 `Z42ArrayType`）。要出错需要**数组协变**，而 z42 没有（`Conversion` 只有「数组→`Array` 基类」一条，无逐元素协变）。 |
| `ConstructTyper:164` | 同上，params 规范形态判定 | 同上 |
| `ExprTyper.Funcref:32` | 函数签名 | **刻意精确全签名**（消解语义就是逐位相等、无变型），不是缺口 |
| `Conversion:173` | —— | 它**就是** `Classify` 的分支 D，处在正确的位置 |
| `TypeChecker:707` | —— | ⚠️ **只剩一句注释，没有代码**（实现早已搬去 `TypeFactsTc._isAssignable` → `Conversion.Classify`，那条带符号表）。注释本身过期，但不影响行为。 |
| `InheritanceResolver:568-574` | 接口成员协变 | 注释明写**刻意不用** `IsAssignableTo`（它会放行数值拓宽），走 table |

⇒ 审计「12 个调用点全迁 `Classify`」的提法，实测下来**不是一刀能做也不该一刀做的事**：
其中 6 处的目标是数组、1 处刻意精确、1 处已经是 `Classify` 本身、1 处只是过期注释。

## Scope（允许改动的文件）

- `src/compiler/z42c.semantics/src/Types/ConstraintChecker.z42`
- `src/compiler/z42c.semantics/tests/typecheck/constraint_paramref_tests.z42`（新）
- `src/tests/generics/paramref_constraint_iface.z42` + `.opt_all`（新，运行期覆盖）
- `src/compiler/z42c.pipeline/src/CompilerFingerprint.z42`（追加 slug）
- `docs/reference/src/language/generic-constraints.md`（型参引用行的脚注）

## 指纹：追加 slug

**bump 的理由是诊断变**：`Pair<IBase, C>` 此前报 E0402、现在零诊断，而**源码哈希一字未变**
⇒ 不 bump 就会命中旧缓存条目、修复永不生效。发码零变化 ⇒ CI 的 fingerprint 守门对这一档是瞎的。
（`version-bumping.md` 的第 1 行已在 #901 里明确扩到含「发出的诊断」。）

新方案是**追加一条 slug**而非改数字（#901 落地）——不再有让号问题。

## 实测：阴性对照精确到位

只撤回那**一行**、其余一字不动、重建编译器后重跑（判据只变一个变量）：

| 用例 | 带修复 | 撤回后 |
|---|---|---|
| `iface_bound_class_impl_satisfied_ok`（`Pair<IBase, C>`） | PASS | 🔴 **FAIL** |
| `iface_bound_derived_iface_satisfied_ok`（`Pair<IBase, IDerived>`） | PASS | 🔴 **FAIL** |
| `iface_bound_unrelated_class_rejected`（`Pair<IBase, D>`） | PASS | PASS |
| `iface_bound_reversed_iface_chain_rejected`（`Pair<IDerived, IBase>`） | PASS | PASS |
| `class_bound_subclass_satisfied_ok`（`Pair<B, S>`） | PASS | PASS |
| `class_bound_unrelated_rejected`（`Pair<B, D>`） | PASS | PASS |
| `same_type_satisfied_ok` | PASS | PASS |
| `unresolved_type_param_forward_no_false_positive` | PASS | PASS |
| **其余 23 个 z42c unit** | 全过 | 全过（`1 unit(s) failed (of 24)` = 只有我这个） |

⇒ **恰好 2 条变红，且正是两条接口上界的正例**。4 条负例全程保持 PASS
—— 这一条分辨的是「真的认了类表」与「把闸门整条放开了」，后者会让负例一起变绿。

## Tasks

- [x] `_satisfiesParamRef` 加接口那格
- [x] 8 条新单测（4 正例 / 4 负例对照，每对只差一个变量）
- [x] **阴性对照**：撤回这一行后 2 条正例精确变红、4 条负例保持 PASS（见上表）
- [x] `xtask test compiler` 零失败（`✅ z42c [Test]: all 24 unit(s) passed`）
- [x] 自举不动点 3/3 gen1==gen2
- [x] 文档：`generic-constraints.md` 补「型参引用的可赋范围」一节（含两条已知缺口）
- [x] 指纹追加 slug `fix-paramref-constraint-iface`
- [x] e2e fixture `paramref_constraint_iface` + **`opt_all` 侧车**（泛型类目此前 sidecar 为零，
      #888 就是这么炸出来的）：interp + jit 双绿
- [x] `xtask test lines` / `test diagcodes` 零新增
- [x] `xtask test e2e` 全绿：**742 / 87 / 3 passed，0 failed**（interp + jit 两档）
- [ ] GREEN：CI 全矩阵绿

⚠️ `xtask test fingerprint` 本地跑不了（它要 `--base <base 源码树根>`，那棵树得先用 base
编译器建好 stdlib）——这门的判定以 CI 为准。

## 不做（Out of Scope）

- **不补 `other is Z42InstantiatedType`**（理由见上表：会把误报换成静默错值）。
- **不整体迁 `Conversion.Classify`**（理由见上表：语义扩张 + 诊断质量倒退）。
- **不清 `TypeChecker.z42` 的孤立注释**。查 `IsAssignableTo` 调用点时发现：该文件
  **约 700 行之后到文件尾是一整段没有代码的注释**（`// 赋值兼容（含类继承 subclass→base）…`
  `// lambda 绑定…` `// default(T)…` `// add-params-varargs D4…` 等等），描述的方法早已搬去
  别的文件（`ExprTyper` / `MemberResolver` / `StmtBinder`），**注释留在原地且不指向新位置**。
  这是本仓已记过多次的「注释即第二份真相」形状的又一窝，但清它要逐条确认每个方法的新家、
  与本刀毫无关系，属独立一刀。**记在这里，谁要动直接接手。**
- **不补「基元满足接口约束」**（`where U : T`，T→`IComparable`，U→`int`）。C# 里 `int` 实现
  `IComparable`，这里改前改后都是不满足 —— **改前改后一致**，所以不是本刀造成的。
  它与直接的接口约束走的是**同一个出口**（`_satisfiesInterface`），所以两条路的行为必然一致。
  ⚠️ 我**没有**单独实测「直接写 `where T : IComparable` + `Box<int>` 也不过」这件事
  （那需要链 stdlib 的路径），只从共用出口推断；真要动这一格时先把它测掉。
