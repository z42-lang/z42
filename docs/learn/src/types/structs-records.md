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

## 🔴 一个坑

### 值语义与字段数无关（一个字段也是值语义）

```z42
// examples/types/structs-records/gaps/onefield.z42
{{#include ../../../../examples/types/structs-records/gaps/onefield.z42}}
```

```console
{{#include ../../../../examples/types/structs-records/gaps/run.console:onefield}}
```

两边都是「改副本不动原值」——**字段数不影响语义**。

> 📜 **2026-09-26 之前这里是坏的**：单字段 struct 走的是引用模型，上面那个 `a.X` 会跟着
> `b` 一起变成 50（值语义只对两个及以上字段生效）。当时的建议是「加一个占位字段，或者先用类」
> —— 现在不需要了。

### 表达式体构造器里的元组赋值

```z42
// examples/types/structs-records/gaps/exprctor.z42
{{#include ../../../../examples/types/structs-records/gaps/exprctor.z42}}
```

```console
{{#include ../../../../examples/types/structs-records/gaps/run.console:exprctor}}
```

这行读起来完全像在给两个字段赋值，其实一个字段都没动——`(A, B)` 在表达式位置是**造一个
新元组**，赋给它等于扔掉。z42 只有**解构声明**（第 12 章），没有「解构赋值」；而解构声明是
语句，`=>` 后面只能放表达式，所以这里连解构声明都不是。

构造器写花括号体，逐个赋值：

```z42,ignore
public Pair(int a, int b) { A = a; B = b; }
```

> 这个坑此前是**静默**的：字段全 `0`，编译器一声不吭。E0482 是写这本书时补上的——
> 赋值给非左值以前完全没有检查，连 `42 = a;` 都能编过能跑。

## 打印 struct 的内容

自己写的 `ToString` 在**每一条**路上都生效 —— 插值、拼接、`.ToString()`、`Console.WriteLine`
给的是同一个答案：

```z42,ignore
Console.WriteLine(p);              // (1, 2)
Console.WriteLine($"{p}");         // (1, 2)
Console.WriteLine("p = " + p);     // p = (1, 2)
Console.WriteLine(p.ToString());   // (1, 2)
```

没写 `ToString` 的类型，四条路一律打**短类型名**（`Point`）。
⚠️ 别在 `ToString` 里拼接 `this`（`=> "P" + this`）—— 那会无限递归。

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
- **字段数不影响值语义**：一个字段的 struct 也是复制（2026-09-26 起）。
- ⚠️ **表达式体构造器里 `(A, B) = (a, b)` 不是赋值**（报 E0482）；构造器写花括号体。
- 自己写的 `ToString` 在插值 / 拼接 / `WriteLine` / 显式调用四条路上**答案一致**。

下一章讲**枚举与模式匹配**——把「这个值是哪一种」写得更直接。
