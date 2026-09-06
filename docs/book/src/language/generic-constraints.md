# 泛型约束（`where` 子句）

> 对齐：2026-09-06（change `add-associated-types` PR-1/PR-2；前序 `complete-where-constraints`）
>
> 本页是**泛型约束语义与校验范围的 SoT**。泛型的整体设计（代码共享策略、reified 类型、
> 跨 zpkg 元数据）见 [`docs/design/language/generics.md`](../../../design/language/generics.md)；
> 方法级类型参数见 [泛型方法](generic-methods.md)。

## 语法

```z42
class Box<T> where T : IFoo { }                  // 接口约束
class Box<T> where T : IFoo + IBar { }           // 多约束用 `+` 分隔（Rust 风格，非 C# 的 `,`）
class Map<K, V> where K : IEquatable<K> where V : class { }   // 每个型参一条 where
void Sort<T>(T[] xs) where T : IComparable<T> { }             // 方法级
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
| 函数类型 | `where T : Func<int, R>` | — | ❌ 未发出（见下） |

`class` 与 `struct` 同时出现在一个型参上 → 报错（互斥）。

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

## 校验发生在哪里

| 时机 | 位置 | 报什么 |
|------|------|--------|
| 声明期 | 每个泛型类 / **接口**的 `where` 子句解析成约束集 | 未知型参 `E0401`、`class`/`struct` 互斥 `E0402`、**未知约束名 `E0443`** |
| 实例化点 | `new Box<D>()` | 违反约束 `E0402`，Span 指向实例化处 |
| 方法调用点 | `obj.m<T>(...)` / `C.m<T>(...)`（**显式**写类型实参时） | 违反约束 `E0402` |

诊断都携带真实 Span：约束声明错误指向 `where` 所在行，违反错误指向实例化 / 调用处。

**本包与跨包同口径**：导入类型的约束走同一个校验函数，判定规则完全一致（见下节）。

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

**作用域限定为接口**（不进类）：类里写 `Self` 是未定义类型 `E0443`，与其它拼错的类型名同码。
这条边界是刻意的——`Self` 进类会牵出协变返回类型那一整块设计面，不在本轮范围。

### 实现模型（以及它带来的边界）

`Self` **不是一个新类型**：解析时它被当作接口的一个**隐式类型参数**（追加在接口自己的
`TypeParams` 之后），随后与真型参 `T` 走完全同一条路。这个选择让约束位、成员签名匹配、
方法派发键三处**零改动**。

直接后果，需要知道：

- **实现类不必也不能写 `Self`**：实现方在自己的签名里写具体类型（上例的 `Point`）。
  今天没有「类实现接口时的成员签名齐备性校验」，所以写错也不会被抓——那是另一件事的欠债。
- **经接口静态类型调用返回 `Self` 的方法，结果类型就是 `Self` 本身**，不会被替换成接收者的
  静态类型：`IClone c; var x = c.Copy();` 里 `x` 的类型是型参 `Self`，还不能当具体类型用。
  要拿到可用的类型，在**具体类**上调用（`Point p; p.Copy()` → `Point`，走的是实现方的签名）。
  这一步的替换属于表达力的下一档，本轮不做。
- **跨包**：`Self` 与型参 `T` 一样以裸字符串写进 zbc 接口方法签名块，导入侧还原成型参，
  **零格式改动**。
## 运算符如何在型参上派发

`where T : INumber<T>` 让泛型代码直接写 `a + b`，而不必写 `a.op_Add(b)`：

```z42
T Sum<T>(T a, T b) where T : INumber<T> { return a + b; }
```

绑定路径（`ExprTyper._bindBinary`）：左操作数是 `Z42GenericParamType` → 到该型参的 where 约束
接口里找 `static abstract op_Add`（沿父接口链找；方法级与类级约束都查）→ 发**接收者驱动的
VCall**（`vcall a.op_Add(b)`），运行期由 `a` 的具体类决定跑哪个实现。与手写 `a.op_Add(b)`
（`generic_inumber.z42` 的写法）**发同一条指令**，只是省掉了显式方法名。

两条必须知道的规则：

- **结果类型恒为 `T`**。依据是协议本身——`INumber` 抬头写明「Mixed-type arithmetic is not
  supported（T + T → T only）」。这条不是可选的：`a + b + c` 的第二个 `+` 需要左侧仍是型参才能
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

## 已知限制（诚实标注）

这些不是 bug，是当前实现的**明确边界**。踩到时不要以为约束在保护你。

### 1. 接口约束只比裸名，不校验类型实参

`where T : IEquatable<T>` 只检查「T 实现了名为 `IEquatable` 的接口」，**不检查实参是否是
T 自己**。故 `class Foo : IEquatable<string>` 也能满足 `where T : IEquatable<T>`。

这与运行期行为一致（它拿到的同样是常量池里的裸名），故两边不产生分歧。裸名匹配还顺带
消掉了 F-bounded 自引用（`interface INumber<T> where T : INumber<T>`）朴素展开会无限递归的
问题。Deferred：`where-constraint-future-type-arg-matching`。

> [`Self`](#self-类型仅接口) 给了一条**绕开**这个限制的写法（`where T : IEq` 根本不写类型实参，
> 就没有实参可以写错），但**没有消除**它：`IEquatable<T>` 这类带实参的接口今天仍然全部按裸名匹配，
> 且标准库现有声明尚未改写成 `Self` 写法。所以这条 Deferred 仍然开着。

### 2. 方法级约束只在显式写类型实参时校验

`Max<int>(a, b)` 校验；`Max(a, b)`（靠推断）**不**校验。
Deferred：`where-constraint-future-inferred-method-args`。

### 3. 顶层函数的 `where` 不校验

只有类的成员方法走方法级校验路径。
Deferred：`where-constraint-future-toplevel-func`。

### 4. 函数类型约束从未发出诊断

`E0422` / `E0423` 已定义但没有代码路径会发出它们，即 `where T : Func<int,int>` 传进去什么都行、
**约束本身不校验**。Deferred：`where-constraint-future-func-constraint`。

> **事实校正（`fix-generic-func-param-indirect-call`）**：本节原写着「代码生成依赖该约束把参数当
> func 值走间接调用，改动需谨慎」——**不成立**。`CallEmitter` 从不看约束，它只查
> `Locals.ContainsKey(名字)`。真正决定 `f(x)` 走间接调用的是 binder（`MemberResolver` 的
> `Z42GenericParamType` 分支），而那条分支**一度不存在**：binder 把 `f(x)` 绑成自由函数调用并报
> E0401，只是诊断被 `--emit-zbc` 吞了，而 emitter 靠名字侥幸补救，于是直接调用能跑。
> 名字一旦不在当前帧 Locals 里（**被 lambda 捕获**）侥幸就没了——不发 `mk_clos`、lambda 体里发
> `call @f` 调一个不存在的自由函数，运行期 `undefined function`。
> 回归守卫：`src/tests/generics/func_constraint_captured.z42`。

### 5. 关联类型 / 嵌套约束**未实现**

`where T : IAdd<Output=T>`、`where T : IIterator<Item=U>, U : IDisplay` 这类 Rust 风格表达力
**当前不支持**——parser 没有 `Name=Type` 的解析。`docs/design/language/generics.md` 的设计
目标一节曾按已实现描述，那是**设计意图而非现状**。

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

## 相关

- [泛型方法](generic-methods.md) —— 方法级类型参数与 `<` 歧义消解
- [`docs/design/language/generics.md`](../../../design/language/generics.md) —— 泛型整体设计与选型
- change [`complete-where-constraints`](../../../spec/archive/2026-09-05-complete-where-constraints/proposal.md) —— 本页所述行为的引入过程（含三层塌陷的完整定位）
