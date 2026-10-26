# 给编译器加一条检查：插件

团队里总有几条「大家都知道、但总有人忘」的规矩：catch 块不能空着、某个老 API 不许再用……
靠 code review 盯是盯不住的。z42 允许把**你自己写的代码装进编译器**，在每次构建时替你盯着——
这就是**插件**。本章写一个最小的 **analyzer**（分析器）插件：发现空的 `catch` 就报一条诊断。

> 熟悉 C# 的读者请注意：z42 的 analyzer 不是另一套工具链（没有 Roslyn 包、没有 IDE 专用加载）。
> 插件就是一个 `kind = "plugin"` 的普通工程，由 `z42 build` 代为构建并直接加载进编译进程。

> 更简单的形态：零依赖的插件连工程清单都不用写——消费方 `[plugins]` 条目直接指源文件
> （`"demo.hook" = { path = "hooks/hook.z42" }`）或无清单源目录（`{ path = "hooks/" }` 全收），
> 工具链合成最小清单后照常代建。本章示例要调规则级别，仍用工程形态（五种条目写法见参考手册
> z42-toml 页的 `[plugins]` 节）。

## 三个工程，一张图

示例目录里有一个插件工程和三个用它的工程。先看插件：

```console
{{#include ../../../../examples/practice/plugins/noemptycatch/run.console:layout}}
```

`kind = "plugin"` 说了两件事：这个包只在**工具链进程**里运行、永远不进产物；编译器自己的契约包
（`Z42.Analyzers`、`Z42.Syntax` 等所在的包）对它**自动可见**，所以 `[dependencies]` 一个字都不用写。

## 写一个 analyzer

一个类，继承 `Analyzer`，三个方法：

```z42
// examples/practice/plugins/noemptycatch/plugin/src/NoEmptyCatchAnalyzer.z42
{{#include ../../../../examples/practice/plugins/noemptycatch/plugin/src/NoEmptyCatchAnalyzer.z42}}
```

- `SupportedRules()`：这个 analyzer 会发哪些**规则**。一条规则 = 一个 id（`DEMO001`）、标题、
  消息、分类、默认级别、是否默认开启。id 就是用户在清单里调级、在代码里抑制时写的那个名字。
- `ObservedKinds()`：只对哪几种语法节点感兴趣。编译器只在这些节点上回调你，别的一概不打扰。
- `OnSyntaxNode(kind, node, diags)`：回调本体。`node` 是 `object`，自己 `as` 成具体节点类型；
  发现问题就 `diags.Report(规则, 位置)`。

类名**必须以 `Analyzer` 结尾**，这是编译器强制的命名约定。

## 把它装进一个工程

用插件的一方在清单里加一段 `[plugins]`，按**工程路径**指向插件：

```console
{{#include ../../../../examples/practice/plugins/noemptycatch/run.console:app-toml}}
```

`app/src/Main.z42` 里埋了一个空 catch：

```z42
// examples/practice/plugins/noemptycatch/app/src/Main.z42
{{#include ../../../../examples/practice/plugins/noemptycatch/app/src/Main.z42}}
```

构建它。`z42 build` 先替你把插件工程编出来（第一行 `wrote -> ../plugin/dist/…`），再加载进编译器
编 `app`，于是那条规则响了：

```console
{{#include ../../../../examples/practice/plugins/noemptycatch/run.console:app-build}}
```

诊断的形状和编译器自带的一样：文件、行列、规则 id、消息。默认级别是警告，所以产物照常生成、
退出码 0。改了插件的源码，下一次构建 `app` 时它会被自动重编——不用手动拷任何东西。

> `[plugins]` 与 `[dependencies]` 是两个世界：前者加载进**工具链进程**、只在构建期运行；
> 后者链进**你的程序**。把一个插件写进 `[dependencies]`，或把一个普通库写进 `[plugins]`，
> 都会被拒绝。

## 把警告升级成错误：`[lints]`

规矩要硬起来，就在清单里按规则 id 调级：

```console
{{#include ../../../../examples/practice/plugins/noemptycatch/run.console:strict-toml}}
```

同样的源码，现在编不过、不产产物、退出码 1——持续集成就此判红：

```console
{{#include ../../../../examples/practice/plugins/noemptycatch/run.console:strict-build}}
```

（这里插件没有重编：它刚被 `app` 那次构建编过，没有变化就复用。）

## 这一处我确实是故意的：`#suppress`

规则总有例外。在源码里用 `#suppress <id>` … `#restore <id>` 圈出一段，区间内这条规则不报：

```z42
// examples/practice/plugins/noemptycatch/quiet/src/Main.z42
{{#include ../../../../examples/practice/plugins/noemptycatch/quiet/src/Main.z42}}
```

```console
{{#include ../../../../examples/practice/plugins/noemptycatch/run.console:quiet-build}}
```

一条诊断都没有。`#suppress` 是纯编译期的指令，不会留在产物里。

## 当前的缺口

- ⚠️ analyzer 能观察的语法节点目前只有 6 种（catch 子句、try 语句、类声明、方法声明、
  `while`、`for`）。想对表达式、字段、属性做检查，现在还挂不上去。
- ⚠️ 插件分**按名**与**按路径**两种引用；本章用的按路径是自己写插件时的推荐写法。按名引用要先单独构建
  插件并把产物放进插件目录，细节见参考手册。

插件还有另外两类——**generator**（构建期生成源码）和 **hook**（接进 `z42b` 的构建相位）——
写法与打包方式和 analyzer 同一套，见[参考手册 · 插件](https://z42-lang.github.io/z42/reference/toolchain/compile-time-extensions.html)。

## 小结

- 插件 = `kind = "plugin"` 的工程，由 `z42 build` 代建并加载进编译器；契约包自动可见。
- analyzer 三件事：`SupportedRules` 声明规则、`ObservedKinds` 选节点、`OnSyntaxNode` 里 `Report`。
- 用插件的一方写 `[plugins] "名" = { path = "…" }`；`[lints]` 按规则 id 调级；`#suppress` / `#restore` 圈出例外。
- 诊断的形状与编译器自带的一致，警告不挡构建、错误挡。
