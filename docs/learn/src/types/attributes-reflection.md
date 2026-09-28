# 特性与反射入门

到这里，你写的每个类型在编译之后都还带着一份**自我说明**：它叫什么、有哪些字段和方法、
每个方法收几个参数。**反射**就是在程序运行时把这份说明读出来，并据此读写字段、调用方法、
造出实例。**attribute（特性）** 是你自己往这份说明里添的注解——贴在声明上的一小块数据，
运行时再读回来。

这两件事合起来能做的事，是写死的代码做不到的：一个序列化器不必知道你的类长什么样、
一个命令行工具不必手写「命令名 → 函数」的表。本章讲够用的部分，完整 API 见
[反射](https://z42-lang.github.io/z42/reference/stdlib/reflection.html)。

反射的类型分两处：`Std.Type` 在 prelude 里（不用 `using`），描述成员的
`FieldInfo` / `MethodInfo` / … 都在 `Std.Reflection`。

## 拿到一个类型：`typeof` 与 `GetType()`

`typeof(T)` 用于编译期就知道的类型，`obj.GetType()` 用于手上只有一个对象的时候。
两者给的是同一个东西——`Std.Type`：

```z42
// examples/types/attributes-reflection/handles/handles.z42
{{#include ../../../../examples/types/attributes-reflection/handles/handles.z42:handle}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/handles/run.console:handle}}
```

`Name` 是简单名，`FullName` 带上命名空间。同一个类型无论从哪个入口拿到，**用 `==` 比是相等的**。

### 名字用的是包装类型的拼写

反射报的名字不是你写的关键字——`int` 的 `Name` 是 `Int32`：

```z42
// examples/types/attributes-reflection/handles/handles.z42
{{#include ../../../../examples/types/attributes-reflection/handles/handles.z42:names}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/handles/run.console:names}}
```

写断言前先记住这张对照，否则会白找半天：

| 你写的 | `Name` | `FullName` |
|---|---|---|
| `int` | `Int32` | `Std.Int32` |
| `string` | `String` | `Std.String` |
| `double` | `Double` | `Std.Double` |
| `bool` | `Boolean` | `Std.Boolean` |
| `int[]` | `Int32[]` | `Std.Int32[]` |
| 自己的 `Shop.Product` | `Product` | `Shop.Product` |

`typeof(int)` 与 `(5).GetType()` 是同一个类型身份，所以上面最后一行是 `true`。

### 问这个类型是什么形状

一组谓词回答「它是类还是值类型、是不是枚举、是不是数组」：

```z42
// examples/types/attributes-reflection/handles/handles.z42
{{#include ../../../../examples/types/attributes-reflection/handles/handles.z42:shape}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/handles/run.console:shape}}
```

常用的还有 `IsInterface` / `IsAbstract` / `IsSealed` / `IsRecord` / `IsGenericType`。

> ⚠️ `string` 的 `IsPrimitive` 是 `false`（和 C# 一致）——它是引用类型，不是基元。

### 按名字找类型

名字只有在运行时才知道（比如从配置文件读来）时，用 `Type.GetType`，找不到给 `null`：

```z42
// examples/types/attributes-reflection/handles/handles.z42
{{#include ../../../../examples/types/attributes-reflection/handles/handles.z42:byname}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/handles/run.console:byname}}
```

## 查一个类型有哪些成员

四个枚举方法，各自返回一个数组。下面这个类贯穿本节：

```z42
// examples/types/attributes-reflection/members/members.z42
{{#include ../../../../examples/types/attributes-reflection/members/members.z42:decl}}
```

`GetFields()` 给出实例字段与静态字段，**含私有字段**，继承来的排在前面：

```z42
// examples/types/attributes-reflection/members/members.z42
{{#include ../../../../examples/types/attributes-reflection/members/members.z42:fields}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/members/run.console:fields}}
```

注意 `Bank` 这个自动属性的后备字段**没有**出现——它是编译器合成的，反射把它藏起来了。

`GetMethods()` 给出方法，包括从 `Object` 继承来的四个，以及属性访问器 `get_X` / `set_X`：

```z42
// examples/types/attributes-reflection/members/members.z42
{{#include ../../../../examples/types/attributes-reflection/members/members.z42:methods}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/members/run.console:methods}}
```

`GetProperties()` 把那些访问器归并成属性视图——有 getter 就可读，有 setter 就可写：

```z42
// examples/types/attributes-reflection/members/members.z42
{{#include ../../../../examples/types/attributes-reflection/members/members.z42:props}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/members/run.console:props}}
```

`GetConstructors()` 单独一份，**构造器不在 `GetMethods()` 里**。每个形参的名字和类型都查得到：

```z42
// examples/types/attributes-reflection/members/members.z42
{{#include ../../../../examples/types/attributes-reflection/members/members.z42:ctors}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/members/run.console:ctors}}
```

## 读写字段、调用方法、造实例

拿到 `FieldInfo` / `MethodInfo` 之后就能动真格了。先是字段——`GetValue` / `SetValue`
直接读写那个槽，不经过属性访问器：

```z42
// examples/types/attributes-reflection/invoke/invoke.z42
{{#include ../../../../examples/types/attributes-reflection/invoke/invoke.z42:field}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/invoke/run.console:field}}
```

**写不进去的值会抛异常，而且字段保持原值**——不会悄悄写进去半个：

```z42
// examples/types/attributes-reflection/invoke/invoke.z42
{{#include ../../../../examples/types/attributes-reflection/invoke/invoke.z42:badwrite}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/invoke/run.console:badwrite}}
```

`int` / `bool` / `double` / `struct` 字段**永不接受 `null`**（和编译期同一条规则）；
引用类型字段写 `null` 则完全合法。

属性用同名的一对方法，走的是 getter / setter：

```z42
// examples/types/attributes-reflection/invoke/invoke.z42
{{#include ../../../../examples/types/attributes-reflection/invoke/invoke.z42:prop}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/invoke/run.console:prop}}
```

调用方法用 `Invoke(接收者, 实参数组)`。静态方法的接收者传 `null`；实参按声明顺序放进
`object[]`；`void` 方法返回 `null`：

```z42
// examples/types/attributes-reflection/invoke/invoke.z42
{{#include ../../../../examples/types/attributes-reflection/invoke/invoke.z42:call}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/invoke/run.console:call}}
```

造实例有两条路：无参构造用 `Activator.CreateInstance(t)`，带参构造用
`ConstructorInfo.Invoke(实参数组)`：

```z42
// examples/types/attributes-reflection/invoke/invoke.z42
{{#include ../../../../examples/types/attributes-reflection/invoke/invoke.z42:create}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/invoke/run.console:create}}
```

> 反射调用比直接写 `a.Deposit(50)` 慢得多，也绕过了编译器的检查（参数个数不对只能等到
> 运行时抛异常）。它的用处是**类型在编译期还不知道**的场合；知道类型就直接调。

## `methodof`：精确指代一个方法

`typeof(T)` 指代一个类型，`methodof` 指代一个**方法**，求值同样得到一个 `MethodInfo`。
括号里写参数类型列表，重载也能指得一清二楚：

```z42
// examples/types/attributes-reflection/methodof/methodof.z42
{{#include ../../../../examples/types/attributes-reflection/methodof/methodof.z42:decl}}
```

```z42
// examples/types/attributes-reflection/methodof/methodof.z42
{{#include ../../../../examples/types/attributes-reflection/methodof/methodof.z42:use}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/methodof/run.console:use}}
```

拿到的就是上一节那个 `MethodInfo`，形参能查、能 `Invoke`：

```z42
// examples/types/attributes-reflection/methodof/methodof.z42
{{#include ../../../../examples/types/attributes-reflection/methodof/methodof.z42:invoke}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/methodof/run.console:invoke}}
```

它的价值是**把运行期的静默失效变成编译期报错**：上一节那种「枚举 `GetMethods()` 再按
字符串名筛」的写法，方法改名、签名变了都只能等到运行期才发现；`methodof` 会在引用点当场报错。
没有重载时参数列表可以省，但**一旦有重载就必须写**，否则报错——它绝不替你猜一个。

必须写成 `类型.成员` 的形式，自由函数指不了：

```z42
// examples/types/attributes-reflection/methodof/free.z42
{{#include ../../../../examples/types/attributes-reflection/methodof/free.z42}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/methodof/run.console:free}}
```

## 用现成的 attribute

attribute 写成方括号贴在声明前面。前面的章节已经用过一个：第 16 章的 `[Record]`。
另一个马上能用的是 `[Deprecated]`——它让**调用点**收到告警：

```z42
// examples/types/attributes-reflection/builtin/deprecated.z42
{{#include ../../../../examples/types/attributes-reflection/builtin/deprecated.z42}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/builtin/run.console:run}}
```

告警指着**调用那一行**，并带上你写在 attribute 里的迁移说明；程序照常编译运行。

内置的 attribute 还有几个，各归各章：`[Record]` 见第 16 章，`[Test]` / `[Setup]` 等属于
写测试，`[Suppress("<诊断码>")]` 用来局部关掉一条诊断。完整清单见
[特性](https://z42-lang.github.io/z42/reference/language/attributes.html)。

## 定义自己的 attribute

attribute 就是一个继承 `Std.Attribute` 的普通类，**类名必须以 `Attribute` 结尾**，
贴的时候把后缀剥掉：类 `RouteAttribute` 写作 `[Route]`。

```z42
// examples/types/attributes-reflection/custom/custom.z42
{{#include ../../../../examples/types/attributes-reflection/custom/custom.z42:decl}}
```

**所有状态都走构造器**——没有「一部分走构造器、一部分直接赋字段」的第二条路，
所以默认值和命名实参照常可用：

```z42
// examples/types/attributes-reflection/custom/custom.z42
{{#include ../../../../examples/types/attributes-reflection/custom/custom.z42:apply}}
```

读回来靠 `GetAttribute(typeof(...))`（不存在返回 `null`）或 `GetCustomAttributes()`
（全部，按你贴的顺序）。⚠️ **查询用真实类名 `RouteAttribute`，不是剥了后缀的 `Route`**：

```z42
// examples/types/attributes-reflection/custom/custom.z42
{{#include ../../../../examples/types/attributes-reflection/custom/custom.z42:readback}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/custom/run.console:readback}}
```

拿回来的是**活的实例**：`route.Path` 就是普通的字段访问。

### 能贴在哪

类、方法、字段、自动属性、形参——五处的读法完全一样：

```z42
// examples/types/attributes-reflection/custom/custom.z42
{{#include ../../../../examples/types/attributes-reflection/custom/custom.z42:carriers}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/custom/run.console:carriers}}
```

### 忘了后缀会怎样

```z42
// examples/types/attributes-reflection/custom/suffix.z42
{{#include ../../../../examples/types/attributes-reflection/custom/suffix.z42}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/custom/run.console:suffix}}
```

`E0444` 点名要改成什么；跟着的 `E0443` 是同一件事的连带报告——`[Route]` 会去找
`RouteAttribute`，而那个类还不存在。

> 熟悉 C# 的读者请注意：C# 里后缀是**可选**的，`[Foo]` 与 `[FooAttribute]` 两种写法都行；
> z42 只有一种——**类名带后缀、应用剥后缀**。另外 z42 暂时没有 `AttributeUsage`，
> 也就是说 attribute 能贴在任何支持的位置上，不限制目标。

### 实参必须是编译期常量

attribute 是**数据**，所以实参只能写编译期就定死的东西：字面量、常量表达式、enum 成员、
`const` 字段、`typeof(...)`、`methodof(...)`，以及由这些构成的数组
（`new string[]{ "a", "b" }`）。写别的会报 `E0500`：

```z42
// examples/types/attributes-reflection/custom/constarg.z42
{{#include ../../../../examples/types/attributes-reflection/custom/constarg.z42}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/custom/run.console:constarg}}
```

`K.FIXED` 是 `const`，所以 `Good` 那行没事；被拒的两行一个读可变静态字段、一个调方法。
这条限制有个具体的理由：attribute 实例不是编译期就造好的，而是**第一次查询它的时候**才造、
之后缓存——实参若依赖会变的状态，读回来的「元数据」就取决于谁先查。

## 合起来用：一张自己长出来的命令表

attribute + 反射最典型的用法：让代码**按数据驱动**，而不是手写一张表。下面这个小程序里，
加一条命令只需要写个方法、贴个 `[Command]`——派发和帮助都会自动带上它。

```z42
// examples/types/attributes-reflection/registry/registry.z42
{{#include ../../../../examples/types/attributes-reflection/registry/registry.z42:attr}}
```

```z42
// examples/types/attributes-reflection/registry/registry.z42
{{#include ../../../../examples/types/attributes-reflection/registry/registry.z42:commands}}
```

驱动的部分只认 attribute，不认方法名：

```z42
// examples/types/attributes-reflection/registry/registry.z42
{{#include ../../../../examples/types/attributes-reflection/registry/registry.z42:driver}}
```

```z42
// examples/types/attributes-reflection/registry/registry.z42
{{#include ../../../../examples/types/attributes-reflection/registry/registry.z42:use}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/registry/run.console:use}}
```

没贴 `[Command]` 的 `Internal` 既不出现在帮助里，也派发不到——**这张表的内容由声明处决定**。

## 🔴 当前实现的边界

这些不是写法问题，踩到时别怀疑自己。

### 顶层函数上的 attribute 反射拿不到

贴是能贴、也能编译（编译期消费它的东西照常工作），但反射的入口都要先有一个 `Type`，
而顶层函数不属于任何类型——于是没有任何办法拿到它的 `MethodInfo`。**要反射就把它挪进某个类
当 `static` 方法**：

```z42
// examples/types/attributes-reflection/gaps/gaps.z42
{{#include ../../../../examples/types/attributes-reflection/gaps/gaps.z42:freefunc}}
```

```z42
// examples/types/attributes-reflection/gaps/gaps.z42
{{#include ../../../../examples/types/attributes-reflection/gaps/gaps.z42:freefunc-run}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/gaps/run.console:freefunc}}
```

输出里只有类里那个 `Helper`——顶层那个贴了 `[Doc]` 的同名函数，反射看不见。

### 只有自动属性带得了 attribute

写了 `get { ... }` 体的**计算属性**没有后备字段，attribute 无处安放，读回来永远是空的：

```z42
// examples/types/attributes-reflection/gaps/gaps.z42
{{#include ../../../../examples/types/attributes-reflection/gaps/gaps.z42:computed}}
```

```z42
// examples/types/attributes-reflection/gaps/gaps.z42
{{#include ../../../../examples/types/attributes-reflection/gaps/gaps.z42:computed-run}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/gaps/run.console:computed}}
```

### 没有「按名字取一个成员」的方法

`GetMethod("Foo")` / `GetField("x")` 这类便利方法都不存在，只能枚举后自己筛
（`GetInterface(name)` 是唯一的例外）：

```z42
// examples/types/attributes-reflection/gaps/gaps.z42
{{#include ../../../../examples/types/attributes-reflection/gaps/gaps.z42:byname}}
```

```console
{{#include ../../../../examples/types/attributes-reflection/gaps/run.console:byname}}
```

要在代码里精确指代一个方法，优先用上面的 `methodof`——它是编译期检查的。

### 还有一处

- **索引器不出现在 `GetProperties()` 里**：`this[int]` 降解成 `get_Item(int)`，
  只在 `GetMethods()` 里作为普通方法出现。

## 小结

- **反射**读的是类型自带的元数据：`typeof(T)` / `obj.GetType()` 拿 `Std.Type`，
  `Type.GetType("名字")` 按名字查（找不到给 `null`）。
- 反射报的名字是包装类型的拼写（`int` → `Int32`），写断言前先对一眼。
- 四个枚举方法：`GetFields()`（含私有与静态、藏起自动属性的后备字段）/ `GetMethods()`
  （含继承与访问器、**不含构造器**）/ `GetProperties()` / `GetConstructors()`。
- 能动真格：`FieldInfo.GetValue` / `SetValue`、`PropertyInfo` 同名的一对、
  `MethodInfo.Invoke(接收者, object[])`、`Activator.CreateInstance` 与
  `ConstructorInfo.Invoke`。写不进去的值会抛异常且**字段保持原值**。
- `methodof(类型.成员(参数类型))` 在编译期就把方法钉住，比按字符串名筛安全；自由函数指不了。
- **attribute** 是贴在声明上的数据：类名必须以 `Attribute` 结尾（`E0444`），应用时剥后缀，
  全部状态走构造器，查询用真实类名。能贴在类 / 方法 / 字段 / 自动属性 / 形参五处。
  实参必须是编译期常量（含 `typeof` / `methodof` / 数组），否则报 `E0500`。
- 🔴 记住三个边界：顶层函数的 attribute 反射拿不到（挪进类当 `static`）、计算属性带不了
  attribute、没有按名取单个成员的 API（索引器也不在 `GetProperties()` 里）。

下一部分转向**标准库实战**，从文件与目录开始。
