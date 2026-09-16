# 工程与构建

上一章的程序只有一个文件。真实的程序通常要拆成多个文件、要用别人写的库、要能交付给别人运行——这些都需要一个**工程**（project）。

这一章介绍工程长什么样、怎么创建、怎么构建，以及产物在哪。

## 什么时候需要工程

单个 `.z42` 文件足以写完一个小程序，但它有两条边界：

- **只能用标准库**。想用别人发布的库，得有地方声明依赖。
- **只有一个文件**。代码长到需要分门别类时，就该拆开。

碰到其中任何一条，就该建工程了。

## 创建工程

`z42 new` 后面跟上工程名：

```console
{{#include ../../../../examples/getting-started/projects/new/new.console:new}}
```

> 工程名只能由小写字母、数字、`-`、`_`、`.` 组成，并以字母或数字开头，例如 `greeter`、`my-app`。

## 工程里有什么

```console
{{#include ../../../../examples/getting-started/projects/new/new.console:files}}
```

一共三样东西（外加一个隐藏的 `.gitignore`）：

| 文件 | 作用 |
|------|------|
| `z42.toml` | **工程清单**：工程叫什么、版本号、编译出什么、源代码在哪 |
| `src/Main.z42` | 源代码，程序从这里开始执行 |
| `README.md` | 工程说明，写着常用命令 |

`z42.toml` 分两段：

- `[project]` 段描述工程本身。`kind = "exe"` 表示这是一个**可执行程序**；另一种是 `lib`（**库**），给其它工程调用，不能直接运行。
- `[sources]` 段说明哪些文件是源代码。`src/**/*.z42` 是一个**通配模式**：`src` 目录下（包括子目录）所有 `.z42` 文件。也就是说，**往 `src/` 里加文件就会被自动编译，不需要改清单**。

## 运行

在工程目录里直接运行 `z42 run`，不用再给文件名——它会从当前目录开始逐级向上寻找 `z42.toml`，所以在工程的任何子目录里（比如 `src/`）运行都可以：

```console
{{#include ../../../../examples/getting-started/projects/greeter/run.console:run}}
```

## 拆成多个文件

把问候语的拼装挪到单独一个文件 `src/Greeting.z42`：

```z42
{{#include ../../../../examples/getting-started/projects/greeter/src/Greeting.z42}}
```

`src/Main.z42` 直接调用它：

```z42
{{#include ../../../../examples/getting-started/projects/greeter/src/Main.z42}}
```

两个文件的第一行都是 **`namespace Greeter;`**——它声明这个文件里的代码属于 `Greeter` **命名空间**。命名空间用来给代码分组，避免不同工程里的同名类型互相冲突。**同一命名空间内的函数可以直接互相调用**，所以 `Main` 里写 `Build(name)` 就够了，不必写成 `Greeter.Build(name)`。

`z42 new` 按工程名生成命名空间（`my-app` 会变成 `MyApp`）。上一章的单文件程序没有写 `namespace`——那是允许的，不写就属于默认的全局命名空间。

## 只构建不运行

`z42 build` 只编译、不执行：

```console
{{#include ../../../../examples/getting-started/projects/greeter/run.console:build}}
```

刚才 `z42 run` 已经构建过一次了，源代码又没有改动，所以这次什么都不用做：

- `cached: 2/2 files` 说明两个源文件全部命中缓存，一个都不必重新编译；
- `no changes; preserved -> ./dist/greeter.zpkg` 说明**产物**原样保留。产物是一个 `.zpkg` 包，里面是编译好的字节码。

改动任意一个源文件再构建，只有改过的那个（以及依赖它的部分）会重编——这就是构建越到后面越快的原因。

默认构建的是 **debug** 配置（编译快、便于调试）。加上 `--release` 构建**发布**配置，编译器会做更多优化：

```sh
z42 build --release
```

两种配置的产物分开存放，互不覆盖。

## 清理

`z42 clean` 删除构建产物与缓存，源代码不受影响：

```console
{{#include ../../../../examples/getting-started/projects/greeter/run.console:clean}}
```

## 小结

- 需要多个源文件、或者需要依赖别人的库时，就用 `z42 new <名字>` 建工程；
- 工程由 `z42.toml` 清单和 `src/` 下的源代码组成，`[sources]` 的通配模式让新文件自动进入编译；
- 同一命名空间内的函数可直接互相调用；
- `z42 run` 在工程内任何位置都能用，`z42 build` 只编译，`--release` 构建发布配置，`z42 clean` 清理产物。

本章的全部代码在 [`examples/getting-started/projects/`](https://github.com/z42-lang/z42/tree/main/examples/getting-started/projects)。
