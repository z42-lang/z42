# 泛型约束（`where` 子句）

> 对齐：2026-09-11（change `resolve-method-where-at-decl`）
>
> 上一次：2026-09-10（change `validate-func-type-constraint`）
>
> 上一次：2026-09-10（change `fix-func-constraint-reported-unknown`）；2026-09-06（change `add-associated-types` PR-1/PR-2；前序 `complete-where-constraints`）
>
> 本页是**泛型约束语义与校验范围的 SoT**。泛型的整体设计（代码共享策略、reified 类型、
> 跨 zpkg 元数据）；
> 方法级类型参数见 [泛型方法](generic-methods.md)。

## 语法

```z42
class Box<T> where T : IFoo { }                  // 接口约束
class Box<T> where T : IFoo + IBar { }           // 多约束用 `+` 分隔（Rust 风格，非 C# 的 `,`）
class Map<K, V> where K : IEquatable where V : class { }      // 每个型参一条 where
void Sort<T>(T[] xs) where T : IComparable { }                // 方法级
```

多个型参各写各的 `where`；同一型参的多条约束用 `+` 连接。

## 七项约束

判定规则的**唯一真相源是运行期** [`validate_type_arg_constraint`](https://github.com/z42-lang/z42/blob/main/src/runtime/src/corelib/reflection/generics.rs)
——它是 `MakeGenericType` 这条绕过编译期的反射入口的自我把关。编译期照抄同一套规则，
两边不各判各的。

| 约束 | 语法 | 满足条件 | 编译期校验 |
|------|------|---------|-----------|
| 接口 | `where T : IFoo` | T 或其基类实现 IFoo（**含接口继承链**：`class C : IDerived`、`interface IDerived : IBase` ⇒ C 满足 `IBase`） | ✅ |
| 基类 | `where T : Base` | T 是 Base 或其子类 | ✅ |
| 引用类型 | `where T : class` | T 非值类型（基元与 struct 不满足；`string`/`object` 满足） | ✅ |
| 值类型 | `where T : struct` | T 是值类型 | ✅ |
| 枚举 | `where T : enum` | T 是 `enum` 声明的类型（基元**不**满足） | ✅ |
| 无参构造 | `where T : new()` | 基元满足；类须**非 abstract** 且可零实参构造 | ✅ |
| 型参引用 | `where U : T` | U 的实参可赋给 T 的实参 | ✅ |
| 函数类型 | `where T : Func<int, R>` | T 是函数类型，且 arity 相同、**形参逆变 / 返回协变**地匹配 | ✅ **E0422**（签名不符）/ **E0423**（与其它约束并置）；⚠️ 仅编译期、仅本包声明的约束，见下 |

`class` 与 `struct` 同时出现在一个型参上 → 报错（互斥）。函数类型约束与**其余任何**约束
并置也报错（`E0423`）——见下「函数类型约束」。

> ⚠️ 上表的「唯一真相源是运行期」对**函数类型约束不成立**：运行期
> `validate_type_arg_constraint` 只有七项，zbc 的约束 flag 位里也**没有** func 签名槽。
> 这一项是**纯编译期**规则，且只对本包声明的约束生效（导入 bundle 恒无 func 约束）。

### `new()` 的一条易错规则

**完全没有声明任何构造器 = 默认构造 = 满足 `new()`。** 只有「声明了构造器、却没有一个能零
实参调用」才不满足。「能零实参调用」包括：无参构造器、形参**全部带默认值**、`params` 变长
构造器。abstract 类一律不满足。

```z42
class Plain { }                                   // ✅ 满足：无显式 ctor
class HasNoArg { public HasNoArg() { } }          // ✅ 满足
class AllDefault { public AllDefault(int x = 1) { } }   // ✅ 满足：形参全带默认值
class NeedsArg { public NeedsArg(int x) { } }     // ❌ 不满足
```

### 基元满足 `new()`，构造出来的是**零值**

`where T : new()` 接受基元（上表第三行），`new T()` 于是要对它们有个答案 —— 答案是
**与 `default(T)` 相同的零值**（对齐 C#：`new int()` ≡ `default(int)` ≡ `0`）：

| T | `new T()` | `default(T)` |
|---|---|---|
| `int` / `long` / `short` / `byte` / `sbyte` / `ushort` / `uint` / `ulong` | `0` | `0` |
| `float` / `double` | `0` | `0` |
| `bool` | `false` | `false` |
| `char` | `'\0'` | `'\0'` |
| **`string`** | **`""`** | **`null`** ← 唯一不同的一格 |

`string` 那一格刻意分开：`default` 是「没有值」，`new` 是「构造一个」。z42 允许
`new string()` 正是因为上面那条「完全没有声明任何构造器 = 默认构造」——`Std.String`
没有声明实例构造器（C# 则反过来，`new string()` 是 CS1729）。

> 📜 **2026-09-25 之前这里是坏的**：`new int()` / `new T()` 会分配一个**空的包装类对象**
> 而不是值。`int` / `double` / `char` 崩在不相干的地方（`Int32.ToString: arg 0 expected int`），
> `bool` 更糟 —— 产出的东西 `if` 判**真**，却 `== true` 与 `== false` **同时为假**，零诊断。

**基元没有任何构造器，所以带实参一律报 E0426**：

```z42
int x = new int(5);        // ✗ E0426: `new int()` takes no arguments — a primitive has no constructor
string s = new string(cs); // ✗ E0426（提示改用 `String.FromChars(cs)`）
```

## 校验发生在哪里

| 时机 | 位置 | 报什么 |
|------|------|--------|
| 声明期 | 每个泛型类 / **接口**的 `where` 解析成约束集；**类成员方法与顶层自由函数的 `where` 也在此过一遍**（只发诊断，方法级不预登记符号表） | 未知型参 `E0401`、`class`/`struct` 互斥 `E0402`、未知约束名 `E0443`、func 约束并置 `E0423`、关联类型绑定名笔误 `E0453` —— 每条**只发一次**，与是否被实例化 / 被调用无关 |
| 类型引用位（use-site） | **凡是写出一个受约束泛型类型实例化的地方**（check-constraints-all-type-refs）：<br>· **体内位**：`new Box<D>()`、局部变量声明、`cast`/`as`、`is`、`default(T)`、`typeof(T)`、catch<br>· **声明位**：字段 / 属性 / 索引器类型、方法（含自由函数）形参·返回类型、基类·接口列表<br>· **嵌套**：`Wrap<Box<D>>` 逐层下钻，内层约束不因外层无约束而逃逸 | 违反约束 `E0402`，Span 指向该类型引用处 |
| 方法调用点 | `obj.m<T>(...)` / `C.m<T>(...)`（显式写类型实参）**及 `m(...)`（推断成功时）**；顶层自由函数同样走这条 | 违反约束 `E0402`、**函数类型签名不符 `E0422`** |
| 方法调用点（**细化类级型参**）| `list.Sort()` —— 方法的 `where` 约束的是**所属类**的型参（见下「方法级 `where` 细化类级型参」）；按**收者的**类型实参校验 | 违反约束 `E0402` |

> 调用点只报**违反**（`E0402` / `E0422`）——那本来就是 per-call-site 的事实，同一个方法被调 3 次
> 传 3 个不合格实参就该报 3 条。**声明级**诊断（约束名写错、并置非法等）已于
> `resolve-method-where-at-decl` 全部挪到声明期，见「已知限制 5」。

> **类型引用位的实现分两条 choke point**（check-constraints-all-type-refs）：体内位由绑定期的
> `TypeChecker._chkTypeRef` 覆盖（access / 弃用 / 跨包重复诊断都挂在这，约束校验并入）；声明位由
> `ConstraintChecker.CheckDeclTypeRefs` 覆盖——它必须放绑定期，因为声明位的收集期入口
> `SymbolCollector._chkTypeRefT` 在 `CollectAll` 时跑、约束尚未 `Resolve`。嵌套下钻由
> `ConstraintChecker.Check` 开头的**无条件递归**实现（在 `HasConstraints` 早退之前，否则外层无约束的
> `Wrap<Box<D>>` 会让内层 `Box<D>` 逃逸）。纯诊断、不回灌发射 ⇒ 无格式 bump、自举字节不动。

诊断都携带真实 Span：约束声明错误指向 `where` 所在行，违反错误指向实例化 / 调用处。

**本包与跨包同口径**：导入**类型**的约束走同一个校验函数，判定规则完全一致（见下节）。

🔴 **例外：方法级 `where` 不跨包**（实测 2026-09-25）。TSIG 不导出方法级约束，
故**导入**的泛型方法 / 自由函数，其 `where` 在调用点**不被校验**：

```z42
// Array.Sort<T> 声明了 where T : IComparable，但它来自 z42.core（导入）
Array.Sort<Opaque>(arr);   // ⚠️ 跨包无诊断（同包写同样的代码则报 E0402）
```

**类级**约束不受影响（`add-associated-types PR-1` 给它建了 TSIG 通道，跨包正常报 E0402）。
这条限制对「方法自己的型参」与「细化类级型参」两种形态**一样成立**。
已登记后续 `export-method-level-wheres`。

## 方法级 `where` 细化类级型参

方法可以用自己的 `where` **收紧所属类的型参**，从而在该方法体内使用更多成员 ——
约束**只作用于这个方法**，不上升为类级：

```z42
class List<T> {
    public void Sort() where T : IComparable { … }   // 只有 Sort 要求可比较
    public void Add(T item) { … }                    // Add 不要求
}

List<int>    xs;  xs.Sort();   // ✅
List<Opaque> ys;  ys.Add(o);   // ✅ —— 类级无约束，构造与其余成员照常可用
                  ys.Sort();   // ❌ E0402（同包）：Opaque 不满足 IComparable
```

> 📜 **2026-09-25 之前这条 `where` 形同注释**：成员确实变可用，但**约束满足性从不校验** ——
> 声明期在「方法自己没有型参」时早退、调用点又对「where 的型参不在方法型参表里」静默跳过，
> 并注明「归声明期报」⇒ **两边各自以为对方会报，结果没人报**，`List<Opaque>().Sort()` 零诊断、
> 运行期崩 `VCall: function \`Opaque.CompareTo\` not found`。
> ⚠️ 受上文「方法级 `where` 不跨包」限制：该校验目前**只在同包生效**。

## 跨包约束是怎么传过来的

约束的载体是 **zbc `TYPE` 段的型参约束 bundle**——不是 zpkg 的 TSIG（该段已随 `drop-tsig-expt`
删除，`ExportedClassZ` 由 `TsigReconcile` 从 `TYPE` + `SIGS` 重建）。整条链路：

```
源码 where 子句
   │  ConstraintChecker.Resolve（包级 hoist，早于 per-file 并行段）
   ▼
SymbolTable.ClassConstraints          ← 键统一经 SymbolTable.ConstraintKey()
   │  ClassDescBuilder 直接复用这份分类（不从 AST 重推 → writer 与 checker 不会漂移）
   ▼
IrConstraintDesc[]  ──ZbcWriter──▶  zbc TYPE 段 bundle
                                        flags u8：bit0 class / bit1 struct / bit2 base
                                                  bit3 型参引用 / bit4 new() / bit5 enum
                                                  bit6 funcSig（尚未产出）
                                        载荷序：base → 型参引用 → iface_count + 名列表
   │  ZbcReader._readConstraintBundle
   ▼
IrClassDesc.TypeParamConstraints ──TsigReconcile──▶ ExportedClassZ.TypeParamConstraints
   │  ImportedSymbolLoader._constraintSetOf
   ▼
SymbolTable.ClassConstraints（导入侧 seed，local-wins）→ 与本包**同一个** _checkBundle
```

三点值得记住：

- **bit0–bit6 的 wire 布局早已规约**，三方 reader（Rust `type_reader.rs`、`ZbcReader`、
  `ZpkgReader._skipConstraintBundle`）一直按完整布局消费。所以接通跨包**没有格式 bump**——
  只是写端从「仅置 bit3」改成置全位。
- **键规则只有一处**：`SymbolTable.ConstraintKey(bareName, tpCount)`，规则与 `Classes` 相同
  （同短名多 arity 才带 `$N`）。写入 / 查询 / 导入三处都调它，否则 `Foo<T>` 与 `Foo<T,U>`
  的约束会互相覆盖。
- **local-wins 有守卫**：导入约束只在该键上的赢家确实是导入类时才 seed，避免本地同名类
  （可能压根没有 `where`）被别的包的约束污染。

## `Self` 类型（仅接口）

接口内可以写 `Self`，它指代**实现该接口的那个类型**：

```z42
interface IEq { bool Same(Self other); }          // 不必写成 IEq<T> where T : IEq<T>
interface IClone { Self Copy(); }
interface IBox<T> { T Get(); Self With(T v); }     // 与接口自己的型参共存

class Point : IEq {
    public int x;
    public bool Same(Point other) { return this.x == other.x; }   // Self 落地成 Point
}

class Bag<T> where T : IEq { }                     // 约束侧不必再写类型实参
```

**为什么加它**：全仓实测，真实存在的 `where` 约束 100% 是 `X<T> where T : Something<T>` 这种
F-bounded 自引用形态（`IEquatable<T>` / `IComparable<T>` / `INumber<T>`）。那个类型实参不携带
任何信息，纯粹是「我指我自己」的样板。`Self` 把它消掉。

**标准库已改写完毕**（change `apply-self-to-core-protocols`，2026-09-07）——三个协议接口
现在都是**非泛型 + `Self`**：

```z42
public interface IEquatable  { bool Equals(Self other); int GetHashCode(); }
public interface IComparable { int CompareTo(Self other); }
public interface INumber     { static abstract Self op_Add(Self a, Self b); … }

public struct Int32 : IComparable, IEquatable, INumber { … }          // 不再写 <int>
public class Dictionary<TKey, TValue> where TKey : IEquatable { … }   // 不再写 <TKey>
```

**代价（刻意接受）**：`class MyInt : IEquatable<int>`——让一个类型与**别的**类型比较——不再
可表达。该能力由两个专职接口承载，它们的 `T` 是被比较对象而非实现方自己，**保持泛型不变**：

| 形态 | 接口 | `T` 的含义 |
|---|---|---|
| 自比较（实例知道怎么比自己） | `IComparable` / `IEquatable` | 无型参；`Self` = 实现方 |
| 外部比较器（第三方知道怎么比两个） | `IComparer<T>` / `IEqualityComparer<T>` | 被比较对象 |

与 Rust 的 `Ord` / `PartialOrd`（`Self` 化）vs 显式 comparator 的分工一致。

**作用域限定为接口**（不进类）：类里写 `Self` 是未定义类型 `E0443`，与其它拼错的类型名同码。
这条边界是刻意的——`Self` 进类会牵出协变返回类型那一整块设计面，不在本轮范围。

### 实现模型（以及它带来的边界）

`Self` **不是一个新类型**：解析时它被当作接口的一个**隐式类型参数**（追加在接口自己的
`TypeParams` 之后），随后与真型参 `T` 走完全同一条路。这个选择让约束位、成员签名匹配、
方法派发键三处**零改动**。

直接后果，需要知道：

- **实现类不必也不能写 `Self`**：实现方在自己的签名里写具体类型（上例的 `Point`）。
  写错**会被抓**：`class Q : IEq` 若把 `bool Same(Self other)` 实现成 `Same(int)`，报
  **E0412**（change `add-interface-satisfaction-check`，2026-09-07——在此之前这里确实无人看守）。
- **经接口静态类型调用返回 `Self` 的方法，结果类型 = 该接口本身**（change
  `self-return-type-substitution`，2026-09-07）：`IClone c; var x = c.Copy();` 里 `x : IClone`。
  这是可靠**上界**——实现方必然实现该接口，所以把结果当接口用一定成立；但它**不是**具体类型，
  编译期在接口静态类型上也确实无从知道具体类是谁。要拿到具体类型，在**具体类**上调用
  （`Point p; p.Copy()` → `Point`，走实现方签名，不经过替换）。
  > 此前这里漏出的是型参 `Self` 本身（`x : Self`）——那个型参在调用方作用域没有意义，
  > 等于类型信息整个丢失。Rust 的对应做法是干脆禁止（`-> Self` 非 object-safe）；z42 选上界替换，
  > 因为 z42 接口没有 object-safety 概念，禁止会平白砍掉一类安全可用的写法。
  >
  > **`Self` 藏在 `Func<…>` 里也会被替换**（change `fix-iface-self-completeness-gaps` A3，
  > 2026-09-15）：`interface IMk { Func<Self,int> Make(); }` 经接口静态类型调用 `m.Make()` →
  > 结果 `Func<IMk,int>`（不再漏出裸 `Self`）。此前 `MemberResolver._substSelf` 只递归数组元素与
  > 泛型实参、**不下钻 `Z42FuncType`**，Self 原样漏给调用方；满足性校验侧的 `_substForIface` 同款漏、
  > 会把 `Apply(Func<Self,int>)` 判成与 `Apply(Func<C,int>)` 不匹配（假红 E0412）。两处都补了 Func 分支。
  > **接口索引器**（`Self this[int]`）的返回位走同一条替换（见
  > [属性与索引器 · 接口索引器](properties-indexers.md)）。
- 🔴 **形参位的 `Self` 不能经接口静态类型调用 —— 报 E0454**（change
  `bind-self-param-and-constraint-members`，2026-09-07）。返回位能取上界是因为它**协变**；
  形参位是**逆变**：接口只保证实参「也实现了该接口」，而实现方的签名要的是「它自己」。

  ```z42
  interface IEq { bool Same(Self other); }
  class P : IEq { public int V;    public bool Same(P other) { … } }
  class Q : IEq { public string S; public bool Same(Q other) { … } }

  IEq a = new P(1);
  IEq b = new Q("hello");
  a.Same(b);        // ❌ E0454
  ```

  > 这里**没有**可用的上界：把 `Self` 换成 `IEq` 一个字也拦不住上例 —— `P` 和 `Q` 都是 `IEq`。
  > 放行的后果是实测过的：派发到 `P.Same(P)`、`other.V` 从一个 `Q` 上读出 **null**，静默返回
  > `false`，全程无报错。故这里与 Rust 的 object-safety 同一选择：**禁止**，而不是假装收紧。

  **替代写法（推荐，也正是诊断消息给出的那条）**——把接口静态类型换成型参：

  ```z42
  bool eqVia<T>(T a, T b) where T : IEq { return a.Same(b); }   // ✅
  ```

  型参这条路上 `Self ≡ T` 是**精确**的（约束断言了运行期 `T` 就是那个实现类型），不是上界。
  同一接口上**没有** `Self` 形参的方法不受影响，照常经接口静态类型调用。
- **跨包**：`Self` 与型参 `T` 一样以裸字符串写进 zbc 接口方法签名块，导入侧还原成型参，
  **零格式改动**。

## 型参收者上的约束成员绑定

`where T : IColl` 之后，在 `T` 类型的值上调用 `IColl` 的成员，编译期会**按约束接口解析出真签名**
（change `bind-self-param-and-constraint-members`，2026-09-07）：

```z42
interface IColl { void Add(int x); int Size(); }

void f<T>(T a) where T : IColl {
    a.Add("nope");        // ❌ E0402：实参 string 不可隐式转 int
    var n = a.Size();     // n : int（不再是 <unknown>）
}
```

查找覆盖**方法级**（`f<T>() where T : I`）与**类级**（`class C<T> where T : I`）两个约束来源，
并沿**父接口闭包**递归。`Object` 的成员（`ToString` / `GetHashCode` / `Equals`）**优先**于约束
接口——这个顺序不能反，它决定派发键。

> **此前这里完全没有检查**：型参收者查不到 `Object` 成员就松绑成 `sig = null`，而实参检查的第一行
> 就是 `if (sig == null) return;` ⇒ 泛型代码里对约束接口方法的调用，实参一律不检查、返回类型一律
> `<unknown>`。`PriorityQueue` / `SortedSet` / `Dictionary` 走的正是这条路。

#### 属性与方法同口径（`check-bare-type-param-member-access`，2026-09-25）

约束提供的**属性**与**方法**在型参收者上现在走同一条解析路：

```z42
interface IHasName { string Name { get; }  string GetName(); }

string viaProp<T>(T a)   where T : IHasName { return a.Name; }      // → "Ada"
string viaMethod<T>(T a) where T : IHasName { return a.GetName(); } // → "Ada"
```

> 📜 **2026-09-25 之前 `a.Name` 静默返回 `null`**（链式 `a.Name.Length` 直接崩）：约束查找只补在
> 方法路，**属性 / 字段路从未补**，运行期把 `Name` 当字段读、读不到 ⇒ `null`。
> 同一约束的 `a.GetName()` 则一直正确 —— 这个不对称是那次修复的根因。

#### 成员名压根不存在 → `E0401`

型参收者上访问一个**任何已知类型都没有声明过**的成员名，编译期报 **E0401**：

```z42
T f<T>(T a) { return a.Bogus; }     // ❌ E0401：no field or property `Bogus` on type parameter `T`,
                                    //    and no known type declares that name
T g<T>(T a) { return a.NoSuch(); }  // ❌ E0401（方法形态同款）
```

**判据刻意收窄到「全仓无此名字」**，不是「不由约束提供就报」。后者会误报今天完全正常的写法：

```z42
T Max<T>(T a, T b) where T : IComparable { … }
var m = Max(numA, numB);   // Num 是 class
m.value                    // ✅ 正常 —— 引用类型经擦除返回位流出时运行期派发良好
```

⚠️ 仍有一格**编译期不报、运行期才崩**：**blob struct**（字段数 ≥ 2 的值 struct）经擦除返回位
流出后访问字段（`id(v).X` → `FieldGet: expected object, got BoxedStruct`）。该格的根因是泛型
**特化**而非成员解析，见 [internals / 泛型](https://z42-lang.github.io/z42/internals/compiler/generics.html)。

### `Self` 形参位：具体类型实参报 E0463

```z42
interface IEq { bool Same(Self other); }

bool bad<T>(T a)      where T : IEq { return a.Same("nope"); }   // ❌ E0463：string 不可赋给型参 T
bool ok<T>(T a, T b)  where T : IEq { return a.Same(b); }        // ✅ 实参也是 T（Self ≡ T）
```

`Self` 经 `_substSelfSig` 精确替换成型参 `T` 后，形参类型是**裸型参**。隐式转换判定里有一条
「恰一侧含泛型形参 → 擦除放行」的通用规则（服务 `T → object` / `T → 接口` 这类合法上转），
对「具体类型实参 → 裸型参 `T` 形参」这个方向**也会擦除**——于是 `a.Same("nope")` 一度静默放行，
运行期派发到 `T.Same` 后从 `string` 上读出不存在的字段（静默错值 / 崩）。

**change `check-constraint-iface-method-args`（2026-09-13）补了一道方向敏感的严格检查**：型参收者上
调约束接口方法时，`Self` 形参位（含藏在 `Func<Self,…>` 里的）若传入**具体类型**实参（完全不含型参、
非 error/unknown）→ 报 **E0463**。实参若也是型参（`T` → `T` 走 Identity、`U` → `T` 报 E0402）或
error/unknown（吸收、不级联）→ 不受影响。**只收「目标裸型参 + 源无具体」这一个方向，不碰通用擦除
规则**（`T → object` 等照旧放行），也**不改发射**（纯诊断、零字节漂移、无格式 bump）。

> **与 [E0454](#self-类型仅接口) 的分工**：E0454 管**接口静态类型**收者（`IEq a, b; a.Same(b)`）——
> `Self` 形参在那里是逆变、无唯一安全上界，只能一刀切**禁止**；E0463 管**型参收者**（`where T : IEq`
> 的 `T`）——那条路 `Self ≡ T` 精确（约束断言运行期 `T` 即实现类型），所以可以**真查实参**而非禁止。
>
> Deferred `tighten-bare-type-param-target-erasure` 的「型参收者」那半由本 change 关闭；其**通用**
> 擦除收紧（任意裸型参目标位，需区分作用域内不透明型参 vs 待推断型参、要给 `Z42GenericParamType`
> 加 owner）仍开着——那是对通用规则动刀、爆炸半径另算。
>
> 📌 **2026-09-25 补记**：`check-bare-type-param-member-access` 关掉了「**收者位**的成员名不存在」
> 这一格（上文 E0401）。它**没有**动目标位，也**没有**给 `Z42GenericParamType` 加 owner ——
> 区分两类型参本来要靠 owner，但收窄后（只报「全仓无此名字」）两种修法都不适用，该区分不再需要。
> 剩下两条独立后续：**按字面全面执行成员可用性规则**（`enforce-bare-type-param-member-rule`，
> 开工前需先裁决 `var m = genericCall(); m.X` 是否接受判红）、以及**方法级 `where` 跨包导出**
> （`export-method-level-wheres`，见下）。

## 运算符如何在型参上派发

`where T : INumber` 让泛型代码直接写 `a + b`，而不必写 `a.op_Add(b)`：

```z42
T Sum<T>(T a, T b) where T : INumber { return a + b; }
```

绑定路径（`ExprTyper._bindBinary`）：左操作数是 `Z42GenericParamType` → 到该型参的 where 约束
接口里找 `static abstract op_Add`（沿父接口链找；方法级与类级约束都查）→ 发**接收者驱动的
VCall**（`vcall a.op_Add(b)`），运行期由 `a` 的具体类决定跑哪个实现。与手写 `a.op_Add(b)`
（`generic_inumber.z42` 的写法）**发同一条指令**，只是省掉了显式方法名。

两条必须知道的规则：

- **结果类型恒为 `T`**（即左操作数那个型参，**不是**接口声明里的 `Self`）。依据是协议本身——
  `INumber` 抬头写明「Mixed-type arithmetic is not supported（Self + Self → Self only）」。
  这条不是可选的：`a + b + c` 的第二个 `+` 需要左侧仍是型参才能
  再次落回约束派发，否则退化成裸算术。（不能改读接口方法的声明返回类型：`INumber` 是**导入**
  接口，其签名经 `ImportedSymbolLoader` 还原后返回类型已不是型参形态。）
- **实现方必须写 `static override`**：`public static override T op_Add(T a, T b)`。只写 `static`
  的方法注册到另一个键，运行期会 `VCall: function X.op_Add not found`。

> **历史（`fix-generic-operator-constraint-dispatch`）**：这条路径**一度整个不存在**。
> `_bindBinary` 的运算符重载分支要求 `lt is Z42ClassType`，型参不匹配 → 落到 `BinaryTypeTable`，
> 类型检查报「operator `+` requires numeric operand, got `T`」，而 emitter 照发**裸 `add i32`**。
> `int` / `double` 的用例之所以一直绿，纯粹因为解释器的 `add` 对 `Value` 动态派发；换成用户
> struct 就是拿 blob 去做整数加法。约束**从未被读过**——删掉整条 where 子句，诊断逐字节相同。
> `static_abstract_operator.z42` 的抬头注释当时已经把这条路径描述得一清二楚，但那是**设计意图**
> 而非现状。这正是 `--emit-zbc` 吞诊断能掩盖的那类缺陷：binder 报的错没人看见，emitter 那半边
> 碰巧能跑，测试就绿。

### 实现接口静态成员：四项校验（含跨包）

接口成员可以是 `static abstract`（`INumber` 的五个 `op_X` 就是）。实现方**必须**写
`public static override`，且满足性校验会逐项比对——**其中 static 位与返回类型曾经是真洞**，
`MangleKey` 只比「名 + 形参」，两者都能静默通过：

| 比对项 | 口径 | 不符 |
|---|---|---|
| 名 + 形参 | `MangleKey`（`String[]≡string[]`、`Int32≡int`、数组叶子 keyword 化） | 不算同一成员，继续找下一个重载 |
| **static / instance** | 接口声明是 `static` 的，实现也必须是 `static`（反之亦然） | **E0412** |
| **可见性** | 必须显式 `public` —— 类成员**无修饰默认 private**，`int M(){…}` 同样被拦 | **E0412** |
| **返回类型** | `TypeKey` 归一相等，或**协变**（子类 / 实现该接口）。数值拓宽 `int→long` **不**放行 | **E0412** |

```z42
interface INum2 { static abstract int MakeZero(); }

struct Good : INum2 { public static override int MakeZero() { return 0; } }   // ✅
struct Bad  : INum2 { public int MakeZero() { return 0; } }                   // ❌ E0412：接口里是 static，这里是实例方法
struct Priv : INum2 { static override int MakeZero() { return 0; } }          // ❌ E0412：默认 private，必须写 public
```

发射点：`src/compiler/z42c.semantics/src/InheritanceResolver.z42:441`（static）/ `:456`（可见性）
/ `:478`（返回类型）。

**跨包同口径**。接口方法的 static 位随 **zbc 1.41** 的接口方法块 `is_static:u8` 过 wire，
导入侧还原真值，因此**导入接口**与本包接口接受完全相同的四项校验。在此之前 static 位从 wire
丢失（恒 false），校验对导入接口整个跳过 ⇒ 跨包把 `static abstract` 成员实现成实例方法是**静默
放行**的。回归门：`src/tests/cross-zpkg/iface_static_impl_mismatch/`（负例 fixture，期望 build
error 含 `is \`static\` in the interface and an instance method here`）。

> ✅ 齐备性也校验（自 `fix-iface-satisfaction-gaps` / `fix-iface-self-completeness-gaps`）：声明了接口
> 就必须实现它的**每个**成员——缺成员编译期报 **E0412** `\`C\` implements \`I\` but does not define
> member \`X\``。属性 `{ get; set; }` 的 `get_X`/`set_X`、索引器的 `get_Item`/`set_Item` 各是**独立契约**
> （满足性键含成员名），缺任一半都报。[关联类型](#关联类型type-item)另走 E0453（必须绑齐）。
> 本节上面讲的是「成员在、但**签名对不对**」那一层（static / 可见性 / 返回类型）；「成员**在不在**」由
> E0412 缺成员分支兜。
>
> ✅ 跨包也覆盖（`add-crosspkg-interface-accessors`）：接口的属性 / 索引器访问器（get_X/set_X/get_Item/
> set_Item）作为普通方法进 TYPE record 过 wire，导入侧 `ImportedSymbolLoader` 恢复进 `it.Methods`——**无需
> 格式 bump**（走既有通用方法块）。跨包接口属性/索引器的满足性与经接口读写与本包同口径。

## 关联类型（`type Item;`）

接口可以声明一个**由实现方决定**的类型，约束侧再要求它绑到具体类型：

```z42
interface IEnum {
    type Item;                       // 由实现方决定
}

class IntBag : IEnum { type Item = int; }
class StrBag : IEnum { type Item = string; }

class Use<T> where T : IEnum<Item = int> { }

new Use<IntBag>()   // ✅
new Use<StrBag>()   // ❌ E0453：binds `Item` to `string`, but `int` is required
```

这是关联类型相对 [已知限制 §1](#1-接口约束只比裸名不校验类型实参) 的「接口只比裸名」真正多出来的
**判别力**：裸名匹配下 `IntBag` 与 `StrBag` 都只是「实现了 IEnum」，无法区分。

### 规则

- **绑定是实现方的事**：接口里写 `type Item = int;` 报错；类里写不带绑定的 `type Item;` 也报错。
- **必须绑齐**：实现了带关联类型的接口就得给出绑定，走**接口继承闭包**（父接口的关联类型同样要绑）。
  缺绑定报 **E0453**——与接口方法/属性齐备性的 **E0412**（见上文「实现接口静态成员」一节）同族：两者都是
  「声明了接口就必须补齐某成员」的编译期强制。关联类型走独立码 E0453 而非 E0412，是因为不绑则**根本无法
  参与约束匹配**（方法缺失至少还能在运行期以 `VCall not found` 暴露，关联类型缺绑连约束求解都无从进行）。
- **绑定显式声明，不推断**：`type Item = int;`，而不是从方法签名反推。推断需要跨成员的统一算法
  （且要处理 F-bounded 递归），代价与收益不成比例。
- 全部相关诊断都是 **`E0453`**。

### 语法边界（两处刻意的取舍）

**`type` 不是关键字。** 全仓大量把 `type` 当变量名 / 参数名 / 属性名用，把它加进 lexer 会一次性
废掉那些源码。所以它是**上下文关键字**：靠「`type` + 标识符 + `;`/`=`」三 token 前瞻拦截。
代价是类型名恰好叫 `type` 的**字段声明**（`type x;`）会被当成关联类型——已实测全仓零命中，
接受。方法与属性不受影响（第三个 token 是 `(` / `{`）。

**`Name = Type` 只在 `where` 约束位可写。** 普通类型位 `List<Item = int>` 仍是语法错误。
`=` 在类型位没有别的含义，一旦全局放开就再也收不回来了。

## 已知限制（诚实标注）

这些不是 bug，是当前实现的**明确边界**。踩到时不要以为约束在保护你。

### 1. 接口约束只比裸名，不校验类型实参

`where T : IFoo<T>` 只检查「T 实现了名为 `IFoo` 的接口」，**不检查实参是否是 T 自己**。
故 `class Foo : IFoo<string>` 也能满足 `where T : IFoo<T>`。

这与运行期行为一致（它拿到的同样是常量池里的裸名），故两边不产生分歧。裸名匹配还顺带
消掉了 F-bounded 自引用朴素展开会无限递归的问题。
Deferred：`where-constraint-future-type-arg-matching`。

> [`Self`](#self-类型仅接口) 给了一条**绕开**这个限制的写法（`where T : IEq` 根本不写类型实参，
> 就没有实参可以写错）。标准库的三个协议接口已于 `apply-self-to-core-protocols` 全部改写成
> `Self`，**它们自己不再踩这个坑**。但这条 Deferred **仍然开着**——`Self` 是绕开、不是消除：
> 任何**其它**带类型实参的接口约束（`IEnumerable<T>` / `IComparer<T>` / 用户自定义泛型接口）
> 今天照旧按裸名匹配。

### ~~2. 方法级约束只在显式写类型实参时校验~~ ✅ 已解决

**2026-09-08（change `add-generic-type-arg-inference`）**：`Max(a, b)` 现在也校验 —— 从实参
结构化 unify 出型参绑定后，复用**同一条** `ConstraintChecker.CheckMethod` 路径。

**残留边界**（推断失败即完全按改动前行为、不发任何诊断）：型参未被任何形参位覆盖 /
同一型参绑到不同类型（v1 不做「最佳公共类型」，`Max(1, 2L)` 仍不校验）/ 实参是 lambda 或
target-typed `new` 这类延迟位 / `params` 尾位。Deferred：`generic-inference-best-common-type`、
`generic-inference-lambda-args`。

### ~~3. 顶层函数的 `where` 不校验~~ 🔴 **这条是过期断言，已订正**

**2026-09-10（change `validate-func-type-constraint`）实测**：顶层泛型函数的 `where`
**声明期与调用点两半都跑**——顶层自由函数的调用同样经 `MemberResolver` 走到
`ConstraintChecker.CheckMethod`：

```
probe_toplevel.z42(9,5):  E0402: type argument `Plain` for `T` does not satisfy constraint `enum` on `TakesEnum`
probe_toplevel.z42(10,5): E0402: type argument `NeedsArg` for `T` does not satisfy constraint `new()` on `TakesNew`
```

Deferred `where-constraint-future-toplevel-func` 随之关闭。
⭐ 又一条「没有东西盯着的断言迟早会烂」：这句话写下时也许为真，但没有任何用例钉住它，
后来 `add-generic-methods` / `add-generic-type-arg-inference` 把顶层函数接进同一条路径，
文档却没人改。

### ~~4. 函数类型约束从未发出诊断~~ ✅ 已解决（2026-09-10 `validate-func-type-constraint`）

判定分三步：

1. 约束里的型参按**本次调用已解析的类型实参**代换（`where T : Predicate<U>` + `U=int`
   ⇒ 要求 `Predicate<int>`）。代换不掉的位（类级型参、`Self`）当**通配**放行。
2. 实参必须是函数类型、且 arity 相同。
3. 逐位比：形参位**逆变**（约束形参可传给实参形参 ⇒ 实参形参更宽）、返回位**协变**。
   类类型走子类判定，其余按规范名相等；**数值拓宽刻意不放行**（间接调用按槽位传值，
   `Func<int,long>` 与 `Func<int,int>` 不是一回事）。

```z42
void Run<T>(T handler, int x) where T : Action<int> { handler(x); }

Func<string,int> g = s => 1;
Run(g, 3);   // E0422: … parameter 1 is `String`, the constraint requires it to accept `Int32`
```

> 🔴 **修前不是「宽松」，是有运行期后果的静默错**：上面这段修前**编译干净**，运行期
> `uncaught exception: VCall: expected object, got I64(3)` —— `int 3` 被直接喂进 `string` 形参。
> 换成字符串拼接则**连崩都不崩**，静默打印 `got: [3]`，错值一路流下去。
> 根源是「约束只被相信、从不被检查」：binder（`MemberResolver` 的 `Z42GenericParamType` 分支）
> 按这条签名把 `handler(x)` 绑成 `CallIndirect` 并推断结果类型，而没有任何一处核对实参真是这个签名。

**残留边界**（都是「漏报」，不会假红）：

| 边界 | 为什么 |
|------|--------|
| **跨包 class 级 func 约束不校验** | zbc 约束 bundle 的 flag 位没有 func 签名槽（要双格式 bump）；导入 bundle 恒无 func 约束 ⇒ 天然跳过。与关联类型跨包同一取舍 |
| **推断失败的调用点完全不校验** | `R Apply<T,R>(T f, int x) where T : Func<int,R>` 的 `R` 不出现在任何形参位 ⇒ `TypeArgInference` 边界 ①（任一型参未绑定即整体失败）⇒ `CheckMethod` 根本不跑 |
| **实参是 lambda / target-typed `new`** | 延迟位类型是 Unknown ⇒ 推断跳过 ⇒ 同上 |
| **`E0423` 在从不被调用的泛型方法上不报** | 方法级 `where` 只在调用点被处理，见下「### 5」 |

> 🔴 **前史：它一度比「不校验」更糟——`where T : Action<int>` 直接编不过**
> （2026-09-10 `fix-func-constraint-reported-unknown` 修）。`complete-where-constraints` 给
> `_fillBundle` 加的「约束名拼错了」分支（`E0443 unknown constraint type`）把函数类型也网了进去——
> `Action` / `Func` / `Predicate` / 用户 `delegate` 经 `SymbolTable.ResolveTypeP` 解析成结构化
> `Z42FuncType`，**不进 `Classes` / `Interfaces` 表**，于是「是型参？是接口？是类？」三问全否，
> 掉进最后那条 else。修法是在报错前先认出函数类型（`ResolveTypeP(...) is Z42FuncType`）。
>
> **为什么当年的探针没抓到**：那条 error 落地时以 warning 跑全仓实测「0 条」，但三个受害文件
> （`src/tests/generics/func_constraint_{action,predicate,captured}.z42`）全走 `z42c --emit-zbc`，
> 而那条路径当时**丢弃全部诊断**。探针看不见的地方，"0 条" 不构成证据。
>
> **当时为什么没顺手补校验、后来怎么补的**：`func_constraint_captured.z42` 里有
> `R Apply<T, R>(T f, int x) where T : Func<int, R>` —— 约束类型里含**另一个型参 `R`**。
> 当时判断「得把约束里的型参当通配去 unify」，属独立一件事。实际落地时发现**不需要 unify**：
> 调用点已经有一份解析好的类型实参（`CheckMethod` 的 `args`），直接用
> `MethodTypeArgSubst.ByName` 把约束里的型参代换掉即可，代换不掉的位当通配。
> 而 `Apply<T,R>` 那条根本走不到校验（`R` 推不出 ⇒ 推断整体失败）。

> **事实校正（`fix-generic-func-param-indirect-call`）**：本节原写着「代码生成依赖该约束把参数当
> func 值走间接调用，改动需谨慎」——**不成立**。`CallEmitter` 从不看约束，它只查
> `Locals.ContainsKey(名字)`。真正决定 `f(x)` 走间接调用的是 binder（`MemberResolver` 的
> `Z42GenericParamType` 分支），而那条分支**一度不存在**：binder 把 `f(x)` 绑成自由函数调用并报
> E0401，只是诊断被 `--emit-zbc` 吞了，而 emitter 靠名字侥幸补救，于是直接调用能跑。
> 名字一旦不在当前帧 Locals 里（**被 lambda 捕获**）侥幸就没了——不发 `mk_clos`、lambda 体里发
> `call @f` 调一个不存在的自由函数，运行期 `undefined function`。
> 回归守卫：`src/tests/generics/func_constraint_captured.z42`。

### ~~5. 方法级 `where` 的声明级诊断：按调用次数重复，零调用时完全消失~~ ✅ 已解决

**2026-09-11（change `resolve-method-where-at-decl`）**：**声明级**诊断
（`E0401` 未知型参 / `E0443` 未知约束名 / `E0402` class·struct 互斥 / `E0423` func 约束并置 /
`E0453` 关联类型绑定名笔误）现在一律在**声明期**（Pass 0.5 `Resolve`）发出，且只发一次。

修前四个症状（都实测复现过）：

| # | 症状 | 修后 |
|---|------|------|
| ① | 类级声明诊断在**单文件路径**报两遍 | 1 条 |
| ② | 方法级被调用 N 次 → 报 N 条 | 1 条 |
| ③ | **从不被调用的泛型方法一条都不报** | 照报 |
| ④ | `where U : IFoo`（U 不是该方法的型参）方法级**完全静默** | 报 `E0401`（与类级同款） |

- ①的根因是**两次 `Resolve`**：`IrDump.BuildModuleD` 显式调一次，紧接着 `Infer(cu, symbols)` 的
  `resolveConstraints` 默认 true 又调一次。删掉前者，两条路（单文件 / 包并行）各有且只有一个 `Resolve`。
- ②③④的修法：`Resolve` 除了走 `ClassDecl` 的 where，还走**类成员方法**与**顶层自由函数**
  （`MethodDecl.IsFree`）的 where，只为发诊断、bundle 丢弃；`CheckMethod` 照旧在调用点即席建 bundle，
  但**不再报**声明级诊断（`_fillBundle(..., report: false)`）。
- **不缓存声明期算出的 bundle**：方法级约束没有一把稳定的键（同名同 arity 的重载各有各的 where），
  造一张键不可靠的缓存表比即席重建（约束项个位数）危险得多。拆的是「报诊断」与「建 bundle」，不是加缓存。

> **违反约束**（`E0402` 实参不满足 / `E0422` 函数签名不符）**仍然每个调用点各报一条**——那本来就是
> per-call-site 的事实，不在本次搬动范围内。

### 6. 关联类型：同包 + 跨包均已实现（`assoc-type-crosspkg`，zbc 1.42）

同包（见上「关联类型」一节）与**跨包**都已完整校验。跨包由三份数据经 wire 承载打通：
① 接口关联类型名单（`type Item;`）+ ② 类侧绑定（`type Item = int;`）→ zbc TYPE 记录尾部**统一 assoc 块**
（接口写 `(Item,"")`、类写 `(Item,int)`）；③ 约束绑定（`where T:IEnum<Item=int>`）→ 约束 bundle **bit7**。
`TsigReconcile`/`ImportedSymbolLoader` 恢复这三份数据后，`add-associated-types` PR-3 曾为「跨包 wire 未到位」
而加的三处 `IsImported` 守卫（约束声明处绑定名合法性、使用点绑定匹配、实现方补齐强制）全部删除——跨包
导入类/接口与同包一样接受完整校验：绑定名不是该接口的关联类型 / 未绑定 / 绑定类型与要求不符，均报 E0453。
关联类型是纯编译期概念，runtime 只消费这两处新载荷保游标对齐（`validate_type_arg_constraint` 无关联类型分支）。

嵌套约束（`where T : IIterator<Item = U>, U : IDisplay` 里 `U` 再被约束）仍未实现。

## 为什么这些约束曾经集体失效

一段值得记住的历史，也是本页存在的理由。

`where T : IFoo` 曾长期**写了等于没写**：不报错、也不校验。比「不支持」更糟——不支持会报错，
假实现让使用者以为拿到了类型保护。三层叠加造成：

1. **编译期只认 4/7 项**，其余静默延后。更早的一刀是：约束填充有个「类型实参个数为 0」的
   前置条件，而真实世界的接口约束绝大多数是泛型接口（`IEquatable<T>` / `IComparable<T>` /
   `INumber<T>`）——它们在接口判定**之前**就被整条丢弃了。
2. **zbc writer 只写一个 flag 位**，把运行期那份完整的七项校验饿成了死代码。
3. **没有任何门能发现前两层**：负例（期望编译报错）语料原在 `src/tests/errors/`，
   2026-05-12 搬进 C# 测试项目，2026-06-26 C# 编译器移除时随整个测试项目一起蒸发。
   自举迁移只搬了「能编过」的正例。

第 3 层是根因：**没有测试盯着的约定迟早会烂**，而一句自洽的注释能让它烂得毫无声息。
今天这些语义由 `src/compiler/z42c.semantics/tests/typecheck/constraint_tests.z42` 的负例
用例守着——那是 `where` 约束第一次有门盯着。

**这个故事在 2026-09-06 又演了一遍，值得写下来**：给 `Self` 补的头一批用例**全是空测试**——
把 `Self` 的支持改动整个退回，7 条断言仍然 7/7 全绿。根因同样是「一句自洽的说明掩盖了一个洞」：
`SemanticDump` 只收 `TypeChecker` 的诊断、把 `SymbolCollector` 那份整个丢掉，而**声明签名位置**
（方法返回类型 / 形参类型 / 字段类型 / 基类表）的未定义类型 `E0443` 恰恰由 collector 发出
⇒ 凡以「签名位置写坏类型应当报错」为断言的用例，无论实现对错都恒绿。

两条教训：**新写的负例必须做「退回对照」才算门**（不会变红的断言不是门）；
**诊断只要在链路上被丢过一次，它守的所有约定都会跟着静默腐坏**。该洞已修
（`SemanticDump._model` 合并两份诊断），并由 `typecheck_tests.z42` 的
`test_undefined_type_in_declaration_signature_positions_reported` 盯着。

## 相关

- [泛型方法](generic-methods.md) —— 方法级类型参数与 `<` 歧义消解
- change [`complete-where-constraints`](../../../spec/archive/2026-09-05-complete-where-constraints/proposal.md) —— 本页所述行为的引入过程（含三层塌陷的完整定位）
