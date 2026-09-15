# Hello, World

这一章创建第一个 z42 工程，运行它，读懂每一行代码，再动手改一改。

开始之前，请确认[上一章](install.md)的 `z42 --version` 能正常输出。

## 创建工程

找一个你喜欢的目录，运行 `z42 new`，后面跟上工程名：

```console
{{#include ../../../../examples/getting-started/hello-world/new/new.console:new}}
```

`z42 new` 创建了一个名为 `hello` 的目录，里面是一个完整的、可以直接运行的工程。输出最后两行提示了下一步要做什么。

> 工程名只能由小写字母、数字、`-`、`_`、`.` 组成，并以字母或数字开头，例如 `hello`、`my-app`。

## 工程里有什么

进入工程目录看一看：

```console
{{#include ../../../../examples/getting-started/hello-world/new/new.console:files}}
```

一共三样东西（外加一个隐藏的 `.gitignore`）：

| 文件 | 作用 |
|------|------|
| `z42.toml` | **工程清单**：工程叫什么、版本号、编译出什么、源代码在哪 |
| `src/Main.z42` | 源代码，程序从这里开始执行 |
| `README.md` | 工程说明，写着常用命令 |

`z42.toml` 分两段：

- `[project]` 段描述工程本身。`kind = "exe"` 表示这是一个**可执行程序**；另一种是 `lib`（**库**），给其它工程调用，不能直接运行。
- `[sources]` 段说明哪些文件是源代码：`src/**/*.z42` 表示 `src` 目录下（包括子目录）所有 `.z42` 文件。

## 运行

在工程目录里运行 `z42 run`：

```console
{{#include ../../../../examples/getting-started/hello-world/new/new.console:run}}
```

`z42 run` 做了两件事：先把源代码**编译**成字节码，再用 z42 虚拟机**运行**它。第二次运行时，没有改动过的文件不会重新编译，所以会快很多。

`z42 run` 会从当前目录开始逐级向上寻找 `z42.toml`，所以在工程的任何子目录里（比如 `src/`）运行都可以。

## 读懂代码

回头看 `src/Main.z42`，一共三部分：

**`namespace Hello;`** 声明这个文件里的代码属于 `Hello` **命名空间**。命名空间用来给代码分组，避免不同工程里的同名类型互相冲突。`z42 new` 按工程名生成它（`my-app` 会变成 `MyApp`）。

**`using Std.IO;`** 引入标准库的 `Std.IO` 命名空间。下面用到的 `Console`（控制台）就定义在那里；不写这一行，编译器会找不到 `Console`。

**`void Main() { ... }`** 定义一个名为 `Main` 的函数，它就是程序的**入口**——运行程序时，z42 从这里开始执行。`void` 表示这个函数不返回值。函数体里只有一句：

- `Console.WriteLine("Hello, World!");` 调用 `Console` 的 `WriteLine` 方法，把一行文字打印到终端。每条语句以分号 `;` 结尾。

> 熟悉 C# 的读者请注意：z42 的 `Main` 是写在文件顶层的**自由函数**，不需要包在 `class Program` 里。

## 让程序接收参数

我们把程序改得有用一点：让它向命令行上给出的名字问好。下面是改好的完整工程（名为 `greet`）：

```z42
{{#include ../../../../examples/getting-started/hello-world/greet/src/Main.z42}}
```

新出现了三样东西：

- `string[] args = Environment.GetCommandLineArgs();` 取得传给程序的命令行参数。`string[]` 是**字符串数组**，`Environment` 与 `Console` 一样来自 `Std.IO`。
- `args.Length > 0 ? args[0] : "World"` 是**条件表达式**：有参数时取第一个参数 `args[0]`，没有时用 `"World"`。
- `$"Hello, {name}!"` 是**插值字符串**：以 `$` 开头，花括号里的表达式会被替换成它的值。

运行时，程序自己的参数写在 `--` 后面，用来和 `z42 run` 自己的选项区分开：

```console
{{#include ../../../../examples/getting-started/hello-world/greet/run.console}}
```

## 出错了会怎样

写代码难免出错。假设我们把 `Console` 误拼成了 `Consle`：

```z42
{{#include ../../../../examples/getting-started/hello-world/typo/src/Main.z42}}
```

运行时，编译器会拒绝编译，并指出问题所在：

```console
{{#include ../../../../examples/getting-started/hello-world/typo/run.console}}
```

- `./src/Main.z42(6,5)` 是出错的位置：文件 `src/Main.z42` 的第 6 行、第 5 列。
- `E0401` 是**错误码**，每一类错误有固定的编号，方便查找说明。
- `undefined: Consle` 说明编译器不认识 `Consle` 这个名字。

最后一行 `[exit: 1]` 表示命令以**退出码 1** 结束（成功时是 0，书中省略不写）。脚本和持续集成通常靠退出码判断命令是否成功。

把拼写改回 `Console`，再运行就好了。

## 小结

- `z42 new <名字>` 创建工程，`z42 run` 编译并运行它；
- 工程由 `z42.toml` 清单和 `src/` 下的源代码组成；
- 程序从顶层函数 `void Main()` 开始执行；`using` 引入命名空间；
- 程序参数写在 `z42 run --` 之后，用 `Environment.GetCommandLineArgs()` 读取；
- 编译错误会给出文件、行列、错误码和原因。

本章的全部代码在 [`examples/getting-started/hello-world/`](https://github.com/z42-lang/z42/tree/main/examples/getting-started/hello-world)。
