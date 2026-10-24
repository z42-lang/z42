# 文档注释与 `z42 doc`

前面每一章的代码都在讲**怎么写**；这一章讲怎么让别人（和三个月后的你）**不读源码就知道怎么用**。
z42 把这件事做成了语言的一部分：写在声明上方的 `///` 是 **doc 注释**，编译器认得它，`z42 doc`
命令按名字就能把它查出来——签名、说明、在哪个文件第几行，一条命令。

> 熟悉 C# 的读者请注意：z42 的 doc 注释不是 XML（没有 `<summary>` / `<param>` 标签）。正文是
> Markdown，结构化的部分用 `@param` 这样的单行标记。

## 两种注释，两种读者

`//` 写给**维护者**：为什么这么实现、踩过什么坑，跟着代码走。
`///` 写给**调用者**：这个东西做什么、有什么约定、什么时候会抛异常。它挂在紧随其后的声明上，
成为那个声明的一部分。

下面这个文件给类、字段、方法都写了 `///`，文件开头的 `//!` 则是整个命名空间的概述：

```z42
// examples/practice/doc-comments/shapes/shapes.z42
{{#include ../../../../examples/practice/doc-comments/shapes/shapes.z42}}
```

它首先得是个能跑的程序：

```console
{{#include ../../../../examples/practice/doc-comments/shapes/run.console:run}}
```

## 不读源码，查它的 API

在这个目录里直接问 `z42 doc`。不需要先构建——它读的是源码：

```console
{{#include ../../../../examples/practice/doc-comments/shapes/run.console:query}}
```

三行：签名与所在命名空间、doc 的第一段、位置。加 `--members` 看一个类型的**轮廓**，每个成员一行：

```console
{{#include ../../../../examples/practice/doc-comments/shapes/run.console:members}}
```

轮廓里每行只显示 doc 的**第一段**——这叫**摘要**。所以第一段要写成一句话、说清它做什么，
细节放到空一行之后的正文里。`Scale` 的 doc 就是这样分层的，查它本身时正文和标记都会出来：

```console
{{#include ../../../../examples/practice/doc-comments/shapes/run.console:tags}}
```

`@param` 和 `@throws` 是**标记**：从第一个行首 `@` 开始，每条独占一行。常用的有这几个：

| 标记 | 写什么 |
|---|---|
| `@param 名 说明` | 签名表达不了的事：取值范围、单位、边界行为 |
| `@returns 说明` | 返回值 |
| `@throws 类型 何时` | 哪种异常、什么条件下抛 |
| `@see [`符号`]` | 相关的符号 |
| `@warning 说明` | 要醒目提示的事（线程安全、不是密码学安全……） |

全部可选。**只为签名表达不了的东西写**——`@param factor 缩放系数` 这种复述参数名的话不如不写。
完整的标记清单与书写规则见[参考手册 · doc 注释](https://z42-lang.github.io/z42/reference/language/doc-comments.html)。

## 命名空间的概述：`//!`

文件开头、`namespace` 之上的 `//!` 挂到命名空间上。查命名空间得到概述加它下面的成员清单：

```console
{{#include ../../../../examples/practice/doc-comments/shapes/run.console:namespace}}
```

每行开头的符号是可见性：`+` 是 public，`~` 是 internal。`Main` 没写 `///`，摘要那一栏就是空的——
`z42 doc` 不会替你编一句。

## 不知道叫什么名字时：搜

`--search` 按名字**和摘要**找，所以把「做什么」写进摘要，别人就搜得到：

```console
{{#include ../../../../examples/practice/doc-comments/shapes/run.console:search}}
```

## record 的位置参数

`[Record]` 的位置参数会展开成字段。给它们写文档不用 `@param`——**每个参数前直接写一行 `///`**，
挂到展开后的字段上：

```z42
// examples/practice/doc-comments/records/records.z42
{{#include ../../../../examples/practice/doc-comments/records/records.z42}}
```

```console
{{#include ../../../../examples/practice/doc-comments/records/run.console:run}}
```

```console
{{#include ../../../../examples/practice/doc-comments/records/run.console:members}}
```

字段各有各的摘要；自动生成的构造器拿的是类型自己的那句。字段也能单独查：

```console
{{#include ../../../../examples/practice/doc-comments/records/run.console:field}}
```

## 什么地方的 `///` 挂不上

`///` 只认**紧随其后的声明**。写在方法形参前面、或者方法体里面的 `///`，不属于任何声明，
`z42 doc` 查不到它们——编译器也不会报错，它们只是悄悄地不存在：

```z42
// examples/practice/doc-comments/orphan/orphan.z42
{{#include ../../../../examples/practice/doc-comments/orphan/orphan.z42}}
```

```console
{{#include ../../../../examples/practice/doc-comments/orphan/run.console:run}}
```

```console
{{#include ../../../../examples/practice/doc-comments/orphan/run.console:add}}
```

`Add` 的摘要在，两行「挂不到任何东西」的注释都不在。方法参数的说明请用 `@param`；
方法体里要留笔记就用普通的 `//`。

另外两条容易踩的：`///` 前面只能有空白（写在代码后面的是普通注释）；`////` 四个斜杠起不算 doc，
可以放心当分隔线用。

## 当前的缺口

- ⚠️ **标记还不校验**。`@param` 写错名字、`@throws` 写了不存在的类型，编译器目前不会警告——
  写完用 `z42 doc` 查一遍自己看。
- ⚠️ **标准库的文档暂时查不到**。`z42 doc` 现在只索引有源码的包；装好的 SDK 里没有标准库源码，
  查 `String` 之类会说找不到。标准库 API 请看[参考手册 · 标准库](https://z42-lang.github.io/z42/reference/stdlib/)，
  那些页面正是从标准库源码的 `///` 生成的。
- ⚠️ `@example` 里的代码还不会被当作测试运行。

## 小结

- `///` 给调用者，`//` 给维护者；`//!` 写在文件开头，是命名空间的概述。
- 第一段是摘要，一句话；细节空一行再写；结构化信息用 `@param` / `@throws` 等标记，只写签名表达不了的。
- `z42 doc <名字>` 查签名与说明，`--members` 看轮廓，`--search` 按名字和摘要搜；不用先构建。
- record 位置参数每个前面写一行 `///`；方法形参和方法体里的 `///` 挂不上任何东西。

命令的全部旗标见[参考手册 · `z42 doc`](https://z42-lang.github.io/z42/reference/toolchain/cli-z42.html#z42-doc-options-符号)。
