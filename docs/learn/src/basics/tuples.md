# 元组

要把**两三个值临时凑在一起**——比如一个函数想同时返回「成功没有」和「结果是多少」——
专门定义一个类太重了。元组就是为这个准备的。

第 9 章讲函数时已经用过它做多返回值，这一章补全规则。

## 写法

```z42
// examples/basics/tuples/basic/basic.z42
{{#include ../../../../examples/basics/tuples/basic/basic.z42:code}}
```

```console
{{#include ../../../../examples/basics/tuples/basic/run.console:run}}
```

- 类型写成 `(int, string)`，值写成 `(7, "hi")`，**形状一样**。
- 取值按位置：`Item1`、`Item2`……**从 1 开始数**，不是 0。
- 元组是**值类型**，赋值是复制，不涉及堆分配。

> ⚠️ **元素不能起名字**。`(int x, int y)` 这种写法不被接受，只能靠 `Item1` / `Item2` 认位置。
> 位置多到记不住时，说明该定义一个类了。

### 最多 8 个元素

```z42
// examples/basics/tuples/basic/toobig.z42
{{#include ../../../../examples/basics/tuples/basic/toobig.z42}}
```

```console
{{#include ../../../../examples/basics/tuples/basic/run.console:toobig}}
```

## 一次返回多个值

这是元组最常见的用途：

```z42
// examples/basics/tuples/multi/multi.z42
{{#include ../../../../examples/basics/tuples/multi/multi.z42:decl}}
```

```z42
// examples/basics/tuples/multi/multi.z42
{{#include ../../../../examples/basics/tuples/multi/multi.z42:call}}
```

```console
{{#include ../../../../examples/basics/tuples/multi/run.console:run}}
```

> ⚠️ **解构前面不加 `var`**——写 `(ok, len) = TryLength(...)`，这一句同时声明了 `ok` 和 `len`。
> 加了 `var` 反而不对。

## 解构

把元组拆成几个变量，按**形状**对应：

```z42
// examples/basics/tuples/destructure/destructure.z42
{{#include ../../../../examples/basics/tuples/destructure/destructure.z42:code}}
```

```console
{{#include ../../../../examples/basics/tuples/destructure/run.console:run}}
```

元组可以**套元组**，解构时照着形状写括号就行；也可以逐层用 `ItemN` 取。

## 在 `switch` 和 `is` 里当模式用

元组的形状可以直接当**模式**，某一位写常量就是「这一位必须等于它」：

```z42
// examples/basics/tuples/patterns/patterns.z42
{{#include ../../../../examples/basics/tuples/patterns/patterns.z42:code}}
```

```console
{{#include ../../../../examples/basics/tuples/patterns/run.console:run}}
```

`_` 表示「这一位不关心」。模式的完整规则（类型模式、关系模式、守卫）留到「枚举与模式匹配」
那一章，也可查参考手册的
[模式匹配](https://z42-lang.github.io/z42/reference/language/pattern-matching.html)。

## 什么时候别用元组

元组胜在轻，也正因为轻，**超过两三个值、或者要传得很远时就该定义一个类**——
`Item1` / `Item2` 读起来没有 `user.Name` / `user.Age` 清楚。判断标准很简单：
**你自己会不会数错位置。**

## 小结

- 类型 `(int, string)`、值 `(7, "hi")`，取值 `Item1` / `Item2`，**从 1 开始**。
- **元素不能起名字**；最多 **8** 个元素。
- 多返回值用它最合适；**解构前面不加 `var`**。
- 可以嵌套，解构与 `ItemN` 都能逐层用。
- 能在 `switch` / `is` 里当模式，`_` 表示不关心这一位。
- 值多了就定义类——别让读者数位置。

语言基础到这里就结束了。下一部分进入**类型与抽象**，从类与对象开始。
