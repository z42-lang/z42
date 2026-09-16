# Hello, World

这一章写下你的第一个 z42 程序，运行它，读懂每一行，再动手改一改。

开始之前，请确认[上一章](install.md)的 `z42 --version` 能正常输出。

## 写下程序

找一个你喜欢的目录，新建一个名为 `hello.z42` 的文件（`.z42` 是 z42 源文件的扩展名），用任何编辑器写入这五行：

```z42
{{#include ../../../../examples/getting-started/hello-world/hello/hello.z42}}
```

## 运行

在这个文件所在的目录里，运行它：

```console
{{#include ../../../../examples/getting-started/hello-world/hello/run.console:run}}
```

`z42 run` 做了两件事：先把源代码**编译**成字节码，再用 z42 虚拟机**运行**它。

编译产物放在 z42 自己的缓存目录里，**不会在你的目录下留下任何东西**——旁边始终只有 `hello.z42` 这一个文件。第二次运行时，没有改动过的文件不必重新编译，所以会快一些。

> 也可以省掉 `run` 直接写 `z42 hello.z42`，效果完全一样。

## 读懂代码

一共两部分：

**`using Std.IO;`** 引入标准库的 `Std.IO` 命名空间。下面用到的 `Console`（控制台）就定义在那里；不写这一行，编译器会找不到 `Console`。

**`void Main() { ... }`** 定义一个名为 `Main` 的函数，它就是程序的**入口**——运行程序时，z42 从这里开始执行。`void` 表示这个函数不返回值。函数体里只有一句：

- `Console.WriteLine("Hello, World!");` 调用 `Console` 的 `WriteLine` 方法，把一行文字打印到终端。每条语句以分号 `;` 结尾。

> 熟悉 C# 或 Java 的读者请注意：z42 的 `Main` 是写在文件顶层的**自由函数**，不需要包在 `class Program` 里。

## 让程序接收参数

我们把程序改得有用一点：让它向命令行上给出的名字问好。新建 `greet.z42`：

```z42
{{#include ../../../../examples/getting-started/hello-world/greet/greet.z42}}
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
{{#include ../../../../examples/getting-started/hello-world/typo/typo.z42}}
```

运行时，编译器会拒绝编译，并指出问题所在：

```console
{{#include ../../../../examples/getting-started/hello-world/typo/run.console}}
```

- `typo.z42(4,5)` 是出错的位置：文件 `typo.z42` 的第 4 行、第 5 列。
- `E0401` 是**错误码**，每一类错误有固定的编号，方便查找说明。
- `undefined: Consle` 说明编译器不认识 `Consle` 这个名字。

最后一行 `[exit: 1]` 表示命令以**退出码 1** 结束（成功时是 0，书中省略不写）。脚本和持续集成通常靠退出码判断命令是否成功。

把拼写改回 `Console`，再运行就好了。

## 小结

- z42 源文件以 `.z42` 结尾，`z42 run <文件>` 直接编译并运行它；
- 程序从顶层函数 `void Main()` 开始执行；`using` 引入命名空间；
- 程序参数写在 `--` 之后，用 `Environment.GetCommandLineArgs()` 读取；
- 编译错误会给出文件、行列、错误码和原因。

## 下一步

单个文件足够写小程序，但它只能使用标准库——**要拆成多个文件、或者用别人写的库，就需要一个工程**。下一章[工程与构建](projects.md)介绍工程长什么样、怎么创建和构建。

本章的全部代码在 [`examples/getting-started/hello-world/`](https://github.com/z42-lang/z42/tree/main/examples/getting-started/hello-world)。
