# 接口

上一章的继承说的是「**是什么**」——狗是一种动物。接口说的是「**能做什么**」——这个东西能算
面积、能画出来、能比较大小。

区别在于：继承只能有一个基类，接口**想实现几个就几个**。

## 声明与实现

```z42
// examples/types/interfaces/basic/basic.z42
{{#include ../../../../examples/types/interfaces/basic/basic.z42:decl}}
```

```z42
// examples/types/interfaces/basic/basic.z42
{{#include ../../../../examples/types/interfaces/basic/basic.z42:use}}
```

```console
{{#include ../../../../examples/types/interfaces/basic/run.console:run}}
```

- 接口里**只有签名，没有体**——和 `abstract` 方法一样。
- 接口成员**不写访问修饰符**；但实现方的成员**必须是 `public`**。
- `Circle` 和 `Rect` 毫无血缘关系，却能放进同一个 `IShape[]`——这是接口比继承灵活的地方。

### 漏实现会被拦下

```z42
// examples/types/interfaces/basic/missing.z42
{{#include ../../../../examples/types/interfaces/basic/missing.z42}}
```

```console
{{#include ../../../../examples/types/interfaces/basic/run.console:missing}}
```

签名对不上（参数类型或个数不同）也报同一个错。

## 一个类可以实现多个接口

```z42
// examples/types/interfaces/multi/multi.z42
{{#include ../../../../examples/types/interfaces/multi/multi.z42:code}}
```

```console
{{#include ../../../../examples/types/interfaces/multi/run.console:run}}
```

逗号分隔，想加几个加几个。**有基类时基类写在最前面**：`class C : Base, IFoo, IBar`。

> 这正是接口存在的理由：`Circle` 既是「有面积的」也是「能画的」，但它只能有一个基类。

## 问一句「你能做什么」

拿到一个 `IShape`，想知道它顺便还能不能画：

```z42
// examples/types/interfaces/check/check.z42
{{#include ../../../../examples/types/interfaces/check/check.z42:code}}
```

```console
{{#include ../../../../examples/types/interfaces/check/run.console:run}}
```

- **`s is IDrawable`** 问「它实现了这个接口吗」，结果是 `bool`。
- **`s as IDrawable`** 转过去；转不了时得到 `null`（不是抛异常）。

## 接口也能要求属性

```z42
// examples/types/interfaces/mutable/mutable.z42
{{#include ../../../../examples/types/interfaces/mutable/mutable.z42:code}}
```

```console
{{#include ../../../../examples/types/interfaces/mutable/run.console:run}}
```

`{ get; set; }` 时，**getter 和 setter 是两份独立契约**——只实现一半照样报 `E0412`。
接口还能要求索引器（`int this[int i] { get; set; }`）和事件。

## 接口继承接口

`interface IDerived : IBase` 的**成员继承是生效的**——继承来的成员经 `IDerived` 照样能调，
多层继承与菱形继承都一样：

```z42
// examples/types/interfaces/gap/gap.z42
{{#include ../../../../examples/types/interfaces/gap/gap.z42}}
```

```console
{{#include ../../../../examples/types/interfaces/gap/run.console:run}}
```

### 🔴 仍缺的一半：接口之间不成立赋值关系

**类**到它任一祖先接口的赋值没问题（上面的 `IBase b = new Impl();`）。缺的是**接口到父接口**
这一步：

```z42,ignore
IDerived d = new Impl();
IBase b = d;      // ✗ E0402: cannot assign IDerived to IBase
```

接口类型之间的赋值判定目前只比较名字是否相等，不走继承链。**变通办法**：需要 `IBase`
视图时从具体类型赋值（`IBase b = impl;`），而不是从 `IDerived` 转。

## 接口还是抽象类？

两者都能表达「子类型必须提供这些东西」，挑法：

| 想要 | 用 |
|------|-----|
| 只约定**能做什么**，不带任何实现 | **接口** |
| 要提供**共享的实现**或字段 | **抽象类** |
| 一个类型要同时满足**多组**约定 | **接口**（抽象类只能有一个） |

拿不准时先用接口——它限制最少。

## 小结

- 接口只有签名没有体；实现方的成员**必须 `public`**，漏实现报 `E0412`。
- **一个类能实现任意多个接口**，有基类时基类写最前面。
- `x is IFoo` 判断，`x as IFoo` 转换（失败得 `null`）。
- 接口可以要求属性 / 索引器 / 事件；`{ get; set; }` 是**两份**契约。
- **接口可以继承接口**，继承来的成员经子接口照样能调；🔴 缺的只有**接口到父接口的赋值**
  （`IBase b = d;` 报 E0402），需要 `IBase` 视图时从具体类型赋值。
- 只约定能力用接口，要共享实现用抽象类。

下一章讲**值类型与记录**——`struct` 的值语义，以及 `[Record]` 怎么一行生成一个数据类。
