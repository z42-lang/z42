# 变量与基本类型

从这一章开始讲语言本身。先是最基础的一件事：**怎么存一个值**。

## 声明变量

写法是「类型 名字 = 值」：

```z42
// examples/basics/variables/types/types.z42
{{#include ../../../../examples/basics/variables/types/types.z42:decl}}
```

跑起来：

```console
{{#include ../../../../examples/basics/variables/types/run.console:decl}}
```

几个容易记混的点：

- **不带后缀的小数是 `double`**（不是 `float`）。要 `float` 得写 `1.5f`。
- **`L` 后缀表示 64 位整数**。`9_000_000_000` 超出 32 位范围，必须写成 `9_000_000_000L`。
- **数字里的 `_` 只是给人看的**，编译器直接忽略——`1_000_000` 和 `1000000` 完全一样。
- **`char` 用单引号，`string` 用双引号**，两者不能混。

## 常用类型

日常够用的就这几个：

| 写法 | 装什么 | 例子 |
|------|--------|------|
| `int` | 整数（32 位） | `42` |
| `long` | 大整数（64 位） | `9_000_000_000L` |
| `double` | 小数 | `3.14` |
| `bool` | 真 / 假 | `true` / `false` |
| `char` | 单个字符 | `'z'` |
| `string` | 文本 | `"hello"` |

> z42 还有另一套 Rust 风格的短名（`i32` / `f64` / `u8`…），和上面的关键字**指向同一个类型**，
> 可以混用。完整类型总表（所有位宽与无符号类型）见参考手册的
> [基本类型与字面量](https://z42-lang.github.io/z42/reference/language/types.html)。

## `var`：让编译器推断类型

右边已经写明白是什么类型时，左边可以写 `var`：

```z42
// examples/basics/variables/types/types.z42
{{#include ../../../../examples/basics/variables/types/types.z42:var}}
```

这段接在上面那段后面，同一个程序再多打一行——`GetType().Name` 报出编译器推断的类型：

```console
{{#include ../../../../examples/basics/variables/types/run.console:var}}
```

`var` **不是**「动态类型」——类型在编译期就定死了，只是不用你重复写一遍。上面 `n` 就是 `int`，
之后给它赋字符串照样报错。

什么时候用：右边一眼能看出类型（字面量、`new Foo()`）。什么时候别用：右边是个函数调用、
读者看不出返回什么时，写清楚类型更友好。

## 类型转换

**变宽不用管，变窄必须自己写。**

```z42
// examples/basics/variables/convert/convert.z42
{{#include ../../../../examples/basics/variables/convert/convert.z42:conv}}
```

```console
{{#include ../../../../examples/basics/variables/convert/run.console:ok}}
```

注意 `(int)3.9` 得到 `3` ——**直接砍掉小数部分，不是四舍五入**。

漏写 `(int)` 会怎样？编译器拦住你：

```z42
// examples/basics/variables/narrowing/narrowing.z42
{{#include ../../../../examples/basics/variables/narrowing/narrowing.z42}}
```

```console
{{#include ../../../../examples/basics/variables/narrowing/run.console:err}}
```

> 熟悉 C# 的读者请注意：**z42 比 C# 更严**。C# 允许 `float f = someDouble` 这类有损转换隐式发生，
> z42 一律要求显式 `(T)`。规则是「**只要可能丢东西，就得由你写出来**」。
> 完整转换矩阵见参考手册的
> [类型转换](https://z42-lang.github.io/z42/reference/language/conversions.html)。

## 不变的值：`const` 与 `readonly`

两个都表示「定下就不改」，但时机不同：

```z42
// examples/basics/variables/immutable/immutable.z42
{{#include ../../../../examples/basics/variables/immutable/immutable.z42:decl}}
```

```console
{{#include ../../../../examples/basics/variables/immutable/run.console:run}}
```

| | 什么时候定下 | 放哪 |
|---|---|---|
| `const` | **编译期**——值直接内联进用它的地方，不占存储 | 类的静态常量，或方法里的局部常量 |
| `readonly` | **运行期**——每个对象各有一份，构造器里赋一次 | 类的字段 |

判断方法：值在写代码时就知道（`3.14159`、`"v1"`、`100`）就用 `const`；要等对象造出来才知道
（由构造器参数算出）就用 `readonly`。

## 关于 `?`

你会看到这种写法：

```z42,ignore
string? maybe = null;

string s = maybe;
if (s == null) { s = "fallback"; }   // 取不到就用备用值
```

关于 `?` 本身，要知道一件事：

> **`?` 不是「这里可能是空」，而是「请编译器在这里强制检查」。**
>
> 引用类型**本来就允许为空**——把 `null` 赋给没标 `?` 的变量不报错，这一点不变。
> `?` 是你主动提的要求：标了它的**参数**和**返回值**，用之前编译器会逼你判空，
> 没判就报 **E0478**。
>
> ```z42,ignore
> int Length(string? s) { return s.Length; }   // ✗ 标了 ? 就得先判
> int Length(string  s) { return s.Length; }   // ✓ 没标 ⇒ 不受检
> ```
>
> **不标就完全不受检**，所以这是"想严格的地方才严格"，老代码一行都不用改。
> 值类型（`int` / `bool` / `struct`…）永远不能为空，给它们标 `?` 会直接报错。
>
> 逃生口有两个：把判空写出来（主要那个），或者 `x.Expect("理由")` —— 后者在运行期
> **真的检查**，为空就当场抛出，消息就是你写的那句理由。**没有**别的语言里那种
> 「我保证非空」的后缀（C# 的 `!`）：那个后缀运行期什么都不做，正是让问题溜走的口子。
> 完整规则见

## 小结

- 声明是「类型 名字 = 值」；`var` 让编译器推断，类型仍在编译期定死。
- 不带后缀的小数是 `double`，`L` 是长整数，`f` 是 float，`_` 只为好读。
- **变宽自动，变窄必须写 `(类型)`**——z42 在这点上比 C# 严。
- `const` 是编译期常量，`readonly` 是运行期每实例一份。
- `?` 是"请编译器在这里强制检查"的请求；不标的地方不受检，值类型不能标。

下一章讲怎么把这些值**算起来**。
