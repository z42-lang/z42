# 组织代码

前面每一章的代码都装在一两个文件里。真实项目会有几十上百个文件，这一章讲怎么把它们摆好：
用 `namespace` 给名字分组、用 `using` 声明这个文件要用到什么、用访问控制决定哪些东西对外可见、
用 `partial` 把一个类型拆到几个文件里。

## `namespace`：给名字分组

一个文件最多写一条 `namespace`，写在最前面。它改变文件里所有声明的**限定名**：

```z42
// examples/types/organization/ns/src/Geometry.z42
{{#include ../../../../examples/types/organization/ns/src/Geometry.z42}}
```

```z42
// examples/types/organization/ns/src/Main.z42
{{#include ../../../../examples/types/organization/ns/src/Main.z42}}
```

```console
{{#include ../../../../examples/types/organization/ns/run.console:run}}
```

两件事值得注意：

- **用到别的 namespace 就要 `using`，同一个包里也一样**——上面 `Demo.App` 要用 `Demo.Geometry`
  的 `Point`，得写 `using Demo.Geometry;`。只有**外围** namespace 例外（与 C# 相同）：写在
  `Demo.App` 里的代码，不写 `using` 就能用 `Demo` 里的东西。
- **限定名就是调用栈里看到的名字**（`Demo.App.Boom`），也是清单里 `entry` 要写的名字。

不写 `namespace` 的文件归属默认命名空间 `main`。

> 熟悉 C# 的读者请注意：z42 **没有**花括号包起来的 `namespace Foo { ... }` 写法，
> 只有文件级的 `namespace Foo;`。

## `using`：声明这个文件要用什么

标准库里只有 `Std` 和 `Std.Runtime` 是自动可用的。**其它一切都要显式 `using`**——
包括同样以 `Std.` 打头的 `Std.IO`、`Std.Collections`、`Std.Text` 等等。

```z42
// examples/types/organization/noimport/noimport.z42
{{#include ../../../../examples/types/organization/noimport/noimport.z42}}
```

```console
{{#include ../../../../examples/types/organization/noimport/run.console:missing}}
```

报错会**点名缺哪个命名空间**，照着补一行就行。

> **全限定名不需要 `using`**：写成 `Std.IO.Console.WriteLine(...)` 也能编译，适合偶尔用一次的名字。
> 写起来长，常用的还是 `using`。反过来，只被全限定名引用到的命名空间，它的 `using` 会被编译器提示为多余。

### `using` 是**文件级**的

每个文件要自己写自己的 `using`——**兄弟文件写过不算**：

```z42
// examples/types/organization/filelevel/src/Shapes.z42
{{#include ../../../../examples/types/organization/filelevel/src/Shapes.z42}}
```

```z42
// examples/types/organization/filelevel/src/Main.z42
{{#include ../../../../examples/types/organization/filelevel/src/Main.z42}}
```

```console
{{#include ../../../../examples/types/organization/filelevel/run.console:run}}
```

这条规则看着啰嗦，好处是**每个文件的依赖自己说清楚**：删掉一个文件的 `using` 只会让这个
文件报错，不会让另一个不相关的文件神秘地编译失败。

### `global using`：真的处处都要用的，写一处

某个命名空间确实全包都在用（日志、集合），可以在一个文件里写 `global using`，
**整个包的文件都不必再写**：

```z42
// examples/types/organization/globalusing/src/prelude.z42
{{#include ../../../../examples/types/organization/globalusing/src/prelude.z42}}
```

```z42
// examples/types/organization/globalusing/src/Main.z42
{{#include ../../../../examples/types/organization/globalusing/src/Main.z42}}
```

```console
{{#include ../../../../examples/types/organization/globalusing/run.console:run}}
```

习惯做法是单独建一个 `prelude.z42` 专放这些，一眼就能看到全包的公共依赖。

⚠️ **别滥用**：`global using` 一多，读某个文件时就看不出它到底依赖什么了——那正是文件级
`using` 想解决的问题。只给「真的处处都要用」的那几个。

## 类型别名：给类型起个短名字

`using 名字 = 类型;` 在**当前文件**里给类型起个别名，常用来压缩长泛型或表达语义：

```z42
// examples/types/organization/alias/alias.z42
{{#include ../../../../examples/types/organization/alias/alias.z42:decl}}
```

```z42
// examples/types/organization/alias/alias.z42
{{#include ../../../../examples/types/organization/alias/alias.z42:use}}
```

```console
{{#include ../../../../examples/types/organization/alias/run.console:use}}
```

三条规则：

- **别名与目标类型完全互通**——`UserId` 就是 `int`，可以互相赋值，**不是**新类型。
  想要「不能互相赋值的 UserId」得用 `struct` 包一层。
- **只在声明它的文件里有效**，不会跟着类型导出到别的包。
- 泛型别名可以直接 `new`（`new Scores()`）。

## 访问控制：哪些东西对外可见

默认值只有两条要记：

| 声明在哪 | 不写修饰符就是 |
|---|---|
| 顶层（类 / 接口 / struct / record / enum / 函数） | `internal`——**本包内**可见 |
| 类的成员（字段 / 方法 / 构造器 / 属性） | `private`——**本类内**可见 |

```z42
// examples/types/organization/access/access.z42
{{#include ../../../../examples/types/organization/access/access.z42:decl}}
```

```z42
// examples/types/organization/access/access.z42
{{#include ../../../../examples/types/organization/access/access.z42:use}}
```

```console
{{#include ../../../../examples/types/organization/access/run.console:use}}
```

四个修饰符：

| 修饰符 | 谁能访问 |
|---|---|
| `private` | 只有当前类（**子类访问不了基类的 private**）|
| `protected` | 当前类 + 所有子类（跨包也行）|
| `internal` | 同一个包 |
| `public` | 所有人 |

忘了写 `public` 是最常见的一跤：

```z42
// examples/types/organization/access/private.z42
{{#include ../../../../examples/types/organization/access/private.z42}}
```

```console
{{#include ../../../../examples/types/organization/access/run.console:private}}
```

⚠️ **构造器也遵守这条**。`Point(int x)` 不写修饰符就是 `private`，类外 `new Point(3)` 会被拦下——
要给外面用就写 `public Point(int x)`。

> **例外只有三处**：不写修饰符的 `override` 方法、`[Record]` 的定位字段、
> 以及主构造器 `class P(int X)` 的那个构造器，都自动是 `public`。
> 注意主构造器那条只管**构造器**——裸 `class P(int X)` 的字段 `X` 仍是 private，
> 想要一个字段公开的数据类就加 `[Record]`（第 16 章）。

顶层声明标 `private` / `protected` 没有意义（默认已经是 `internal`），会直接报错：

```z42
// examples/types/organization/access/toplevel.z42
{{#include ../../../../examples/types/organization/access/toplevel.z42}}
```

```console
{{#include ../../../../examples/types/organization/access/run.console:toplevel}}
```

## `partial`：一个类型拆到几个文件

给类型加 `partial`，就能把它分成几段写在不同文件里，编译期合并成一个：

```z42
// examples/types/organization/partial/src/Widget.Size.z42
{{#include ../../../../examples/types/organization/partial/src/Widget.Size.z42}}
```

```z42
// examples/types/organization/partial/src/Widget.Text.z42
{{#include ../../../../examples/types/organization/partial/src/Widget.Text.z42}}
```

```console
{{#include ../../../../examples/types/organization/partial/run.console:run}}
```

一个碎片里的方法可以直接用另一个碎片的字段（`Area()` 用了 `Height`）——合并之后它们本来
就是同一个类。

规则：

- **每一段都要写 `partial`**，少写一个就报错（防止你以为在拆、其实是不小心重名）。
- 所有碎片必须**同包、同 namespace、同种类**（都是 `class` 或都是 `interface`…）。
- 基类和主构造器**至多一个碎片**能写。
- 同名同签名的成员在两个碎片里重复声明会报错；**重载不算重复**。

典型用途是按维度拆：平台相关的一段一个文件、生成的代码单独一个文件。
`partial` 也能用在 `struct` / `record` / `interface` 上。

## 小结

- **`namespace Foo;`** 文件级、一个文件最多一条、写在最前面；它决定限定名（= 栈里的名字）。
  同包内不同 namespace 互相可见，不需要 `using`。
- **`using` 管跨包，且是文件级的**：只有 `Std` / `Std.Runtime` 自动可用，其它都要本文件自己写；
  写全限定名可以不 `using`。确实处处要用的写 `global using`（一处，全包生效），但别滥用。
- **`using 名字 = 类型;`** 是文件级别名，与目标类型**完全互通**（不是新类型），不导出。
- 访问控制默认值：**顶层 `internal`、成员 `private`**。构造器同样适用——忘写 `public`
  就 `new` 不出来。顶层标 `private` / `protected` 直接报错。
- **`partial`** 把一个类型拆到几个文件；每段都要写 `partial`，必须同包同 namespace 同种类。

下一章讲**特性与反射入门**——怎么用内置 attribute，以及 `typeof` / `GetType` 和成员查询。
