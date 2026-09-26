# 泛型类型实参推断

> **页型**: 机制页 ｜ **状态**: ✅ 已实现 ｜ **代码**: `src/compiler/z42c.semantics/src/TypeArgInference.z42`
> （`Infer` / `InferPreBinding` / `_unify` / `_resolvedForm`）· `MethodTypeArgSubst.z42` · `MethodTypeParamUse.z42`
> **相关**: [源代码编译流程](source-compile.md) · [架构总览](architecture.md) ｜ **对齐**: 2026-09-17
>
> 用户视角的「`where` 能写什么、报什么错」在参考手册的「泛型约束」页；本页只回答
> **省略尖括号时，编译器怎么把型参绑出来**，以及为此定下的三条不变式。
>
> 本页抽自原 `docs/book/src/language/generics.md`（三书重构批 3b）——那篇 1521 行里
> 这三节是唯一「近期、准确、纯实现」的内容，其余或与参考手册重复、或已被实现推翻。

## 类型实参推断（2026-09-08 `add-generic-type-arg-inference`）

省略尖括号的泛型方法调用 `IdOf(7)` 会从**已绑定的实参类型**结构化 unify 出方法级型参绑定
（裸型参 / 数组元素 / 实例化类型实参 / func 形参·返回四个层面递归）。推断成功后：

- 形参位按绑定代换 → 实参走与赋值 / `return` / var-decl **同一条**可转检查门；
- 复用 `ConstraintChecker.CheckMethod` 校验 `where` 约束（此前只有显式写类型实参才校验）。

**保守收口四条**（爆炸半径全部来自这里）：型参未全绑定 → 整体失败；同一型参绑到不同类型 →
见下「数值取最佳公共类型」；实参类型是 `Unknown`/`Error`
（含 lambda、target-typed new 的延迟位）→ 跳过该位；`params` 尾位整段跳过。
**失败 = 完全按改动前行为、不发任何诊断。**

**数值冲突取最佳公共类型**（2026-09-10 `generic-inference-best-common-type`）：同一型参在多个形参位
绑到不同类型时，若**双方都是数值**，取算术拓宽公共类型（`double > float > long > int`，复用二元
`int + long` 的同一张 `TypeFacts.ArithmeticResult` 表）而非整体失败 —— `Max(1, 2L)` 现在推出 `T = long`
并真正校验 where / 实参（`long` 归一同 `_resolvedForm`，与显式 `<long>` 同形）；3+ 参的折叠与顺序无关
（数值拓宽是格上的 max）。**边界**：无数值公共类型的冲突（`string` + `int`、`int` + `uint` 等）仍按
上面「整体失败、静默」处理 —— 把这类冲突改成响亮的 E0402 是独立的爆炸半径问题，留待单独 change。
**仍不回灌 `MethodTypeArgs`**（见下）⇒ 发射零改动、无格式 bump。

**不变式：推断出的类型实参必须与显式写出的同形**（2026-09-09 `fix-inferred-type-arg-not-resolved`）。
内建基元在 z42c 里**两种拼写并存**（`unify-value-types` 阶段 3 删掉 `Z42PrimType` 后的遗留）：

| 来源 | 类型对象 | `Name()` |
|---|---|---|
| 表达式类型（`21` 的 `.Type()`） | `Z42ClassType.Builtin` 轻量合成 | 关键字 `"int"` |
| 显式 `<int>`（`env.ResolveType` → `SymbolTable.BuiltinType`） | `Classes` 表里的包装类 `Std.Int32` | `"Int32"` |

而下游判定只认后者（`ConstraintChecker._satisfiesInterface` → `symbols.Implements(裸名)` →
`Classes.Find`）。推断的绑定值直接取自 `args[i].Type()`（上表第一行），若不归一就会出现
**`Double<int>(21)` 过、`Double(21)` 报「`int` 不满足 `INumber`」——差别只有一个尖括号**。
故 `TypeArgInference.Infer` 在**推断出口**统一过一次 `_resolvedForm`
（门取 `IsBuiltinType()`，码 0..13，含 `string`=12 / `object`=13；用 `IsScalarType()`（0..11）
会漏掉 `where T : IComparable` 下的 `Max("a", "b")`）。归一放出口而不是放各判定函数：
后者只是众多消费方之一，逐个打补丁等于承认「型参实参有两种形态」。

## 显式类型实参：**先代换签名，再绑实参**（2026-09-10 `fix-explicit-type-arg-not-substituted`）

写出 `Foo<int>(...)` 时，被调方的签名在**绑定实参之前**就按类型实参代换掉——
`Array.Sort<int>(xs, (a, b) => b - a)` 的第二个形参目标类型是 `Comparison<int>` 而不是
`Comparison<T>`，于是 lambda 形参 `a`/`b` 拿到 `int`。

**为什么必须在绑实参之前**：lambda 形参类型**只**来自目标类型
（`BindArgsToSignature` → `BindWithTarget(rawArg, 形参类型)`）。签名不代换，形参就是裸 `T`，
体内 `b - a` 报 `E0402: operator - requires numeric operand, got T`。全仓 18 条 E0402
全是这一个形状，且全在 golden 语料里——运行期照常通过（型参已擦除），只有编译期在报，
而 `--emit-zbc` 把它吞了。

实现：`MemberResolver._substForExplicitTypeArgs` 在 7 个调用形态（自由函数 / 实例 / 接口 /
实例化 / 裸类名静态 / prim wrapper 静态 / ns 限定静态）各接一次，产出一个换了签名的
`MethodSymbol` 浅拷贝（`MethodSymbol.WithSignature`，`RegKey` 原样保留 ⇒ 发射目标不变）。
按名代换需要方法级型参**名**，故 `MethodSymbol` 新增 `TypeParamNames`（本地取
`Decl.TypeParams.Names`，跨包取 `ExportedMethodZ.TypeParams`——那些名字早就读进来了，
只是此前只当解析上下文用、没留在符号上）。

> 🔴 **`_substByName` 的 `Z42FuncType` 分支是这次补的**。此前它只递归数组元素与实例化类型实参，
> 而 `TypeArgInference._unify` 的注释把这个不对称记成「这不会出错（只是少换一次）」——**那句话是错的**：
> func 位正是 lambda 形参类型的唯一来源，少换一次就是上面那 18 条。

**与推断路径的分工**（两条不变式并存，别混）：

| | 显式 `Foo<int>(…)` | 推断 `Foo(…)` |
|---|---|---|
| 代换时机 | `_withDefaults` **之前** | 非-Func 位：`_withDefaults` **之后**（仅诊断）；**Func 位：之前**（见下 lambda 节） |
| 影响面 | 实参绑定（含 lambda 形参类型）+ 诊断 | 非-lambda 位：仅诊断；**lambda 位：实参绑定 + 诊断** |
| 回灌 `MethodTypeArgs` | 照旧写（本来就写） | **不写**（design D4） |
| 自举字节 | 会漂（闭包形参类型进 zbc），走两代收敛 | 零漂移（build 源零命中省略-`<>`+lambda 形态，不动点 3/3 兜底） |

推断路径那条「代换结果绝不回灌」的不变式**不受影响**：它针对的是**推断出来**的类型实参
（不回灌是数据裁决的结果，见下）；显式写出的类型实参不在其约束范围内——`Sort<int>` 的签名
本来就是 `Sort(int[], Comparison<int>)`，让实参照着它绑才是正确语义（C# 同）。

**推断结果刻意不回灌 `BoundCall.MethodTypeArgs`**：回灌会把 opcode 从 `Op.Call` 换成
`Op.CallGeneric`、重排 zbc 字符串池、并关掉 `exec_call.rs` 的 native 快路径门；而全仓普查显示
隐式泛型调用 **112 处全部是 `Array.Copy<T>`**，其 `T` 纯粹是编译期类型安全装置（函数体只做参数
校验，搬运落到非泛型 native 原语 `CopyRange`）⇒ 回灌是纯回归。代价由 **E0455** 兜住：callee 体内
真消费型参（`typeof(T)` / `new T()` / `default(T)` / `new T[n]`，或把 `T` **转发**给嵌套泛型调用）
时，省略尖括号直接报错、要求显式写出——把静默错值换成编译错误。

## lambda 实参驱动推断（2026-09-11 `generic-inference-lambda-args`）

省略尖括号时，**lambda 实参也参与型参推断**——这是让 `Map(nums, n => n * n)` /
`Filter(nums, n => n > 4)` / `Array.Sort(xs, (a, b) => b - a)` 这类**无标注 lambda** 能编、能跑的关键。
两个推断源：

- **源①（非-lambda 实参）**：型参从数组 / seed 等普通实参推出（如 `Array.Sort<T>(T[], Func<T,T,int>)`
  的 `T` 从 `xs` 推出）。
- **源②（lambda 自身标注）**：型参**只**出现在 Func 形参位时（`Compose<T>(Func<T,T>, Func<T,T>)`），
  从 lambda **带标注**的形参类型推出（`Compose((int x) => …, (int y) => …)` ⇒ `T = int`）。

推断出的型参**只代换 Func 形参位**（`MethodTypeArgSubst.SubstituteFuncParams`），据此在**绑定 lambda
之前**把目标从 `Func<T,…>` 换成 `Func<int,…>`，于是**无标注 lambda 形参拿到具体类型**、体内运算定型、
发射具体 opcode。**非-Func 形参位保持原裸型参**——非-lambda 实参的绑定 / 装箱 / params 展开 / 默认值
逐字节不变（`Array.Copy<T>` 等无 Func 位的隐式泛型调用零触碰）。这也解除了一个既存缺陷：此前 lambda
位的裸 `T` 会与其它实参推出的绑定**冲突**、令整条推断失败（连 `where` 都不校验）。

**两条边界**：

- **无标注纯 A 无解**：型参只在 lambda 里、**且该 lambda 形参无标注**（`Compose((x) => …)`）——鸡蛋
  依赖（不绑体拿不到型参、不知型参绑不了体），保持今天行为（静默）。C# 靠 target-typing 多阶段推断
  部分破解，z42 暂不引入。**有标注**（`(int x) => …`）经源②可解。
- **只在 lambda 返回位的型参不回灌**：`Map<T,U>(source, Func<T,U> f)` 的 `U`——`T` 推出后代换进
  `Func<T,U>` 使 lambda **体**能编，但 `U` 不回灌（同上「不回灌」原则）⇒ 调用方看到的返回类型
  `List<U>` 里 `U` 仍是裸型参。

> 与「显式 `<>` 全代换」的区别：显式路径代换**整条**签名（用户显式要求、接受全面字节变化）；
> 本推断路径**只**放开「lambda 重绑」这一条通道，其余对非-lambda 位一律关闭 ⇒ 对自举 / stdlib 构建
> 零字节漂移（build 源里没有「省略 `<>` + lambda」形态；不动点 3/3 gen1==gen2 兜底）。

## 类级型参的载体：沿基链定位**声明类**（2026-09-26）

类级 `typeof(T)` / `default(T)` 的载体此前只有一个：**受者自己**的 `type_args`。
于是从泛型基继承来的型参取不到 —— `class Derived : Box<int> {}` 的实例自己没有实参：

```z42
class Box<T> { string Tof() { return typeof(T).FullName; } T Def() { return default(T); } }
class Derived : Box<int> { }
new Derived().Tof()   // 修前 → 占位名 "T"（应为 Std.Int32）
new Derived().Def()   // 修前 → null（应为 0）
```

⭐ **实参其实一直都在 —— 在基的名字里**，只是此前被剥掉了。#831（P1）让**闭合**泛型基保留
实参（`Derived` 的 base 现在是 `Demo.Box<int>`）之后，这条路才通。

**两个载体，顺序是关键：**

| # | 载体 | 何时用 |
|---|---|---|
| ① | **声明那一层的名字** | 沿基链走到 erased 名 == 声明类的那层，从名字解析实参 |
| ② | 受者自己的 `type_args` | ①落空时的回落 —— 跨包泛型没有身份名，实参只在这个载体里 |

🔴 **②绝不能在「声明类是某个基」时生效**：`class DG<U> : Box<int>` 的受者
`type_args = [U 的实参]`，按下标 0 读回答的是 `DG` 的型参，而问的是 `Box` 的
—— **看起来对的错类型**，比占位名更糟。实测 `DG<string>` 曾把 `Box` 的 `T` 报成 `Std.String`
（这条是新用例抓出来的，不是设计时想到的）。

所以 builtin 多带一个**声明类 FQ 名**实参，按它定位那一层 —— 精确，不是按下标启发式。
类级 `default(T)` 也因此从 `DefaultOfInstr` 改发 `__class_default`（指令只带下标，加操作数要
格式 bump；builtin 不用，同 `__class_type_arg` 的先例）。

### 🔴 静态语境：诚实的 null，不要读 `regs[0]`

`DefaultOfInstr` 读 `regs[0]`，而**静态帧的 reg0 是第一个实参、不是 `this`**：

```z42
class Box<T> { static string S(Box<int> probe) { … default(T) … } }
Box<string>.S(boxOfInt)   // 修前 → "0"（读了 probe 的实参表！）应为 null
```

类级 `typeof(T)` 早就有这道语境判据（`TypeOpTyper` 注释写明了同一个理由），`default(T)` 一直漏着
—— 把「明显不知道」升级成了「看起来对的错值」。现在静态语境给 null，与 typeof 给占位名同口径。

## 含型参的形参位由谁检查（2026-09-25 `fix-ctor-param-resolved-in-caller-scope`）

实参检查分两条路，**按形参声明类型含不含型参分区**，不重不漏：

| 形参声明类型 | 谁查 | 拿什么当目标类型 |
|---|---|---|
| 含型参（`U` / `U[]` / `G<U>`） | `CheckSubstitutedArgs`（方法）/ `ConstructTyper._chkCtorSubstArgs`（ctor） | 按 receiver 实参**代换后**的类型 |
| 不含型参（`int` / `string`） | `_adaptArgs` 的 `CheckArg` / `_checkOneArg` | 声明类型本身 |

🔴 **修前 ctor 那条路两边都查**，而它的目标类型来自
`OverloadBinder._adaptParamType` 的 `md != null` 分支 —— 那是在**调用点**的环境里
`env.ResolveType(md.Params[i].Type)` 重新解析声明类型。型参在调用点根本不存在 ⇒ `Unknown`。

**同一个根因，两副面孔：**

```z42
class Bare<U> { Bare(U v) {} }
new Bare<int>("x");        // U → Unknown，被转换格吸收 ⇒ 静默放行（漏报）

class Wrap<U> { Wrap(G<U> i) {} }
new Wrap<int>(new G<int>(7));
// G<U> → G<<unknown>>，与 G<Int32> 结构比 ⇒ E0402（误报）——这个类根本构造不出来
```

⭐ 只有**嵌套**形态会变成误报：裸型参与数组型参的 `Unknown` 被转换格吸收，看上去「没事」。
⇒ 修完之后**漏报那一侧会开始响**，这正是必须 bump `CompilerFingerprint` 的理由
（那类源文件此前编得过、哈希一字未变）。发码零变化（自举不动点 3/3 兜底）。

用例：`src/compiler/z42c.semantics/tests/typecheck/ctor_param_scope_tests.z42`（误报侧 3 条 +
漏报侧 4 条，两侧都立）与 `src/tests/generics/generic_ctor_param_scope.z42`（端到端）。

## 限制（本阶段）

- ~~primitive 类型（int/string/...）**未**实现 interface，`Max<int>(1, 2)` 暂不可用~~
  ✅ **已失效**（2026-09-09 核实更正）：`src/libraries/z42.core/src/Primitives/` 下每个 wrapper
  都写着 `struct Int32 : IComparable, IEquatable, INumber`，`Max<int>(1, 2)` / `Double<int>(21)`
  编译期与运行期都正常。这条限制在实现落地后**一直没有被撤**——而 `Max(1, 2)`（省略尖括号）
  当时确实报「`int` 不满足 `IComparable`」，那不是本条限制，是
  `fix-inferred-type-arg-not-resolved` 修掉的归一缺口。
- 约束不写入 zbc 二进制（仅编译期使用），VM 不做运行时校验（**L3-G3 必须补齐**）
- 其他约束范式排期见 L3-G2.5 子迭代（见下）
- **返回类型不按推断代换**：推断只驱动诊断，不进入任何发射决策（不变式：**推断**的代换结果
  绝不回灌 `_withDefaults` / 装箱 / params 打包 / 重载决议——那四条通道每条都是确定性的自举
  字节漂移）。⚠️ 这条只管**推断**路径；**显式**写出类型实参时代换发生在 `_withDefaults`
  **之前**（见上「显式类型实参」节），那是有意的、且已接受字节漂移代价
- **推断不参与重载决议**：一律在决议选定唯一候选**之后**做（提前会把 `void F(int)` 与
  `void F<T>(T)` 变歧义，让今天能编的代码编不过）

