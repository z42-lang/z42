# 枚举与模式匹配

前面四章讲的都是**怎么造**类型。这一章讲怎么**用**它们——当一个值可能是几种情形之一时，
怎么把「是哪一种」和「里面装了什么」一次问清楚。

先认识 `enum`（把「几种情形」写成一个类型），再认识 **模式匹配**（把「是哪一种、里面是什么」
写成一个表达式）。

## `enum`：一个值只能是这几种

```z42
// examples/types/patterns/enums/enums.z42
{{#include ../../../../examples/types/patterns/enums/enums.z42:decl}}
```

成员用 `类型名.成员名` 引用，底层是从 0 开始的整数，可以显式转换：

```z42
// examples/types/patterns/enums/enums.z42
{{#include ../../../../examples/types/patterns/enums/enums.z42:use}}
```

```console
{{#include ../../../../examples/types/patterns/enums/run.console:run}}
```

插值打印出来的是**成员名**（`East`），不是数字。

> 熟悉 C# 的读者请注意：z42 的 `enum` 是**独立类型**，不会和整数隐式互换——
> `d == 1` 不合法，要比较得先 `(int)d`。

## `switch`：从语句到表达式

`switch` 有两副面孔。语句形态和大多数语言一样，每个分支自己 `break` 或 `return`：

```z42
// examples/types/patterns/enums/enums.z42
{{#include ../../../../examples/types/patterns/enums/enums.z42:stmt}}
```

**表达式形态**直接求出一个值——`switch` 写在被判断的值后面，臂用 `=>`，整体可以赋给变量
或者直接 `return`：

```z42
// examples/types/patterns/enums/enums.z42
{{#include ../../../../examples/types/patterns/enums/enums.z42:expr}}
```

两者用的是**同一套模式**，下面讲的每种写法在两边都能用。表达式形态更常用：它逼着你为每种
情形给出一个值，读起来也更像「这个值映射到那个值」。

## 🔴 穷尽性：编译器会提醒，但不会替你兜底

漏掉一种情形，编译器会警告：

```z42
// examples/types/patterns/exhaust/missing.z42
{{#include ../../../../examples/types/patterns/exhaust/missing.z42}}
```

```console
{{#include ../../../../examples/types/patterns/exhaust/run.console:missing}}
```

两件事要看清楚：

1. **`W0700` 是警告，不是错误**——程序照样编译、照样运行。
2. **运行到没有匹配臂的情形时，`switch` 表达式静默产出 `null`**。上面最后一行打的是
   `[null]`，而 `shortOf` 的返回类型明明是 `string`。

第二条很伤人：拿到的 `null` 会一路往下流，可能在很远的地方才炸。**穷尽性要靠你自己保证**
——要么补齐所有情形，要么加一条 `_ =>` 兜底臂。

> ⚠️ 这条对数值类型更危险：`int a = n switch { 1 => 10, 2 => 20 };` 在 `n` 是别的值时，
> `a` 拿到的不是 0，是个**垃圾值**。别指望它。

穷尽性检查不只管 `enum`。`bool` 要覆盖 `true` / `false`；**类层次**只要基类没标 `public`，
编译器就认为子类的集合是封闭的，也会检查：

```z42
// examples/types/patterns/exhaust/closed.z42
{{#include ../../../../examples/types/patterns/exhaust/closed.z42}}
```

```console
{{#include ../../../../examples/types/patterns/exhaust/run.console:closed}}
```

抽象基类本身不算在内——一个值的实际类型总是某个具体子类。

## 结构化模式：一次问清「是哪一种、里面是什么」

模式不止能比常量。对 `[Record]` 类型（第 16 章），可以按主构造器的**声明顺序**直接拆开：

```z42
// examples/types/patterns/destructure/destructure.z42
{{#include ../../../../examples/types/patterns/destructure/destructure.z42:types}}
```

```z42
// examples/types/patterns/destructure/destructure.z42
{{#include ../../../../examples/types/patterns/destructure/destructure.z42:positional}}
```

这几行里有四样东西：

- `Point(0, 0)` —— 子模式可以是**常量**，两个字段都得对上。
- `Point(0, y)` —— 常量和**绑定**可以混用：先验 `X == 0`，再把 `Y` 绑给新变量 `y`。
- `Point(x, y)` —— 两个都绑定，等于「拆开这个 record」。
- `if x > 0` —— **守卫**。模式本身匹配上了，还要再过这个条件。

臂**从上往下试**，第一个匹配的生效——所以特殊情形写前面，一般情形写后面。

> 不需要写任何 `Deconstruct` 方法：位置到字段的映射由 record 的主构造器直接提供。

### 按字段名匹配

不想数位置、或者只关心其中一两个字段，用**属性模式**（花括号 + 字段名）。类型可以省，
省了就是「被判断的那个值的类型」：

```z42
// examples/types/patterns/destructure/destructure.z42
{{#include ../../../../examples/types/patterns/destructure/destructure.z42:property}}
```

### 嵌套

子模式本身还可以是模式，随便嵌多深：

```z42
// examples/types/patterns/destructure/destructure.z42
{{#include ../../../../examples/types/patterns/destructure/destructure.z42:nested}}
```

这里顺带出现了第三个用模式的地方——**`is` 表达式**。它和 `switch` 臂用同一套模式，
匹配成功时绑定的变量在 `true` 分支里可见。

```console
{{#include ../../../../examples/types/patterns/destructure/run.console:use}}
```

## 组合子：范围、或、以及「整体加局部」

数值和字符可以直接写**范围**（`..=` 含两端）和**关系**（`>` `>=` `<` `<=`）：

```z42
// examples/types/patterns/combinators/combinators.z42
{{#include ../../../../examples/types/patterns/combinators/combinators.z42:scalar}}
```

`|` 把几个模式并成一个——**任一匹配即匹配**。最有用的形态是「多个变体，同一种处理」，
各个分支还可以绑定同名变量：

```z42
// examples/types/patterns/combinators/combinators.z42
{{#include ../../../../examples/types/patterns/combinators/combinators.z42:types}}
```

```z42
// examples/types/patterns/combinators/combinators.z42
{{#include ../../../../examples/types/patterns/combinators/combinators.z42:orbind}}
```

> 各个分支必须绑定**完全一样的一组变量**，同名的类型也要一致——否则臂里的代码不知道
> 自己拿到的是什么。不一致编译器会报错。

有时候既想要拆出来的字段，又想要**整个值**。`名字 @ 模式` 两个一起给：

```z42
// examples/types/patterns/combinators/combinators.z42
{{#include ../../../../examples/types/patterns/combinators/combinators.z42:at}}
```

`whole` 的类型是 `Square`（不是 `Shape`），所以能直接传给收 `Square` 的函数。

```console
{{#include ../../../../examples/types/patterns/combinators/run.console:run}}
```

## 解构声明：不想要 `switch` 的时候

只是想把一个 record 拆成几个局部变量，不需要分支，那就别写 `switch`——直接写：

```z42
// examples/types/patterns/destructure/decl.z42
{{#include ../../../../examples/types/patterns/destructure/decl.z42:code}}
```

```console
{{#include ../../../../examples/types/patterns/destructure/run.console:decl}}
```

这是模式的第四个用武之地。因为没有「不匹配」这个出口，它只收**一定能成功**的模式：
通配 `_`、绑定、嵌套的位置/属性模式；常量、范围、类型测试这些**可能失败**的一律不收。

> ⚠️ `(A, B) = (a, b)` 这种写法**不是赋值**。它声明两个新的局部变量；如果名字撞上了
> 当前类的字段，字段一个都不会动（编译器会警告）。给字段赋值就老老实实写
> `this.A = a;`。

## 小结

- `enum` 把「几种情形」写成一个类型；它是**独立类型**，不和整数隐式互换。
- `switch` 有语句和**表达式**两副面孔，共用同一套模式；表达式形态更常用。
- 🔴 **穷尽性只是警告**。漏掉的情形在运行期**静默给出 `null` 或垃圾值**——自己补齐，
  或者加 `_ =>` 兜底。
- 模式能做的事：常量、类型、**按位置拆 record**、**按字段名拆**、嵌套、守卫 `if`。
- 组合子：范围 `..=`、关系 `>`、或 `|`（可带绑定，多变体同处理）、`名字 @ 模式`。
- 用模式的四个地方：`switch` 语句、`switch` 表达式、`is` 表达式、**解构声明**。

完整的模式文法、各组合子的边界与诊断，见
[模式匹配](https://z42-lang.github.io/z42/reference/language/pattern-matching.html)
与[枚举](https://z42-lang.github.io/z42/reference/language/enums.html)。
