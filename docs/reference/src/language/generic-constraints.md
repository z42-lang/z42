# 泛型约束（`where` 子句）

> 本页是**泛型约束语义与校验范围的 SoT**。泛型的整体设计（代码共享策略、reified 类型、
> 跨 zpkg 元数据）见[泛型的实现](https://z42-lang.github.io/z42/internals/compiler/generics.html)；
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
| 型参引用 | `where U : T` | U 的实参可赋给 T 的实参：同名 / **子类** / **实现该接口**（含接口继承链）/ 数值拓宽 | ✅ ⚠️ 见下「型参引用的可赋范围」 |
| 函数类型 | `where T : Func<int, R>` | T 是函数类型，且 arity 相同、**形参逆变 / 返回协变**地匹配 | ✅ **E0422**（签名不符）/ **E0423**（与其它约束并置）；⚠️ 仅编译期、仅本包声明的约束，见下 |

`class` 与 `struct` 同时出现在一个型参上 → 报错（互斥）。函数类型约束与**其余任何**约束
并置也报错（`E0423`）——见下「函数类型约束」。

### 型参引用的可赋范围

`where U : T` 判的是「U 的实参**可赋给** T 的实参」。可赋的四格：

```z42
interface IBase { }
interface IDerived : IBase { }
class C : IBase { }
class B { }
class S : B { }

class Pair<T, U> where U : T { }

Pair<C, C>              // ✅ 同名
Pair<B, S>              // ✅ S 是 B 的子类
Pair<IBase, C>          // ✅ C 实现 IBase
Pair<IBase, IDerived>   // ✅ 接口继承链：IDerived 是 IBase 的子类型
Pair<IDerived, IBase>   // ❌ 方向反了 —— IBase 不是 IDerived 的子类型
```

> ⚠️ **上界是「实例化泛型」的那格当前不满足**：`where U : T` 里 T 的实参写成
> `Box<int>` 时，即便 U 的实参是 `Box<int>` 的子类也判不满足（只有**同名**那条早退能过）。
> 这是已知缺口，不是设计：判对它需要连类型实参一起比（否则
> `Pair<Box<int>, Box<string>>` 会被静默放行 —— 那比误报更坏）。
>
> 基元实参（`Pair<IBase, int>`）走的是与直接接口约束（`where T : IBase` + `Box<int>`）
> **同一个出口**（`ConstraintChecker._satisfiesInterface`），两条路的判定一致 —— 本行没有
> 自己特有的基元规则。

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

### `new T()` 不会去调一个它满足不了的构造器

`T` 只有到运行期才知道，所以「有没有能用的无参构造器」这个判断必须**在运行期再做一遍**
—— 非泛型那条路早就在编译期做了（`new V2()` 报 `E0426`），泛型这条做不到。

```z42
struct V2 { public int X; public int Y; public V2(int x, int y) { … } }   // 只有带参 ctor
T mk<T>() where T : struct { return new T(); }
mk<V2>()        // → 零值 {0, 0}（对齐 C#：struct 总有隐式无参构造）
```

**有无参构造器就一定会跑它**，两者不冲突：

```z42
struct S3 { public int A; public S3() { this.A = 5; } }
mk<S3>().A      // → 5
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

**基元没有任何构造器，所以带实参一律报 E0426**：

```z42
int x = new int(5);        // ✗ E0426: `new int()` takes no arguments — a primitive has no constructor
string s = new string(cs); // ✗ E0426（提示改用 `String.FromChars(cs)`）
```

## 校验发生在哪里

| 时机 | 位置 | 报什么 |
|------|------|--------|
| 声明期 | 每个泛型类 / **接口**的 `where` 解析成约束集；**类成员方法与顶层自由函数的 `where` 也在此过一遍**（只发诊断，方法级不预登记符号表） | 未知型参 `E0401`、`class`/`struct` 互斥 `E0402`、未知约束名 `E0443`、func 约束并置 `E0423`、关联类型绑定名笔误 `E0453` —— 每条**只发一次**，与是否被实例化 / 被调用无关 |
| 类型引用位（use-site） | **凡是写出一个受约束泛型类型实例化的地方**：<br>· **体内位**：`new Box<D>()`、局部变量声明、`cast`/`as`、`is`、`default(T)`、`typeof(T)`、catch<br>· **声明位**：字段 / 属性 / 索引器类型、方法（含自由函数）形参·返回类型、基类·接口列表<br>· **嵌套**：`Wrap<Box<D>>` 逐层下钻，内层约束不因外层无约束而逃逸 | 违反约束 `E0402`，Span 指向该类型引用处 |
| 方法调用点 | `obj.m<T>(...)` / `C.m<T>(...)`（显式写类型实参）**及 `m(...)`（推断成功时）**；顶层自由函数同样走这条 | 违反约束 `E0402`、**函数类型签名不符 `E0422`** |
| 方法调用点（**细化类级型参**）| `list.Sort()` —— 方法的 `where` 约束的是**所属类**的型参（见下「方法级 `where` 细化类级型参」）；按**收者的**类型实参校验 | 违反约束 `E0402` |

> 调用点只报**违反**（`E0402` / `E0422`）——那本来就是 per-call-site 的事实，同一个方法被调 3 次
> 传 3 个不合格实参就该报 3 条。**声明级**诊断（约束名写错、并置非法等）全部在
> 声明期，见「已知限制 4」。

> **类型引用位的实现分两条 choke point**：体内位由绑定期的
> `TypeChecker._chkTypeRef` 覆盖（access / 弃用 / 跨包重复诊断都挂在这，约束校验并入）；声明位由
> `ConstraintChecker.CheckDeclTypeRefs` 覆盖——它必须放绑定期，因为声明位的收集期入口
> `SymbolCollector._chkTypeRefT` 在 `CollectAll` 时跑、约束尚未 `Resolve`。嵌套下钻由
> `ConstraintChecker.Check` 开头的**无条件递归**实现（在 `HasConstraints` 早退之前，否则外层无约束的
> `Wrap<Box<D>>` 会让内层 `Box<D>` 逃逸）。纯诊断、不回灌发射 ⇒ 无格式 bump、自举字节不动。

诊断都携带真实 Span：约束声明错误指向 `where` 所在行，违反错误指向实例化 / 调用处。

**本包与跨包同口径**：导入**类型**的约束走同一个校验函数，判定规则完全一致（见下节）。

🔴 **例外：方法级 `where` 不跨包**。TSIG 不导出方法级约束，
故**导入**的泛型方法 / 自由函数，其 `where` 在调用点**不被校验**：

```z42
// Array.Sort<T> 声明了 where T : IComparable，但它来自 z42.core（导入）
Array.Sort<Opaque>(arr);   // ⚠️ 跨包无诊断（同包写同样的代码则报 E0402）
```

**类级**约束不受影响（有 TSIG 通道，跨包正常报 E0402）。
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
   │  ConstraintCodec.Read（z42.package ZpkgWire.z42：三个读者共用）
   ▼
IrClassDesc.TypeParamConstraints ──TsigReconcile──▶ ExportedClassZ.TypeParamConstraints
   │  ImportedSymbolLoader._constraintSetOf
   ▼
SymbolTable.ClassConstraints（导入侧 seed，local-wins）→ 与本包**同一个** _checkBundle
```

三点值得记住：

- **bit0–bit6 的 wire 布局早已规约**，reader（Rust `type_reader.rs`，以及 z42 侧 `.zbc` / `.zpkg` /
  `.zsym` 三个读者共用的 `ConstraintCodec.Read`）一直按完整布局消费。所以接通跨包**没有格式 bump**——
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

**标准库的三个协议接口**都是**非泛型 + `Self`**：

```z42
public interface IEquatable  { bool Equals(Self other); int GetHashCode(); }
public interface IComparable { int CompareTo(Self other); }
public interface INumber     { static abstract Self op_Add(Self a, Self b); … }

public struct Int32 : IComparable, IEquatable, INumber { … }          // 不写 <int>
public class Dictionary<TKey, TValue> where TKey : IEquatable { … }   // 不写 <TKey>
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
  **E0412**。
- **经接口静态类型调用返回 `Self` 的方法，结果类型 = 该接口本身**：`IClone c; var x = c.Copy();` 里 `x : IClone`。
  这是可靠**上界**——实现方必然实现该接口，所以把结果当接口用一定成立；但它**不是**具体类型，
  编译期在接口静态类型上也确实无从知道具体类是谁。要拿到具体类型，在**具体类**上调用
  （`Point p; p.Copy()` → `Point`，走实现方签名，不经过替换）。
  > 若漏出型参 `Self` 本身（`x : Self`）——那个型参在调用方作用域没有意义，
  > 等于类型信息整个丢失。Rust 的对应做法是干脆禁止（`-> Self` 非 object-safe）；z42 选上界替换，
  > 因为 z42 接口没有 object-safety 概念，禁止会平白砍掉一类安全可用的写法。
  >
  > **`Self` 藏在 `Func<…>` 里也会被替换**：`interface IMk { Func<Self,int> Make(); }` 经接口静态类型调用 `m.Make()` →
  > 结果 `Func<IMk,int>`（不会漏出裸 `Self`）。`MemberResolver._substSelf` 与满足性校验侧的 `_substForIface`
  > 都会下钻 `Z42FuncType`（否则 `Apply(Func<Self,int>)` 会被判成与 `Apply(Func<C,int>)` 不匹配，假红 E0412）。
  > **接口索引器**（`Self this[int]`）的返回位走同一条替换（见
  > [属性与索引器 · 接口索引器](properties-indexers.md)）。
- 🔴 **形参位的 `Self` 不能经接口静态类型调用 —— 报 E0454**。返回位能取上界是因为它**协变**；
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
：

```z42
interface IColl { void Add(int x); int Size(); }

void f<T>(T a) where T : IColl {
    a.Add("nope");        // ❌ E0402：实参 string 不可隐式转 int
    var n = a.Size();     // n : int（不是 <unknown>）
}
```

查找覆盖**方法级**（`f<T>() where T : I`）与**类级**（`class C<T> where T : I`）两个约束来源，
并沿**父接口闭包**递归。`Object` 的成员（`ToString` / `GetHashCode` / `Equals`）**优先**于约束
接口——这个顺序不能反，它决定派发键。

#### 属性与方法同口径

约束提供的**属性**与**方法**在型参收者上走同一条解析路：

```z42
interface IHasName { string Name { get; }  string GetName(); }

string viaProp<T>(T a)   where T : IHasName { return a.Name; }      // → "Ada"
string viaMethod<T>(T a) where T : IHasName { return a.GetName(); } // → "Ada"
```

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

**blob struct**（字段数 ≥ 2 的值 struct）经擦除返回位流出后访问字段同样正常：

```z42
struct Vec2 { public long X; public long Y; }
T id<T>(T a) { return a; }

id(v).X        // ✅ 7 —— 基元 / 引用 / bool / 嵌套 struct 四种叶子皆可
id(v).Sum()    // ✅ 方法一直可以
```

> 已知限制：`id(v).X = 5`（**写**进从擦除返回位流出的临时盒）崩 `FieldSet: expected object` ——
> 那应当是**编译错误**（写入必然被丢弃），已登记 `reject-assign-to-erased-call-result`。

### `Self` 形参位：具体类型实参报 E0463

```z42
interface IEq { bool Same(Self other); }

bool bad<T>(T a)      where T : IEq { return a.Same("nope"); }   // ❌ E0463：string 不可赋给型参 T
bool ok<T>(T a, T b)  where T : IEq { return a.Same(b); }        // ✅ 实参也是 T（Self ≡ T）
```

`Self` 经 `_substSelfSig` 精确替换成型参 `T` 后，形参类型是**裸型参**。隐式转换判定里有一条
「恰一侧含泛型形参 → 擦除放行」的通用规则（服务 `T → object` / `T → 接口` 这类合法上转），
对「具体类型实参 → 裸型参 `T` 形参」这个方向**也会擦除**——`a.Same("nope")` 若照此放行，
运行期派发到 `T.Same` 后从 `string` 上读出不存在的字段（静默错值 / 崩）。

**为此有一道方向敏感的严格检查**：型参收者上
调约束接口方法时，`Self` 形参位（含藏在 `Func<Self,…>` 里的）若传入**具体类型**实参（完全不含型参、
非 error/unknown）→ 报 **E0463**。实参若也是型参（`T` → `T` 走 Identity、`U` → `T` 报 E0402）或
error/unknown（吸收、不级联）→ 不受影响。**只收「目标裸型参 + 源无具体」这一个方向，不碰通用擦除
规则**（`T → object` 等照旧放行），也**不改发射**（纯诊断、零字节漂移、无格式 bump）。

> **与 [E0454](#self-类型仅接口) 的分工**：E0454 管**接口静态类型**收者（`IEq a, b; a.Same(b)`）——
> `Self` 形参在那里是逆变、无唯一安全上界，只能一刀切**禁止**；E0463 管**型参收者**（`where T : IEq`
> 的 `T`）——那条路 `Self ≡ T` 精确（约束断言运行期 `T` 即实现类型），所以可以**真查实参**而非禁止。
>
> **通用**擦除收紧（任意裸型参目标位，需区分作用域内不透明型参 vs 待推断型参、要给 `Z42GenericParamType`
> 加 owner）仍是 Deferred `tighten-bare-type-param-target-erasure`——那是对通用规则动刀、爆炸半径另算。
>
> 「**收者位**的成员名不存在」（上文 E0401）不动目标位，也不需要给 `Z42GenericParamType` 加 owner。
> 剩下两条独立后续：**按字面全面执行成员可用性规则**（`enforce-bare-type-param-member-rule`，
> 开工前需先裁决 `var m = genericCall(); m.X` 是否接受判红）、以及**方法级 `where` 跨包导出**
> （`export-method-level-wheres`，见上）。

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
- **实现方可以是多字段（blob）struct**：`struct Vec2 : INumber`（两个字段）在泛型里照常
  `a + b`。它要多一道桥接 —— 见下方「blob struct 的返回位」。
- **实现方必须写 `static override`**：`public static override T op_Add(T a, T b)`。只写 `static`
  的方法注册到另一个键，运行期会 `VCall: function X.op_Add not found`。

### blob struct 的返回位

返回**多字段 struct** 的运算符实现（`static override Vec2 op_Add(Vec2, Vec2)`）比其它实现多一件事：
blob struct 走 **sret**（调用方传一个隐藏返回槽），而泛型体里 `T` 已擦除、调用点按引用发码、不传该槽。

这一格由编译器**自动桥接**处理，用户无需写任何东西：裸名槽放一个无 sret 的桥接（内部调具体实现、
把结果装箱返回），具体实现挪到内部名 `op_Add$struct`；具体类型收者（`p + q`）仍直接走后者，零装箱。

> 同一机制也覆盖接口声明 `Self Copy()` 被多字段 struct 实现、经接口收者调用的情形
> （机制见 [internals / missing-symbol](https://z42-lang.github.io/z42/internals/runtime/missing-symbol.html)）。

### 实现接口静态成员：四项校验（含跨包）

接口成员可以是 `static abstract`（`INumber` 的五个 `op_X` 就是）。实现方**必须**写
`public static override`，且满足性校验会逐项比对——
`MangleKey` 只比「名 + 形参」，所以 static 位与返回类型要另行比对：

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

发射点：`src/compiler/z42c.semantics/src/Symbols/InheritanceResolver.z42:441`（static）/ `:456`（可见性）
/ `:478`（返回类型）。

**跨包同口径**。接口方法的 static 位随 **zbc 1.41** 的接口方法块 `is_static:u8` 过 wire，
导入侧还原真值，因此**导入接口**与本包接口接受完全相同的四项校验。回归门：`src/compiler/z42c.pipeline/tests/fixtures/cross-zpkg/iface_static_impl_mismatch/`（负例 fixture，期望 build
error 含 `is \`static\` in the interface and an instance method here`）。

> 齐备性也校验：声明了接口
> 就必须实现它的**每个**成员——缺成员编译期报 **E0412** `\`C\` implements \`I\` but does not define
> member \`X\``。属性 `{ get; set; }` 的 `get_X`/`set_X`、索引器的 `get_Item`/`set_Item` 各是**独立契约**
> （满足性键含成员名），缺任一半都报。[关联类型](#关联类型type-item)另走 E0453（必须绑齐）。
> 本节上面讲的是「成员在、但**签名对不对**」那一层（static / 可见性 / 返回类型）；「成员**在不在**」由
> E0412 缺成员分支兜。
>
> 跨包也覆盖：接口的属性 / 索引器访问器（get_X/set_X/get_Item/
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
> 就没有实参可以写错）。标准库的三个协议接口全部是
> `Self` 形态，**它们自己不踩这个坑**。但这条 Deferred **仍然开着**——`Self` 是绕开、不是消除：
> 任何**其它**带类型实参的接口约束（`IEnumerable<T>` / `IComparer<T>` / 用户自定义泛型接口）
> 今天照旧按裸名匹配。

### 2. 方法级约束：推断失败时不校验

省略类型实参时（`Max(a, b)`），从实参结构化 unify 出型参绑定后复用**同一条** `ConstraintChecker.CheckMethod` 路径校验。

**残留边界**（推断失败即不发任何诊断）：型参未被任何形参位覆盖 /
同一型参绑到不同类型（v1 不做「最佳公共类型」，`Max(1, 2L)` 仍不校验）/ 实参是 lambda 或
target-typed `new` 这类延迟位 / `params` 尾位。Deferred：`generic-inference-best-common-type`、
`generic-inference-lambda-args`。

### 3. 函数类型约束的校验

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

> 🔴 **为什么必须校验**：不校验的话上面这段编译干净，运行期
> `uncaught exception: VCall: expected object, got I64(3)` —— `int 3` 被直接喂进 `string` 形参。
> 换成字符串拼接则**连崩都不崩**，静默打印 `got: [3]`，错值一路流下去。
> binder（`MemberResolver` 的 `Z42GenericParamType` 分支）按这条签名把 `handler(x)` 绑成 `CallIndirect`
> 并推断结果类型，所以必须核对实参真是这个签名。

**残留边界**（都是「漏报」，不会假红）：

| 边界 | 为什么 |
|------|--------|
| **跨包 class 级 func 约束不校验** | zbc 约束 bundle 的 flag 位没有 func 签名槽（要双格式 bump）；导入 bundle 恒无 func 约束 ⇒ 天然跳过。与关联类型跨包同一取舍 |
| **推断失败的调用点完全不校验** | `R Apply<T,R>(T f, int x) where T : Func<int,R>` 的 `R` 不出现在任何形参位 ⇒ `TypeArgInference` 边界 ①（任一型参未绑定即整体失败）⇒ `CheckMethod` 根本不跑 |
| **实参是 lambda / target-typed `new`** | 延迟位类型是 Unknown ⇒ 推断跳过 ⇒ 同上 |

### 4. 方法级 `where` 的声明级诊断

**声明级**诊断（`E0401` 未知型参 / `E0443` 未知约束名 / `E0402` class·struct 互斥 / `E0423` func 约束并置 /
`E0453` 关联类型绑定名笔误）一律在**声明期**（Pass 0.5 `Resolve`）发出，且只发一次：类级、方法级（无论被调用几次，
包括从不被调用的泛型方法）；`where U : IFoo`（U 不是该方法的型参）报 `E0401`（与类级同款）。

- 两条路（单文件 / 包并行）各有且只有一个 `Resolve`。
- `Resolve` 除了走 `ClassDecl` 的 where，还走**类成员方法**与**顶层自由函数**
  （`MethodDecl.IsFree`）的 where，只为发诊断、bundle 丢弃；`CheckMethod` 照旧在调用点即席建 bundle，
  但**不报**声明级诊断（`_fillBundle(..., report: false)`）。
- **不缓存声明期算出的 bundle**：方法级约束没有一把稳定的键（同名同 arity 的重载各有各的 where），
  造一张键不可靠的缓存表比即席重建（约束项个位数）危险得多。拆的是「报诊断」与「建 bundle」，不是加缓存。

> **违反约束**（`E0402` 实参不满足 / `E0422` 函数签名不符）**仍然每个调用点各报一条**——那本来就是
> per-call-site 的事实。

### 5. 关联类型：同包 + 跨包均已实现（zbc 1.42）

同包（见上「关联类型」一节）与**跨包**都已完整校验。跨包由三份数据经 wire 承载打通：
① 接口关联类型名单（`type Item;`）+ ② 类侧绑定（`type Item = int;`）→ zbc TYPE 记录尾部**统一 assoc 块**
（接口写 `(Item,"")`、类写 `(Item,int)`）；③ 约束绑定（`where T:IEnum<Item=int>`）→ 约束 bundle **bit7**。
`TsigReconcile`/`ImportedSymbolLoader` 恢复这三份数据后，跨包
导入类/接口与同包一样接受完整校验（约束声明处绑定名合法性、使用点绑定匹配、实现方补齐强制都不对导入侧特判）：绑定名不是该接口的关联类型 / 未绑定 / 绑定类型与要求不符，均报 E0453。
关联类型是纯编译期概念，runtime 只消费这两处新载荷保游标对齐（`validate_type_arg_constraint` 无关联类型分支）。

嵌套约束（`where T : IIterator<Item = U>, U : IDisplay` 里 `U` 再被约束）仍未实现。

## 相关

- [泛型方法](generic-methods.md) —— 方法级类型参数与 `<` 歧义消解
