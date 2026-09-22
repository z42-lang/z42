# 值类型与记录

第 13 章讲过一条：**类是引用类型**，`q = p` 复制的是指向。这一章讲另一半——怎么造**值类型**，
以及怎么让编译器替你把「数据类」的样板代码写完。

## `struct`：值语义

把 `class` 换成 `struct`，语义就从「共享指向」变成「复制内容」：

```z42
// examples/types/structs-records/value/value.z42
{{#include ../../../../examples/types/structs-records/value/value.z42:decl}}
```

```z42
// examples/types/structs-records/value/value.z42
{{#include ../../../../examples/types/structs-records/value/value.z42:use}}
```

```console
{{#include ../../../../examples/types/structs-records/value/run.console:run}}
```

`b = a` 之后改 `b.X`，`a` 纹丝不动——和第 13 章那个类的例子正好相反。

`Equals` 也不一样：类比的是「是不是同一个对象」，**struct 比的是内容**（逐字段），
所以两个各自 `new` 出来的 `Point(1, 2)` 相等。

> struct 能写字段、方法、构造器、静态成员、运算符重载，也能实现接口。
> 但它**不参与继承**——没有基类，也不能被继承。

### 什么时候用 struct

小而不可变、代表「一个值」的东西：坐标、颜色、金额、时间点。
数据一大或者要共享修改，就用类——struct 每次赋值和传参都在复制。

## `[Record]`：一行生成一个数据类

只是想装几个字段、能比较、能打印？`[Record]` 把样板全包了：

```z42
// examples/types/structs-records/record/record.z42
{{#include ../../../../examples/types/structs-records/record/record.z42:decl}}
```

```z42
// examples/types/structs-records/record/record.z42
{{#include ../../../../examples/types/structs-records/record/record.z42:use}}
```

```console
{{#include ../../../../examples/types/structs-records/record/run.console:run}}
```

一行 `[Record] public class Person(string Name, int Age);` 换来四样东西：

- **主构造器**——括号里的参数直接成为字段，不用自己写赋值。
- **值相等**——`Equals` 逐字段比，两个内容相同的 `Person` 相等。
- **像样的 `ToString`**——`Person { Name = 小明, Age = 7 }`，不用自己拼。
- **`with` 表达式**——`p with { Age = 8 }` 得到一个**新**对象，原来的不变。

`[Record]` 也能加在 `struct` 上（`[Record] struct Vector2(double X, double Y);`），
那就同时拥有值语义和这套合成成员。

> ⚠️ `with` 目前**只支持 record class**，用在 record struct 上会报错
> （`with on a struct record is not yet supported`）。

## 🔴 两个坑

### 单字段 struct 没有值语义

值语义目前只对**两个及以上字段**的 struct 生效：

```z42
// examples/types/structs-records/gaps/onefield.z42
{{#include ../../../../examples/types/structs-records/gaps/onefield.z42}}
```

```console
{{#include ../../../../examples/types/structs-records/gaps/run.console:onefield}}
```

单字段那个 `a.X` 跟着 `b` 一起变了——**它表现得像引用类型**。这是当前实现的缺口，
不是设计意图。真需要一个字段的值类型时，加一个占位字段，或者先用类。

### 表达式体构造器 + 元组赋值会静默失效

```z42
// examples/types/structs-records/gaps/exprctor.z42
{{#include ../../../../examples/types/structs-records/gaps/exprctor.z42}}
```

```console
{{#include ../../../../examples/types/structs-records/gaps/run.console:exprctor}}
```

字段全是 `0`，**编译器不报错**。原因是 `(A, B) = (a, b)` 是第 12 章讲过的**解构声明**——
它声明了两个新的局部变量 `A` / `B`，跟字段没关系。struct 构造器请写花括号体：

```z42,ignore
public Pair(int a, int b) { A = a; B = b; }
```

## 打印 struct 的内容

自己写的 `ToString` 在插值、拼接、`.ToString()` 上都生效（上面第一个例子就用的插值）。
**但 `Console.WriteLine(s)` 例外**——它对 struct、类、record 一律打 `类型名{...}`，
不走 `ToString`：

```z42,ignore
Console.WriteLine(p);              // Point{...}
Console.WriteLine($"{p}");         // (1, 2)   ← 用插值
Console.WriteLine(p.ToString());   // (1, 2)   ← 或显式调用
```

## 三者怎么挑

| 想要 | 用 |
|------|-----|
| 共享同一个对象、会变、数据多 | **`class`** |
| 一个小的「值」，复制语义 | **`struct`** |
| 装几个字段，要相等 / 打印 / 复制改一处 | **`[Record]`**（加在 class 或 struct 上）|

## 小结

- **`struct` 是值类型**：赋值和传参复制内容，`Equals` 逐字段比；**不参与继承**。
- `[Record]` 一行合成主构造器、值相等、`ToString`、`with`。
- ⚠️ `with` 只支持 record **class**。
- 🔴 **单字段 struct 仍表现为引用语义**——当前实现缺口。
- 🔴 **表达式体构造器 + 元组赋值静默失效**，字段全 0；构造器写花括号体。
- `Console.WriteLine(s)` 不走 `ToString`，用插值或显式 `.ToString()`。

下一章讲**枚举与模式匹配**——把「这个值是哪一种」写得更直接。
