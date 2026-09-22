# 运算符与表达式

上一章讲了怎么存一个值，这一章讲怎么把值**算起来**：四则运算、比较、逻辑判断、位运算，
以及几个处理「可能是空」的写法。

## 算术

和你想的一样：

```z42
// examples/basics/operators/arith/basic.z42
{{#include ../../../../examples/basics/operators/arith/basic.z42:code}}
```

```console
{{#include ../../../../examples/basics/operators/arith/run.console:basic}}
```

### 整数相除得到整数

这是最容易栽的一处：**两边都是整数时，`/` 做的是整除，小数部分直接丢掉**。

```z42
// examples/basics/operators/arith/intdiv.z42
{{#include ../../../../examples/basics/operators/arith/intdiv.z42:code}}
```

```console
{{#include ../../../../examples/basics/operators/arith/run.console:intdiv}}
```

想要小数结果，让**至少一边是小数**：写 `17.0 / 5`，或把变量转一下 `(double)a / b`。

注意 `-17 / 5` 得到 `-3` 而不是 `-4`——**向零取整**。余数 `%` 的符号跟着被除数走。

### 整数会悄悄绕回去

超出类型范围时 z42 **不报错**，而是绕回另一头：

```z42
// examples/basics/operators/arith/overflow.z42
{{#include ../../../../examples/basics/operators/arith/overflow.z42:code}}
```

```console
{{#include ../../../../examples/basics/operators/arith/run.console:overflow}}
```

> ⚠️ 这是**运行期静默发生**的，编译器不会提醒。算出来的数可能很大时，用 `long`。

小数除法则不抛异常：`1 / 0.0` 得到 `inf`。但**整数除以 0 会抛异常**
（`DivideByZeroException`），程序直接中止。

### `+=` 与 `++`

```z42
// examples/basics/operators/arith/incr.z42
{{#include ../../../../examples/basics/operators/arith/incr.z42:code}}
```

```console
{{#include ../../../../examples/basics/operators/arith/run.console:incr}}
```

`n++` 放在表达式里时给出的是**旧值**，加 1 发生在之后。分不清就别把 `++` 写进更大的
表达式里——单独一行 `n++;` 永远不会有歧义。

> ⚠️ **`+=` 不做窄化检查。** `int x` 写 `x = x + 2.5;` 会被编译器拦下，但等价的
> `x += 2.5;` 能通过编译，到运行期才出问题。混用不同数值类型时写成显式的
> `x = x + (int)y;`。完整规则见参考手册的
> [运算符语义](https://z42-lang.github.io/z42/reference/language/operators.html)。

## 比较

`==` `!=` `<` `<=` `>` `>=`，结果都是 `bool`：

```z42
// examples/basics/operators/compare/compare.z42
{{#include ../../../../examples/basics/operators/compare/compare.z42:code}}
```

```console
{{#include ../../../../examples/basics/operators/compare/run.console:compare}}
```

字符串的 `==` 比的是**内容**，不是"是不是同一个对象"——这点和 C# 一致。

整数和小数可以直接比：`int i = 5;` 时 `i == 5.0` 是 `true`，`i < 5.5` 也是 `true`——
两边会先**加宽**到共同类型再比。`char` 和整数之间同理（`'A' == 65` 是 `true`）。

浮点数有个跨语言都一样的老问题：`0.1 + 0.2 == 0.3` 是 `false`，因为 `0.1 + 0.2`
实际算出来是 `0.30000000000000004`。**小数不要用 `==` 比**，改成判断差值够不够小。

> ⚠️ 同样的道理，`long` 很大时转成小数会丢精度：`9007199254740993L == 9007199254740992.0`
> 是 `true`（两个数加宽后变成了同一个 `double`）。要精确比较就别混用类型。

## 逻辑

`&&`（并且）、`||`（或者）、`!`（取反）。关键性质是**短路**：左边已经定了胜负时，
右边根本不求值。

```z42
// examples/basics/operators/compare/logic.z42
{{#include ../../../../examples/basics/operators/compare/logic.z42:code}}
```

```console
{{#include ../../../../examples/basics/operators/compare/run.console:logic}}
```

短路不只是省一点运算——它是一种常用的**保护写法**：`empty != null && empty[0] > 0`
里，左边为 `false` 时右边的索引操作完全不会发生，所以不会炸。顺序反过来就会炸。

## 位运算

直接操作二进制位，常用于标志位：

```z42
// examples/basics/operators/bits/bits.z42
{{#include ../../../../examples/basics/operators/bits/bits.z42:code}}
```

```console
{{#include ../../../../examples/basics/operators/bits/run.console:run}}
```

> 熟悉 C# 的读者请注意：**z42 的 `&` `|` `^` `~` 只接受整数，不能用在 `bool` 上。**
> C# 里 `a & b`（两个 `bool`）表示"不短路的逻辑与"，z42 直接报错：

```z42
// examples/basics/operators/boolbits/boolbits.z42
{{#include ../../../../examples/basics/operators/boolbits/boolbits.z42}}
```

```console
{{#include ../../../../examples/basics/operators/boolbits/run.console:err}}
```

确实需要"两边都求值"时，把右边先算进一个局部变量再用 `&&`。

## 处理"可能是空"

### 取不到就用备用值

```z42
// examples/basics/operators/nullish/coalesce.z42
{{#include ../../../../examples/basics/operators/nullish/coalesce.z42:code}}
```

```console
{{#include ../../../../examples/basics/operators/nullish/run.console:coalesce}}
```

> **z42 没有 `??`。** 别的语言里 `name ?? "(匿名)"` 一行就能兜底，代价和 `?.` 一样：
> 兜底动作被藏进运算符里，读代码的人看不见「这里其实可能是空」。写 `a ?? b`
> 会报 **E0480**，并告诉你改成上面的写法。
>
> 如果你在写的是「读一个设置，取不到用默认值」，更好的办法是让 API 直接收默认值——
> 比如 `Environment.GetEnvironmentVariable("HOME", "")`，一行搞定且结果保证非空。

### 取内容之前先判空

```z42
// examples/basics/operators/nullish/nullcheck.z42
{{#include ../../../../examples/basics/operators/nullish/nullcheck.z42:code}}
```

```console
{{#include ../../../../examples/basics/operators/nullish/run.console:nullcheck}}
```

判空就老老实实写出来。这样"哪里判过、哪里没判"一眼看得见。

> **z42 没有 `?.`。** 别的语言里 `missing?.Name` 能在为空时整体求值成 `null`，
> 看着省事，代价是**空值被悄悄往下传**——真正出错的地方离病因可能隔着好几层。
> z42 把这个口子去掉了：写 `n?.value` 会直接报错 **E0480**，并告诉你改成上面的写法。
>
> `?` 本身的含义见[上一章](variables.md)：它不是"这里可能是空"，而是
> **"请编译器在这里强制检查"**——不标的地方完全不受检，所以老代码一行都不用改。

### `?:`：三元条件

```z42
// examples/basics/operators/nullish/ternary.z42
{{#include ../../../../examples/basics/operators/nullish/ternary.z42:code}}
```

```console
{{#include ../../../../examples/basics/operators/nullish/run.console:ternary}}
```

读作"条件 ? 成立时的值 : 不成立时的值"。适合**在表达式里做二选一**；分支里要干好几件事
就该用 `if`（下一章）。

## 优先级

记两条就够日常用：

1. **算术比比较紧，比较比逻辑紧**——`a + 1 > b && c` 就是 `((a + 1) > b) && c`。
2. **位运算比比较松**——`x & 1 == 0` 不是你想的意思，它是 `x & (1 == 0)`。位运算和比较
   放一起时，**加括号**：`(x & 1) == 0`。

完整的优先级表见参考手册的
[运算符语义](https://z42-lang.github.io/z42/reference/language/operators.html)。拿不准就加括号，
没人会因为括号多批评你。

## 小结

- **整数 `/` 整数 = 整除**，要小数就让一边是小数；负数向零取整。
- 整数溢出**静默绕回**，不报错；整数除以 0 抛异常，小数除以 0 得 `inf`。
- `&&` `||` **短路**，可以拿来当保护条件用。
- `&` `|` `^` `~` **只接受整数**——这点比 C# 严。
- **没有 `??` 也没有 `?.`**——兜底和判空都要写出来；`?:` 三元照常可用。
- 位运算与比较混写时加括号。

下一章讲怎么让代码**分支和重复**。
