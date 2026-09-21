# 函数

前三章的代码都挤在 `Main` 里。程序一长就该切开——**函数**就是切开的单位：给一段代码起个
名字，说清它要什么、给什么，别处就能反复用它。

## 声明一个函数

写法是「返回类型 名字(参数表) { 函数体 }」——和你已经写过无数遍的 `void Main()` 一模一样，
`Main` 本来就是个普通函数，只不过 z42 拿它当入口。

```z42
// examples/basics/functions/declare/declare.z42
{{#include ../../../../examples/basics/functions/declare/declare.z42:free}}
```

在 `Main` 里用它们：

```z42
{{#include ../../../../examples/basics/functions/declare/declare.z42:call}}
```

```console
{{#include ../../../../examples/basics/functions/declare/run.console:free}}
```

三件事值得单独点出来：

- **函数写在文件顶层**，不必包在类里。这种叫**自由函数**。
- **`=> 表达式;` 是 `{ return 表达式; }` 的简写**，只有一句话时用它。叫**表达式体**。
- **不返回值就写 `void`**。`void` 函数里 `return;` 可以直接结束，也可以不写。

> 熟悉 C# 或 Java 的读者请注意：z42 的自由函数是真的自由，不需要 `static class Helpers`
> 那种包装类。

## 写在类里的函数叫方法

签名语法完全一样，只是放进了类里，调用时要带上类名。

```z42
// examples/basics/functions/declare/method.z42
{{#include ../../../../examples/basics/functions/declare/method.z42:code}}
```

```console
{{#include ../../../../examples/basics/functions/declare/run.console:method}}
```

`Show` 写了两个同名版本，参数类型不同——这叫**重载**。调用时编译器按你给的实参类型挑一个。
类本身留到第三部分细讲，这里只需要知道方法和函数是一回事。

### ⚠️ 自由函数不能重载

上面的 `Show` 能有两个版本，是因为它在类里。**顶层的自由函数同名就是重复声明，
和参数类型无关**：

```z42
// examples/basics/functions/declare/dup.z42
{{#include ../../../../examples/basics/functions/declare/dup.z42}}
```

```console
{{#include ../../../../examples/basics/functions/declare/run.console:dup}}
```

要么起不同的名字，要么把它们放进一个类。

## 默认值

参数可以带默认值，调用时不给就用默认的。

```z42
// examples/basics/functions/defaults/defaults.z42
{{#include ../../../../examples/basics/functions/defaults/defaults.z42:code}}
```

调用时给不给 `prefix` 都行：

```z42
{{#include ../../../../examples/basics/functions/defaults/defaults.z42:call}}
```

```console
{{#include ../../../../examples/basics/functions/defaults/run.console:run}}
```

**把带默认值的参数排在最后**。z42 目前不强制这一点（`F(int a = 1, int b)` 能编译），
但可选参夹在中间时，按位置调用没法跳过它——只能靠下面的命名实参。

## 命名实参

实参可以按**名字**传，而不是按位置。

```z42
// examples/basics/functions/named/named.z42
{{#include ../../../../examples/basics/functions/named/named.z42:decl}}
```

调用点的四种写法：

```z42
{{#include ../../../../examples/basics/functions/named/named.z42:call}}
```

```console
{{#include ../../../../examples/basics/functions/named/run.console:run}}
```

两个用处：**跳过中间的可选参**（`Box(depth: 10)`），以及**让调用点自己说明白**——
`Box(5, 6, 7)` 三个数字谁是谁全靠记，`Box(width: 5, height: 6, depth: 7)` 一眼就懂。

规则是位置实参必须在前、命名实参在后；命名实参之间可以乱序。完整规则见参考手册的
[命名实参](https://z42-lang.github.io/z42/reference/language/named-arguments.html)。

## `params`：接收任意多个实参

在最后一个参数前写 `params`（它的类型必须是数组），调用方就能散着传任意多个值。

```z42
// examples/basics/functions/variadic/variadic.z42
{{#include ../../../../examples/basics/functions/variadic/variadic.z42:decl}}
```

调用时想传几个传几个：

```z42
{{#include ../../../../examples/basics/functions/variadic/variadic.z42:call}}
```

```console
{{#include ../../../../examples/basics/functions/variadic/run.console:run}}
```

散着传时，编译器把它们打包成一个数组交给方法；直接传数组也接受。两条硬规则：

- **`params` 必须是最后一个参数**：

  ```z42
  // examples/basics/functions/variadic/notlast.z42
  {{#include ../../../../examples/basics/functions/variadic/notlast.z42}}
  ```

  ```console
  {{#include ../../../../examples/basics/functions/variadic/run.console:notlast}}
  ```

- **`params` 不能同时带默认值或 `ref` / `out` / `in`**（报 `E0208`）。

## 让函数改到调用方的变量：`ref` / `out`

参数默认是**按值**传的——方法拿到的是一份副本，改它不影响调用方。要改到调用方的变量本身，
用 `ref`（进出都要）或 `out`（只出）。

```z42
// examples/basics/functions/byref/byref.z42
{{#include ../../../../examples/basics/functions/byref/byref.z42:decl}}
```

调用点也要写修饰符：

```z42
{{#include ../../../../examples/basics/functions/byref/byref.z42:call}}
```

```console
{{#include ../../../../examples/basics/functions/byref/run.console:run}}
```

`out var n` 是就地声明一个新变量接住输出，很常用——不必先 `int n;` 再传。

还有一个 `in`，意思是「按引用传进来，但我不改它」，用于传很大的值时省掉复制。

### 🔴 修饰符目前没有编译期检查，写错不报错

这是本章最该记住的一条。z42 当前把 `ref` / `out` / `in` 塌缩成了同一个标志，
**调用点和声明处对不上时，编译器一声不吭**，而且两个方向的后果相反：

```z42
// examples/basics/functions/byref/forgot.z42
{{#include ../../../../examples/basics/functions/byref/forgot.z42}}
```

```console
{{#include ../../../../examples/basics/functions/byref/run.console:forgot}}
```

- **漏写 `ref`**（声明有、调用点没有）：编译通过，方法里的写入**静默丢失**，
  `a` 还是 0，没有任何运行期迹象。
- **多写 `ref`**（声明没有、调用点有）：编译通过，而 `b` **真的被改成了 999**——
  也就是说，**光看函数声明判断不出一个参数会不会被改**。

同理，`out` 参数不赋值能编译（调用方读到的是空值），`in` 参数在方法里被赋值也能编译。
**在这些检查补上之前，调用点自己对齐修饰符**，并把 `out` / `in` 当成给人看的标注，
不是编译器给的保证。完整的缺口清单见参考手册的
[参数修饰符](https://z42-lang.github.io/z42/reference/language/parameter-modifiers.html)。

## 返回多个值：用元组

需要一次返回两三个值时，**别用 `out`，用元组**——它没有上面那些缺口。

```z42
// examples/basics/functions/multi/multi.z42
{{#include ../../../../examples/basics/functions/multi/multi.z42:decl}}
```

接住返回值有两种写法：

```z42
{{#include ../../../../examples/basics/functions/multi/multi.z42:call}}
```

```console
{{#include ../../../../examples/basics/functions/multi/run.console:run}}
```

两点容易踩：

- **解构前面不加 `var`**，写 `(ok, len) = f();`，这一句同时声明了 `ok` 和 `len`。
- **元组的元素没有名字**，`(bool ok, int v) F()` 这种写法不被接受；整个接住时按
  `Item1` / `Item2` 取。

元组还会在第二部分后面单独讲一章，完整规则见参考手册的
[元组](https://z42-lang.github.io/z42/reference/language/tuples.html)。

## 局部函数与递归

函数体内还能再声明函数，只在外层函数里可见，并且**能直接用外层的局部变量**。

```z42
// examples/basics/functions/local/local.z42
{{#include ../../../../examples/basics/functions/local/local.z42:decl}}
```

适合那种「只有这一个方法需要、抽成顶层函数反而碍眼」的小工具。

函数也可以调用自己，这叫**递归**。写递归只有一条纪律：**先写出口**。

```z42
// examples/basics/functions/local/local.z42
{{#include ../../../../examples/basics/functions/local/local.z42:recur}}
```

```console
{{#include ../../../../examples/basics/functions/local/run.console:run}}
```

没有出口（或者出口永远到不了）就会一直递归下去，直到栈用完、程序崩掉。

## 小结

- 函数可以写在文件顶层（自由函数），也可以写在类里（方法）；`=> 表达式;` 是单句函数的简写。
- **自由函数不能重载**，同名即报 `E0408`；要重载就放进类里。
- 默认值让参数可选，**排在最后**；命名实参能跳过中间的可选参，也让调用点更好读。
- `params` 收任意多个实参，**必须是最后一个参数**。
- `ref` / `out` 让方法改到调用方的变量，**但编译器不检查修饰符是否对齐**——
  漏写会静默失效，多写会让按值参数真的被改。
- 返回多个值用**元组**，解构写 `(a, b) = f();`，前面不加 `var`。
- 局部函数能捕获外层局部变量；写递归先写出口。

下一章讲**字符串**——你一直在用的 `$"…"` 到底还能做什么。
