# 类与对象

前面存的都是**散的**值：几个变量、一个数组、一个元组。真实程序里，数据往往**成组出现并带着
行为**——一个「点」有 x 和 y，还知道怎么算到另一个点的距离。把它们绑在一起的东西就是**类**。

这一部分讲怎么造自己的类型，从最基本的类开始。

## 定义一个类

```z42
// examples/types/classes/define/define.z42
{{#include ../../../../examples/types/classes/define/define.z42:decl}}
```

用起来：

```z42
// examples/types/classes/define/define.z42
{{#include ../../../../examples/types/classes/define/define.z42:use}}
```

```console
{{#include ../../../../examples/types/classes/define/run.console:run}}
```

三样东西：

- **属性** `public double X { get; set; }`——对外看着像字段，读写都走它。
- **构造器** `public Point(...)`——名字必须和类同名，`new Point(3.0, 4.0)` 时执行。
- **方法**——类里的函数，能直接用 `X` / `Y`，不必写 `this.X`（写了也对）。

## 🔴 类是引用类型：赋值复制的是引用

这是从「值」转到「对象」时最容易栽的一跤：

```z42
// examples/types/classes/define/refsem.z42
{{#include ../../../../examples/types/classes/define/refsem.z42:code}}
```

```console
{{#include ../../../../examples/types/classes/define/run.console:refsem}}
```

`q = p` **没有复制对象**，只是让 `q` 和 `p` 指着同一个。改 `q.V` 就是改 `p.V`。

> 对比一下前面学过的：`int` / `double` / 元组是**值**，赋值复制内容；类是**引用**，
> 赋值复制指向。要「另一个独立的对象」，就得自己 `new` 一个。

输出第二行还说明一件事：**没重写 `ToString` 时，默认打印的是类名**。想让它有用就自己重写
（下面讲）。

## 字段与可见性

属性之外还有**字段**——直接的存储格子：

```z42
// examples/types/classes/fields/fields.z42
{{#include ../../../../examples/types/classes/fields/fields.z42:code}}
```

```console
{{#include ../../../../examples/types/classes/fields/run.console:run}}
```

- `public` 的成员外面能用，`private` 的只有类里面能用。
- **不写修饰符时默认是 `private`**——想让外面用就明确写 `public`。
- 只写 `get` 不写 `set` 就是**只读属性**：外面只能读。

什么时候用字段、什么时候用属性？**对外暴露的一律用属性**（将来想加检查或改算法，不用动调用
方的代码）；纯内部的存储用私有字段。

## 构造器

一个类可以有**多个**构造器，参数不同即可。想复用别的构造器，用 `: this(...)` 转调：

```z42
// examples/types/classes/ctor/ctor.z42
{{#include ../../../../examples/types/classes/ctor/ctor.z42:code}}
```

```console
{{#include ../../../../examples/types/classes/ctor/run.console:run}}
```

`: this(...)` 写在参数表后面、函数体前面，意思是「先去跑那个构造器，再跑我自己的体」。
这样默认值只写在一处。

## 属性的两种写法与索引器

```z42
// examples/types/classes/props/props.z42
{{#include ../../../../examples/types/classes/props/props.z42:code}}
```

```console
{{#include ../../../../examples/types/classes/props/run.console:run}}
```

- **自动属性** `{ get; set; }`——编译器帮你准备存储。
- **计算属性** `{ get { return ...; } }`——每次读都现算，**没有存储**。`Fahrenheit` 就是从
  `Celsius` 算出来的，不占地方也不会不同步。
- **索引器** `this[int i]`——让你的对象能用 `b[0]` 这种写法。前面 `List` 和 `Dictionary`
  的方括号就是这么来的。

完整规则（`init` 访问器、访问器各自的可见性等）见参考手册的
[属性与索引器](https://z42-lang.github.io/z42/reference/language/properties-indexers.html)。

## 建对象时顺手设值

```z42
// examples/types/classes/init/init.z42
{{#include ../../../../examples/types/classes/init/init.z42:code}}
```

```z42
// examples/types/classes/init/init.z42
{{#include ../../../../examples/types/classes/init/init.z42:use}}
```

```console
{{#include ../../../../examples/types/classes/init/run.console:run}}
```

- **对象初始化器** `new Config { Host = "...", Port = 8080 }`——先跑构造器，再按写的顺序赋值。
  没提到的字段保持构造器设的值（上面 `b` 的 `Host` 还是 `localhost`）。
- **`new()`**——左边已经写明类型时，`new` 后面的类名可以省掉。

## 静态成员：属于类，不属于对象

```z42
// examples/types/classes/statics/statics.z42
{{#include ../../../../examples/types/classes/statics/statics.z42:code}}
```

```console
{{#include ../../../../examples/types/classes/statics/run.console:run}}
```

`Total` 全类共用一份，`Mine` 每个对象各有一份。静态成员用**类名**访问（`Counter.Total`），
不需要先有对象。

## 重写 `ToString`

每个类都隐式继承 `Std.Object`，它带着 `ToString` / `Equals` / `GetHashCode` 三个方法。
默认的 `ToString` 只打类名（前面见过），通常值得重写：

```z42,ignore
public override string ToString() => $"Point({X}, {Y})";
```

`override` 的完整规则、以及 `Equals` / `GetHashCode` 要一起重写的道理，下一章讲继承时再说。

## 小结

- 类把**数据和行为**绑在一起；`new` 造对象，构造器负责初始化。
- **类是引用类型**——`q = p` 复制的是指向，不是对象。这点和值类型正好相反。
- 成员**默认 `private`**；对外暴露用属性，内部存储用私有字段。
- 多个构造器用 `: this(...)` 转调，默认值只写一处。
- 属性分**自动**（有存储）和**计算**（现算、无存储）；索引器让对象支持 `[ ]`。
- 对象初始化器 `new C { X = 1 }` 建完顺手赋值；类型已知时可写 `new()`。
- **静态成员属于类**，全类共用，用类名访问。
- 默认 `ToString` 只打类名，值得重写。

下一章讲**继承与多态**——让一个类在另一个类的基础上扩展。
