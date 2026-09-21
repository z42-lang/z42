# 控制流

到目前为止代码都是从上往下一行行跑。这一章讲怎么让它**分岔**和**重复**。

## `if` / `else`

```z42
// examples/basics/control-flow/ifelse/ifelse.z42
{{#include ../../../../examples/basics/control-flow/ifelse/ifelse.z42:code}}
```

```console
{{#include ../../../../examples/basics/control-flow/ifelse/run.console:run}}
```

条件必须是 `bool`——**不能**像 C 那样写 `if (n)` 表示"n 非零"。要判非零就写 `if (n != 0)`。

`else if` 可以接任意多个，从上往下第一个成立的分支执行，其余都跳过。

## `while`

条件成立就一直重复：

```z42
// examples/basics/control-flow/loops/while.z42
{{#include ../../../../examples/basics/control-flow/loops/while.z42:code}}
```

```console
{{#include ../../../../examples/basics/control-flow/loops/run.console:while}}
```

⚠️ 循环体里**一定要有能让条件变假的东西**（上面是 `n = n + 1`）。忘了写就是死循环。

## `do` / `while`

区别只有一个：**先跑一次再判断**，所以循环体至少执行一次。

```z42
// examples/basics/control-flow/loops/dowhile.z42
{{#include ../../../../examples/basics/control-flow/loops/dowhile.z42:code}}
```

```console
{{#include ../../../../examples/basics/control-flow/loops/run.console:dowhile}}
```

`k` 一开始就是 `99`，`k < 3` 从来没成立过，但那行还是打出来了。注意末尾的**分号**。

## `for`

把"初始化、继续条件、每轮末尾做什么"三件事写在一行：

```z42
// examples/basics/control-flow/loops/for.z42
{{#include ../../../../examples/basics/control-flow/loops/for.z42:code}}
```

```console
{{#include ../../../../examples/basics/control-flow/loops/run.console:for}}
```

`i` 只在循环里可见，出了循环就没了。计数循环用 `for`，其它情况用 `while` 更清楚。

## `foreach`：逐个取出集合里的元素

不关心下标时用它，比 `for` + 下标短也更不容易写错：

```z42
// examples/basics/control-flow/foreach/arrays.z42
{{#include ../../../../examples/basics/control-flow/foreach/arrays.z42:code}}
```

```console
{{#include ../../../../examples/basics/control-flow/foreach/run.console:arrays}}
```

字符串也可以直接 `foreach`，一次取一个字符：

```z42
// examples/basics/control-flow/foreach/chars.z42
{{#include ../../../../examples/basics/control-flow/foreach/chars.z42:code}}
```

```console
{{#include ../../../../examples/basics/control-flow/foreach/run.console:chars}}
```

> ⚠️ 但 `Dictionary` **不能**直接 `foreach`，要用 `dict.Keys()` 或 `dict.Entries()`。
> 完整规则（`foreach` 到底按什么顺序挑遍历方式）见参考手册的
> [迭代](https://z42-lang.github.io/z42/reference/language/iteration.html)。

## `break` 与 `continue`

- `continue`——**这一轮不往下走了**，直接进入下一轮。
- `break`——**整个循环不要了**，跳出去。

```z42
// examples/basics/control-flow/jumps/breakcontinue.z42
{{#include ../../../../examples/basics/control-flow/jumps/breakcontinue.z42:code}}
```

```console
{{#include ../../../../examples/basics/control-flow/jumps/run.console:breakcontinue}}
```

两者都只作用于**最近一层**循环。

### 跳出多层循环

z42 **没有 `goto`，也没有循环标签**。想跳出外层循环，用一个标志变量：

```z42
// examples/basics/control-flow/jumps/nested.z42
{{#include ../../../../examples/basics/control-flow/jumps/nested.z42:code}}
```

```console
{{#include ../../../../examples/basics/control-flow/jumps/run.console:nested}}
```

另一个常用做法是把内层循环抽成一个方法，找到就直接 `return`（下一章讲函数）。

## `switch`：多路分支

`if / else if` 链太长时用 `switch` 更整齐：

```z42
// examples/basics/control-flow/switch/basic.z42
{{#include ../../../../examples/basics/control-flow/switch/basic.z42:code}}
```

```console
{{#include ../../../../examples/basics/control-flow/switch/run.console:basic}}
```

> 熟悉 C# 的读者请注意：**`break` 可以不写，而且没有 fallthrough。**
> 每个 `case` 体执行完就自动跳出 `switch`，不会继续掉进下一个 `case`。
> 上面只打印了"向南"。写 `break` 也合法，只是不再必需。

### `default` 写在哪都行

`default` **只在所有 `case` 都不匹配时才走**，和它写在第几行无关：

```z42
// examples/basics/control-flow/switch/defaultlast.z42
{{#include ../../../../examples/basics/control-flow/switch/defaultlast.z42:code}}
```

```console
{{#include ../../../../examples/basics/control-flow/switch/run.console:defaultlast}}
```

`default` 写在最前面，命中的仍是 `case 2`。惯例是把它写在最后——读起来更顺——但那只是风格。

### `switch` 里的 `continue` 作用于外层循环

`break` 在 `switch` 里是"跳出 switch"，但 `continue` **穿透 switch**，作用于外面那层循环：

```z42
// examples/basics/control-flow/jumps/switchcontinue.z42
{{#include ../../../../examples/basics/control-flow/jumps/switchcontinue.z42:code}}
```

```console
{{#include ../../../../examples/basics/control-flow/jumps/run.console:switchcontinue}}
```

`i=2` 那一轮被 `continue` 跳过了，所以只打印了 `i=1` 和 `i=3`。

### `case` 还能写得更复杂

`case` 后面可以是范围、类型、带条件的模式，`switch` 也能当**表达式**用来直接产出一个值。
这些留到「枚举与模式匹配」那一章讲；完整规则见参考手册的
[控制流](https://z42-lang.github.io/z42/reference/language/control-flow.html)。

## 小结

- 条件必须是 `bool`，`if (n)` 这种写法不成立。
- `while` 先判断，`do/while` **至少跑一次**（别忘末尾分号）。
- 计数用 `for`，逐元素用 `foreach`。
- 字符串可以直接 `foreach`（一次一个字符）；**`Dictionary` 不行**，用 `Keys()` / `Entries()`。
- `break` / `continue` 只管最近一层；**没有 `goto`、没有循环标签**，跳多层用标志变量或抽成方法。
- `switch` **没有 fallthrough**，`break` 可省；`default` 写在哪都行，只在所有 `case` 都不匹配时才走。

下一章讲怎么把代码切成**函数**。
