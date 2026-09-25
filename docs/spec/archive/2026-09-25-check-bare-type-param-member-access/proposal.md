# Proposal: 型参收者上的成员访问 —— 先修静默错值，再补缺失的检查

> 状态：🔴 DRAFT，待 User 确认。类型：**lang**（A 修语义缺口 + B 收紧类型规则、新增编译错误）⇒ 走完整流程。

## Why

**规则文档里有，发它的检查不在。** 三本书都写明了裸型参上什么可用：

- `docs/learn/src/types/generics.md:304`：「裸 `T` 上什么都调不了；**`where` 是让成员变可用的开关**」
- `docs/reference/src/language/generic-constraints.md:265`：「`Object` 的成员（`ToString` /
  `GetHashCode` / `Equals`）**优先**于约束接口」

而实测下来，这条规则的**两侧都没落实**：**约束提供了的读出静默错值**（A），
**约束没提供也不是 `Object` 成员的则编译期零诊断、运行期崩**（B）。

```z42
struct Vec2 { public long X; public long Y; }
T id<T>(T a) { return a; }

id(v).X          // 🔴 运行期 FieldGet: expected object, got BoxedStruct(…整屏堆转储…)
id(v).Bogus      // 🔴 同一个运行期错误 —— 字段压根不存在，编译期也零诊断
id(v).NoSuch()   // 🔴 运行期 VCall on boxed struct
id<Vec2>(v).X    // ✅ 写了显式类型实参就正常

T f<T>(T a) { long q = a.X; return a; }   // 🔴 泛型体内同样零诊断、运行期崩
```

这是与 **E0407**（DA pass 随老编译器丢了）、**WS013**（规则是真的、发它的 lint 不在了）
**同族**的缺口：规范成立、执行缺位。也与刚修掉的坑点 ①（基元收者上不存在的成员零诊断，#801）
是同一类——`.Bogus` / `.NoSuch()` 这种**必然错**的写法也被放行。

**根因机制**（`generic-constraints.md:268-272` 自己记着）：型参收者查不到 `Object` 成员就
**松绑成 `sig = null`**，而实参检查第一行就是 `if (sig == null) return;` ⇒ 整条检查被跳过。
`check-constraint-iface-method-args`（2026-09-13）收紧了「约束接口方法的实参」那半，
**「成员本身存不存在 / 可不可用」这半仍开着**。

不做的代价：泛型代码里的拼错成员名、以及「忘了写显式类型实参」，一律推到运行期，
错误信息是整屏 `ScriptObject { … }` 堆转储，离现场极远。

## What Changes

### A. 先修一个静默错值：约束提供的**属性**在型参收者上读出 `null`

⭐⭐ **探索中发现的、比原目标更严重的一格**（实测）：

```z42
interface IHasName { string Name { get; }  string GetName(); }
class Person : IHasName { … }

string viaMethod<T>(T a) where T : IHasName { return a.GetName(); }   // ✅ "Ada"
string viaProp  <T>(T a) where T : IHasName { return a.Name;      }   // 🔴 null —— 静默错值
// 非泛型对照 p.Name → ✅ "Ada"；链式 a.Name.Length → 崩
```

**根因**：`bind-self-param-and-constraint-members` Part A 给**方法**路补了
`_constraintIfaceMethod` 约束查找（`MemberResolver.z42:254`），而**属性 / 字段**路
（`MemberResolver.z42:322` 的 **GS5 松绑**）**从来没补** —— 直接 `return BoundMember(…, Unknown)`，
运行期按名字当字段读 ⇒ 读不到 ⇒ `null`。

⇒ **A = 给 GS5 补同款约束查找**（约束接口的 `get_<Name>` → `BoundCall`，返回类型经
`_substSelfSig`）。**纯修复、零行为收紧**：只把 `null` 变成正确值。

### C. 顺带修第三个缺口：方法级 `where` 细化类级型参时，约束从不被校验

⭐⭐ **User 2026-09-25 裁决要求一并修**（否则 B 对 stdlib 的交付只是把静默挪个地方）。

`class List<T>` 的 `public void Sort() where T : IComparable` —— 方法级 `where` **细化**类级型参
——今天**能编、成员真可用**（实测），但**约束是否被满足从不检查**：

```z42
class Opaque { public int V; }                       // 不实现 IComparable
class Box<T> { public int Cmp() where T : IComparable { return this.a.CompareTo(this.b); } }
new Box<Opaque>(…).Cmp()   // 🔴 零诊断，运行期崩 VCall: function `Opaque.CompareTo` not found
```

**对照（两种都是对的）**：方法自己的型参 → ✅ E0402；类级型参带类级约束 → ✅ E0402。
**只有「方法级 where 细化类级型参」这一格漏**。

**根因是「互相推诿」**：
- 声明期 `ConstraintChecker._diagnoseMethodWheres:81` 在 **`md.TypeParams.Count == 0` 就早退**
  ⇒ 方法自己没有型参时整条 `where` 不看；
- 调用点 `CheckMethod:667-669` 对 `pi < 0`（where 的型参不在方法型参表里）**静默跳过**，
  注释写着「**那条也归声明期报**」。

⇒ 两边各自以为对方会报，**结果没人报**。同族于 E0407 / WS013 那类「规则在、执行缺位」。

**为什么 B 必须带上 C**：`List<T>.Sort()` 的 `T` 是类级型参，加**类级**约束会废掉
`List<任何不可比较类>`（灾难性 API 收紧）；而显式转 `(IComparable)x` 被语言**刻意堵住**
（实测 **E0454**：「a `Self` parameter has no safe bound here — use a type parameter instead」）。
⇒ 唯一可行的是方法级细化约束，而它今天不被校验 ⇒ 不修 C，B 就等于把「成员访问不检查」
换成「约束满足性不检查」，运行期照样崩。

### B. 再补诊断：一条规则、两个报错点

（判据同一个，措辞按修法分。A 落地后「约束提供」这一格已真正可用，B 才不会误伤它。）

| 报错点 | 形态 | 诊断 | 修法 |
|---|---|---|---|
| **调用点** | `id(v).X` —— 收者类型是从 callee 返回位**漏出来的**裸型参 | **E0455**（`GenericTypeArgRequired`，复用）| 写显式类型实参 `id<Vec2>(v)` |
| **体内** | `a.X` —— 收者是**当前作用域内**的不透明型参 | **E0401**（`UndefinedSymbol`，复用，镜像 #801 的基元收者路径）| 加 `where T : IFoo` |

**放行（成文规则，实测确认今天就能工作，必须不回归）**：

| 裸 `T` 上 | 今天实测 | 本变更后 |
|---|---|---|
| `a.ToString()` | ✅ `V(7)`（正确派发到 override）| ✅ 不变 |
| `a.GetHashCode()` | ✅ | ✅ 不变 |
| `a.GetType().Name` | ✅ `Vec2` | ✅ 不变 |
| `a.Equals(a)` | ✅ `true` | ✅ 不变 |
| `where T : IColl` 提供的 `a.Add(1)` / `a.Size()` | ✅ | ✅ 不变 |
| `typeof(T)` | 🟡 已报 E0455 | 不碰（不是成员访问）|
| `Vec2 r = id(v);`（不访问成员）| ✅ | ✅ 不变 |

**唯一的行为收紧**：`id(v).Sum()` 这类「成员恰好存在于运行期实际类型上、靠 VCall 动态派发
恰好能跑」的写法变成编译错误。按手册写明的规则它本就不该编得过。

## 🔑 与已归档 Deferred 的关系（必须说清）

`generic-constraints.md:296-299` 挂着 Deferred **`tighten-bare-type-param-target-erasure`**，
其通用那半仍开着，并预先警告：

> 需区分**作用域内不透明型参 vs 待推断型参**、要给 `Z42GenericParamType` 加 owner
> ——那是对通用规则动刀、**爆炸半径另算**。

- **本变更不是那一半**。那条 Deferred 管的是**目标位**擦除（把具体值赋给裸型参形参 /
  变量）；本变更管的是**收者位**的成员访问。两者不重叠。
- 但它警告的那个区分**正是本变更的两个报错点**。**好消息：不需要给
  `Z42GenericParamType` 加 owner。** 调用点本来就手握 callee 的已解析签名，照
  `RetIsNullable` 的既有接线（`BoundCall.Marked`）在结果上打一个标记即可；成员访问时看
  收者表达式有没有该标记就能分辨。⇒ 类型模型零改动，Deferred 预估的那份成本不发生。
- ⚠️ **不能用「名字在不在当前 env 的型参表里」当判据**：`void f<T>() { id(v).X }` 里
  caller 的 `T` 与 callee 的 `T` **同名不同物**，env 判据会误判成「作用域内」⇒ 给出错的修法。
  这正是 Deferred 说「要加 owner」的原因；标记法绕过了它。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---|---|---|
| `src/compiler/z42c.semantics/src/BoundExprOp.z42` | MODIFY | `BoundCall` 增 `RetIsErasedTypeParam` 标记；`Marked(bc, sig)` 里按 `sig.Ret is Z42GenericParamType` 置位 |
| `src/compiler/z42c.semantics/src/MemberResolver.z42` | MODIFY | **A**：GS5（`:322`）补约束接口 `get_<Name>` 查找（镜像方法路 `:254`）；**B**：方法路 `sig = null` 兜底（`:263`）与 GS5 兜底改为按收者是否带标记发 E0455 / E0401 |
| `src/compiler/z42c.semantics/src/ConstraintChecker.z42` | MODIFY | **C**：声明期不再对 `TypeParams.Count == 0` 早退（where 的型参是**类级**型参 ⇒ 合法细化；两边都不是 ⇒ 报未知型参）；新增调用点校验入口，按**收者的**类型实参验细化约束 |
| `src/libraries/z42.core/src/Array.z42` | MODIFY | 7 个排序/二分函数补 `where T : IComparable`（此前靠松绑）|
| `src/libraries/z42.core/src/Collections/List.z42` | MODIFY | `Sort()` 补方法级 `where T : IComparable`（细化类级型参）|
| `src/libraries/z42.core/src/Collections/List.Query.z42` | MODIFY | `BinarySearch(T)` 同款 |
| `src/compiler/z42c.semantics/tests/typecheck/bare_type_param_member_tests.z42` | NEW | 诊断单测：B 两个报错点 + C 的细化约束校验 + 放行护栏 |
| `src/tests/generics/bare_type_param_members.z42` | NEW | e2e 正面用例（`Object` 成员 / 约束成员 / 赋局部 在裸型参上仍可用）|
| `docs/reference/src/language/generic-constraints.md` | MODIFY | 「成员可用性」段补：不可用时报什么码、两种修法；更新该页的 Deferred 段（本变更关闭了「收者位」那半）|
| `docs/learn/src/types/generics.md` | MODIFY | 第 18 章：「裸 T 上什么都调不了」从**口头规则**升级为**会判红**，给出两种修法 |
| `docs/reference/src/appendix/error-codes.md` | MODIFY | E0455 / E0401 词条补本变更的触发形态（**不取新号**）|

**只读引用**：

- `src/compiler/z42c.semantics/src/Z42Type.z42` — `Z42GenericParamType` 只有名字（无 owner/约束）
- `src/compiler/z42c.semantics/src/MemberResolver.Prim.z42` — #801 基元收者报 E0401 的先例
- `src/compiler/z42c.semantics/src/ConstraintChecker.z42` — 约束成员查找（`Object` 成员优先的实现）
- `docs/spec/archive/*check-constraint-iface-method-args*` — 收紧了哪半、留下哪半

## Out of Scope

- 🔴 **让 `id(v).X` 真的工作**（而不是报错）。实测 IR 证明：显式 `id<Vec2>(v)` 之所以能跑，
  是因为发射了**特化函数 `@id<Vec2>`**（两份 IR 只差 callee 名，字段读都是裸 `field_get`、
  都没有 AsCast）⇒ 那要动**泛型特化**，属 [[z42-generic-instantiation-layout]] 线，
  且 `wt-geninst` 有别的会话正在做（**User 2026-09-25 裁决：泛型的不处理**）。
  已归档裁决 D4「推断只驱动诊断、刻意不回灌」本就要求用诊断兜住这一格 —— 本变更正是补那道诊断。
- 🔴 **`tighten-bare-type-param-target-erasure` 的目标位那半**（把具体值赋给裸型参）——
  仍为 Deferred，本变更不碰。
- ④a（泛型约束运算符派发的 sret ABI 错位）、blob 型参 + 调方法导致 ctor 解析不到 ——
  各自独立，后者属泛型特化线。

## Open Questions

- [x] 放行边界？→ **`Object` 成员 + `where` 提供的成员**（成文规则，且实测今天就能工作）。
- [x] 要不要取新诊断码？→ **不用**，两个报错点各复用 E0455 / E0401。
- [x] 怎么区分两个报错点而不给类型加 owner？→ **调用点在结果上打标记**（照 `RetIsNullable`）。
- [ ] **`id(v).Sum()` 从「恰好能跑」变编译错，是否接受？**（推荐接受：违反成文规则，且它今天
      能跑纯属 VCall 动态派发的意外，与 `static_abstract_operator` 那个「测试写了但抓不到
      bug」的成因同源。）**这是本变更唯一的行为收紧点** —— A 与 B 的其余部分都只把
      「静默错值 / 运行期崩」换成「正确值 / 编译期报错」。
- [x] `List<T>.Sort()` 怎么办？→ **User 裁决：加方法级细化约束 + 一并修 C**（选项对比见 What Changes C）。
- [ ] A / B / C 要不要拆 PR？（推荐**同一个**：B 的判据依赖 A 落地后「约束提供」这一格
      真的可用，否则 B 会把 `a.Name` 误报成「成员不可用」——两者是同一处代码的两半。）
