# 发布到别人的机器上

`z42 run` 跑的是你自己机器上的工程；`z42 build` 产出的 `.zpkg` 也还要靠已安装的 z42 才能跑。
要把程序**交给没装 z42 的人**，或者送到手机和浏览器里，就是这一章的事。z42 把它分成两个动词：

| 动词 | 产出 | 给谁 |
|---|---|---|
| `z42 publish` | 桌面应用目录：可执行文件 + 你的 `.zpkg` + 依赖库 | macOS / Linux / Windows 用户，双击或命令行就能跑 |
| `z42 export --rid <rid>` | 原生 IDE 工程（Xcode / Gradle / 网页） | iOS / Android / 浏览器——交给平台自己的工具链去编 |

> 熟悉 C# 的读者请注意：`publish` 的默认形态与 `--self-contained` 对应 .NET 的 framework-dependent
> 与 self-contained；手机和浏览器不走 `publish`，而是 `export` 出原生工程交给平台工具链。

本章用的还是最普通的程序：

```z42
// examples/practice/publishing/greet/src/Main.z42
{{#include ../../../../examples/practice/publishing/greet/src/Main.z42}}
```

```console
{{#include ../../../../examples/practice/publishing/run.console:run}}
```

## 第一步：在清单里声明「我要发布桌面应用」

`z42 publish` 要给可执行文件配一个**宿主**（apphost）——一个小小的原生程序，负责把 z42 运行时拉起来、
装载你的 `.zpkg`。清单里不说要它，`publish` 不会猜：

```console
{{#include ../../../../examples/practice/publishing/run.console:plain}}
```

照它说的，在 `z42.toml` 里加一段 `[platform.desktop]`：

```console
{{#include ../../../../examples/practice/publishing/run.console:toml}}
```

## 第二步：装一次 desktop workload

apphost 不随基础 SDK 走，它在 **desktop workload** 里（里面是各个桌面 RID 的 apphost）。
没装的时候 `publish` 会把安装命令直接打给你：

```console
{{#include ../../../../examples/practice/publishing/run.console:publish}}
```

`z42 workload list` 看本机装了哪些；一台机器装一次就够：

```console
{{#include ../../../../examples/practice/publishing/run.console:list}}
```

```text
$ z42 workload install desktop
```

> ⚠️ 本书的示例在一个**没有装任何 workload** 的干净环境里重放，所以上面只能演示到「它要什么」；
> 装好 desktop workload 之后，照下一节的描述做就能拿到产出。

## 第三步：`z42 publish`

装好之后，在工程目录里：

```text
$ z42 publish                    # 当前机器的 RID，产出到 ./publish
$ z42 publish --self-contained   # 把 z42 运行时一起打进去
```

产出落在工程目录下的 `publish/`（`--output` 或清单的 `[platform.desktop] publish_dir` 可改）。
两种形态的区别只有一条：目标机器上**有没有装 z42**。

| | 目录里有什么 | 目标机器要求 |
|---|---|---|
| 默认 | 可执行文件、`app.zpkg`、`libs/`（用到的标准库）、`app.runtimeconfig.toml` | 装了 z42 运行时 |
| `--self-contained` | 同上，可执行文件里**嵌入**了运行时 | 什么都不用装 |

`app.runtimeconfig.toml` 就是[运行时设置](https://z42-lang.github.io/z42/reference/toolchain/runtime-settings.html)
的侧车文件：改 GC、JIT 之类的旋钮不用重新发布，改这个文件就行。

**目前只有桌面 RID 能 `publish`**（`macos-*` / `linux-*` / `windows-*`）。手机和浏览器走下一节。

## 手机与浏览器：`z42 export`

iOS、Android、浏览器没有「一个可执行文件」这回事——它们各有自己的工程格式和构建工具。
`z42 export --rid <rid>` 替你生成那个**原生工程**，之后交给 Xcode / Gradle / 网页服务器：

```console
{{#include ../../../../examples/practice/publishing/run.console:export}}
```

生成的目录按 `<工程名>-<平台>` 命名（`--output` 可改）。浏览器目标得到一页 `index.html` 加装载脚本；
iOS / Android 分别得到 Xcode 与 Gradle 工程，各自还要几个必填项（`--bundle-id` / `--app-id`，
或写在清单的 `[platform.ios]` / `[platform.android]` 里）。

和 `publish` 一样，真正编出来要先装对应平台的 workload（`z42 workload install wasm` / `ios` / `android`），
没装时 `export` 会在输出里提示缺什么——上面省略掉的几行就是这类提示。

## 小结

- 两个动词：桌面用 `publish`（可执行目录），手机 / 浏览器用 `export --rid`（原生工程）。
- `publish` 前要两步：清单 `[platform.desktop] apphost = true`、本机 `z42 workload install desktop`（一次）。
- 默认形态依赖目标机器上的 z42 运行时；`--self-contained` 把运行时嵌进去，目标机器零安装。
- 产出里的 `app.runtimeconfig.toml` 可以事后调运行时旋钮，不必重新发布。

旗标全表与清单字段见[参考手册 · `z42 publish`](https://z42-lang.github.io/z42/reference/toolchain/cli-z42.html#z42-publish-options-manifest)
与 [`z42.toml` 的 `[platform.*]`](https://z42-lang.github.io/z42/reference/toolchain/z42-toml.html)。
