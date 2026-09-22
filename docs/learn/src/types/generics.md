# 泛型

上一章的 `switch` 让一个函数能处理「几种情形」。泛型解决的是另一件事：让一个类或函数能处理
**任意类型**，而且**不丢掉类型信息**。

没有泛型时只有两条烂路：给每个类型抄一遍代码，或者一律用 `object` 装、取出来再强转——后者
把类型错误从编译期推到运行期，正是第 16 章那个 `InvalidCastException` 的来源。

## 泛型类：类型参数

`class Box<T>` 里的 `T` 是**类型参数**。它不是某个具体类型，是个占位——用的时候才决定。

```z42
// examples/types/generics/basic/basic.z42
{{#include ../../../../examples/types/generics/basic/basic.z42:decl}}
```

```z42
// examples/types/generics/basic/basic.z42
{{#include ../../../../examples/types/generics/basic/basic.z42:use}}
```

```console
{{#include ../../../../examples/types/generics/basic/run.console:use}}
```

关键在于 `Box<int>` 和 `Box<string>` 是**两个不同的类型**，各自的 `Get()` 返回各自的类型：
`n.Get() + 1` 是整数加法，`s.Get().Length` 是取字符串长度。**编译器全程知道 `T` 是什么**，
不需要你强转。

### 类型不匹配会被拦下

```z42
// examples/types/generics/basic/typemismatch.z42
{{#include ../../../../examples/types/generics/basic/typemismatch.z42}}
```

```console
{{#include ../../../../examples/types/generics/basic/run.console:mismatch}}
```

三条错各对应一个方向：取出来的类型、塞进去的类型、整个盒子的类型。`Box<int>` 与
`Box<string>` 之间**没有**任何赋值关系——哪怕 `int` 和 `string` 都是 `object`。

## 泛型方法：方法自己的类型参数

方法可以有自己的类型参数，**不要求所在的类是泛型的**：

```z42
// examples/types/generics/methods/methods.z42
{{#include ../../../../examples/types/generics/methods/methods.z42:decl}}
```

```z42
// examples/types/generics/methods/methods.z42
{{#include ../../../../examples/types/generics/methods/methods.z42:use}}
```

```console
{{#include ../../../../examples/types/generics/methods/run.console:use}}
```

两种写法：

- **能从实参推出来就不用写**——`pick(true, 1, 2)` 里 `T` 从 `1` / `2` 推成 `int`。
- **推不出来就显式写**——`nameOf<int>()` 没有任何实参携带 `T`，必须写 `<int>`。

个数写错会被拦下：

```z42
// examples/types/generics/methods/arity.z42
{{#include ../../../../examples/types/generics/methods/arity.z42}}
```

```console
{{#include ../../../../examples/types/generics/methods/run.console:arity}}
```

> `a < b && b > c` 不会被误当成泛型调用。编译器只在 `名<类型列表>` 后面紧跟 `(` 时才判为
> 泛型调用，否则回退成比较。

## `where` 约束：给类型参数提要求

裸的 `T` 上**什么成员都调不了**——编译器不知道它是什么，也就不知道它有什么。`where` 就是
告诉编译器「`T` 至少满足这些」，于是那些成员变得可用。

```z42
// examples/types/generics/constraints/constraints.z42
{{#include ../../../../examples/types/generics/constraints/constraints.z42:iface}}
```

```z42
// examples/types/generics/constraints/constraints.z42
{{#include ../../../../examples/types/generics/constraints/constraints.z42:newc}}
```

```z42
// examples/types/generics/constraints/constraints.z42
{{#include ../../../../examples/types/generics/constraints/constraints.z42:use}}
```

```console
{{#include ../../../../examples/types/generics/constraints/run.console:use}}
```

七种约束：

| 写法 | 要求 |
|---|---|
| `where T : IShape` | T 实现了 `IShape`（含接口继承链） |
| `where T : Base` | T 是 `Base` 或它的子类 |
| `where T : class` | T 是引用类型（基元和 struct 不满足） |
| `where T : struct` | T 是值类型 |
| `where T : enum` | T 是 `enum` 声明的类型 |
| `where T : new()` | T 能零实参构造（`abstract` 类一律不满足） |
| `where U : T` | U 的实参能赋给 T 的实参 |

> ⚠️ **多个约束用 `+` 连接**，不是逗号：`where T : IFoo + IBar`（Rust 风格）。多个类型参数
> 各写各的 `where`：`where K : IEquatable where V : class`。

`new()` 有一条容易搞错：**完全没写构造器 = 有默认构造 = 满足**。只有「写了构造器、却没有
一个能零实参调用」才不满足——形参全带默认值的也算能零实参调用。

### 不满足约束会被拦下

```z42
// examples/types/generics/constraints/violate.z42
{{#include ../../../../examples/types/generics/constraints/violate.z42}}
```

```console
{{#include ../../../../examples/types/generics/constraints/run.console:violate}}
```

## `Self`：指代「实现者自己」

这是 z42 与 C# 差别最大的一处。C# 里「能和自己比较」要写成
`class Money : IEquatable<Money>`——那个类型实参纯粹是样板，它不携带任何信息，只是在说
「我指我自己」。z42 用 `Self` 把它消掉：

```z42
// examples/types/generics/selftype/selftype.z42
{{#include ../../../../examples/types/generics/selftype/selftype.z42:decl}}
```

```z42
// examples/types/generics/selftype/selftype.z42
{{#include ../../../../examples/types/generics/selftype/selftype.z42:use}}
```

```console
{{#include ../../../../examples/types/generics/selftype/run.console:use}}
```

两条规则：

- **接口里写 `Self`，实现类里写具体类型**（上面是 `Same(Money other)`）。实现类里写
  `Self` 是错的——`Self` 只在接口内有意义。
- **约束侧不写类型实参**：`where T : IEq`，不是 `where T : IEq<T>`。

标准库的三个协议接口都是这个形状：`IEquatable` / `IComparable` / `INumber` 全都非泛型 + `Self`。

### `Self` 形参不能经接口类型调用

```z42
// examples/types/generics/selftype/viaiface.z42
{{#include ../../../../examples/types/generics/selftype/viaiface.z42}}
```

```console
{{#include ../../../../examples/types/generics/selftype/run.console:viaiface}}
```

为什么必须拦：`a` 和 `b` 的静态类型都是 `IEq`，但一个是 `P`、一个是 `Q`。`P.Same` 要的是
`P`，喂给它一个 `Q` 会从错误的内存位置读字段——**放行的话既不报错也不崩，静默返回错的结果**。
诊断里给的替代写法就是把接口类型换成类型参数（`where T : IEq`），那条路上 `Self` 就**精确**
等于 `T`。

## 关联类型：让实现方决定一个类型

接口除了声明方法，还能声明**一个由实现方决定的类型**：

```z42
// examples/types/generics/assoc/assoc.z42
{{#include ../../../../examples/types/generics/assoc/assoc.z42:decl}}
```

```z42
// examples/types/generics/assoc/assoc.z42
{{#include ../../../../examples/types/generics/assoc/assoc.z42:use}}
```

```console
{{#include ../../../../examples/types/generics/assoc/run.console:use}}
```

绑错了会被拦下：

```z42
// examples/types/generics/assoc/mismatch.z42
{{#include ../../../../examples/types/generics/assoc/mismatch.z42}}
```

```console
{{#include ../../../../examples/types/generics/assoc/run.console:mismatch}}
```

> 同一行报两遍不是笔误——局部变量的**声明类型**和 `new` 表达式各算一处类型引用，各报一条。

三条规则：`type Item;` 只能写在接口里（不带绑定）；`type Item = int;` 只能写在实现类里
（必须绑定，而且实现了就得绑齐）；`Name = Type` 这种写法**只在 `where` 约束位**能用。

## 型参上的运算符

`where T : INumber` 之后，泛型代码里可以直接写 `a + b`，不必写 `a.op_Add(b)`：

```z42
// examples/types/generics/operators/operators.z42
{{#include ../../../../examples/types/generics/operators/operators.z42:decl}}
```

```z42
// examples/types/generics/operators/operators.z42
{{#include ../../../../examples/types/generics/operators/operators.z42:use}}
```

```console
{{#include ../../../../examples/types/generics/operators/run.console:use}}
```

同一个 `sum` 对 `int` 走整数加法、对 `double` 走浮点加法——运行期按实际类型派发。
结果类型恒为 `T`（所以 `a + b + c` 里第二个 `+` 还能继续走这条路）。

> 自己的类型想参与这条路，得实现 `INumber` 并且把运算符写成 **`static override`**
> （只写 `static` 会注册到另一个键上，运行期找不到）。

## 🔴 当前实现的边界

这些不是写法问题，是**当前实现的洞**。踩到时别怀疑自己。

### `typeof(T)` 对类级型参给不出真类型

```z42
// examples/types/generics/gaps/typeofgap.z42
{{#include ../../../../examples/types/generics/gaps/typeofgap.z42}}
```

```console
{{#include ../../../../examples/types/generics/gaps/run.console:typeof}}
```

`full()` 该打 `Int32`，实际打 `T`；`isInt()` 该是 `true`，实际 `false`——**静默走错分支，
零诊断**。泛型类里按 `typeof(T)` 分派的代码会安静地跑错。

**方法级的 `typeof(U)` 是对的**（上面 `nameOf<int>()` 打出 `Int32` 就是它）；只有**类级**
型参的 `typeof` 拿不到实参。同一个类级 `T` 的 `default(T)` **是对的**（第三行打出 `Int32`）。

要按类型分派，就把型参放在**方法**上；只想知道具体类型，用 `default(T).GetType()`。

### 泛型构造器的实参不检查

```z42
// examples/types/generics/gaps/ctorgap.z42
{{#include ../../../../examples/types/generics/gaps/ctorgap.z42}}
```

```console
{{#include ../../../../examples/types/generics/gaps/run.console:ctor}}
```

同一个 `T` 的**实例方法**是检查的（`n.Set("str")` 会报
`cannot assign string to Int32`），只有**构造器**这条路漏了——错误一路推到运行期才现形。

### 接口约束只比接口名，不比类型实参

`where T : IFoo<T>` 只检查「T 实现了名叫 `IFoo` 的接口」，**不检查实参是不是 T 自己**——
所以 `class Wrong : IFoo<string>` 也能满足 `where T : IFoo<T>`。这正是标准库把三个协议
接口改成 `Self` 的原因：不写类型实参，就没有实参可以写错。

### 两个会崩的组合

- **`new T()` 且 T 是基元**（`make<int>()`）：约束判定说基元满足 `new()`，真去构造却崩。
  用 `default(T)` 代替。
- **字段数 ≥ 2 的 struct 走 `where T : INumber` 的运算符派发**：崩
  `MissingSymbolException`。单字段 struct 与基元不受影响。

## 小结

- 泛型让一个类 / 方法处理任意类型，而**不丢类型信息**——不必用 `object` + 强转。
- `Box<int>` 与 `Box<string>` 是**两个不同类型**，互不赋值。
- 类型参数有两级：**类级**（`class Box<T>`）和**方法级**（`pick<T>()`），同名时方法级优先。
  能从实参推出来就不写类型实参。
- 裸 `T` 上什么都调不了；**`where` 是让成员变可用的开关**。七种约束，多约束用 `+` 连。
- 🔴 **`Self` 是 z42 的特色**：接口里指代实现者自己，省掉 `IEquatable<T>` 那种自引用样板。
  形参位的 `Self` 不能经接口类型调用——改用型参。
- 关联类型 `type Item;` 让实现方决定一个类型，约束侧用 `IStore<Item = int>` 要求它。
- 🔴 记住四个边界：类级 `typeof(T)` 给不出真类型、泛型构造器实参不检查、接口约束只比名字、
  `new T()` 遇基元会崩。

完整的约束语义、校验时机与跨包传递，见
[泛型约束](https://z42-lang.github.io/z42/reference/language/generic-constraints.html)
与[泛型方法](https://z42-lang.github.io/z42/reference/language/generic-methods.html)。
