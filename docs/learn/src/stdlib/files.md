# 文件与目录

从这一部分开始，讲的不再是语言本身，而是**标准库怎么用**。第一站是文件系统：把字符串存成
文件、把文件读回来、列一个目录里有什么、拼出正确的路径。

三个静态类分工很清楚，都在 `Std.IO` 里：

| 类 | 管什么 |
|---|---|
| `File` | 单个文件：整读、整写、追加、复制、删除、大小 |
| `Directory` | 目录：创建、列出、删除 |
| `Path` | **纯字符串**的路径拼接与拆解——不碰磁盘，路径不存在也照样算 |

这三个属于 `z42.core`，单文件 `z42 run x.z42` 里 `using Std.IO;` 就能用，不需要建工程。
完整 API 见[控制台与文件](https://z42-lang.github.io/z42/reference/stdlib/io-file.html)。

## 读写一个文本文件

最常用的三个方法：写（覆盖）、追加、整读。

```z42
// examples/stdlib/files/text/text.z42
{{#include ../../../../examples/stdlib/files/text/text.z42:write}}
```

```console
{{#include ../../../../examples/stdlib/files/text/run.console:write}}
```

`WriteAllText` 的语义是**覆盖**：文件不存在就创建，存在就整个替换掉。要接着往后写用
`AppendAllText`。两者写的都是 UTF-8。

```z42
// examples/stdlib/files/text/text.z42
{{#include ../../../../examples/stdlib/files/text/text.z42:read}}
```

```console
{{#include ../../../../examples/stdlib/files/text/run.console:read}}
```

> 20 字节而字符数只有 8：中文一个字在 UTF-8 里占 3 字节。`GetSize` 数的是**磁盘上的字节**，
> `string.Length` 数的是**字符**，两者对非 ASCII 内容本来就不相等。

删除就是 `Delete`：

```z42
// examples/stdlib/files/text/text.z42
{{#include ../../../../examples/stdlib/files/text/text.z42:delete}}
```

```console
{{#include ../../../../examples/stdlib/files/text/run.console:delete}}
```

## 读写字节

图片、压缩包这类不是文本的内容，用 `ReadAllBytes` / `WriteAllBytes`——它们不做任何编码处理：

```z42
// examples/stdlib/files/bytes/bytes.z42
{{#include ../../../../examples/stdlib/files/bytes/bytes.z42:bytes}}
```

**`ReadAllText` 是严格 UTF-8 的**：内容不是合法 UTF-8 时它抛异常，而不是给你一串替换字符。
所以别拿它读二进制文件：

```z42
// examples/stdlib/files/bytes/bytes.z42
{{#include ../../../../examples/stdlib/files/bytes/bytes.z42:notutf8}}
```

```console
{{#include ../../../../examples/stdlib/files/bytes/run.console:run}}
```

> 这一整套都是**整读整写**：一次把文件全部装进内存。文件很大、或者要边读边处理时需要
> **流**，那套 API（`FileStream` 等）在 `z42.io` 包里，得先在工程清单里声明依赖才能用，
> 单文件模式解析不到。用法见参考手册的
> [流](https://z42-lang.github.io/z42/reference/stdlib/io-stream.html)。

## 拼路径：`Path`

不要用 `+` 拼路径，用 `Path.Join`：

```z42
// examples/stdlib/files/paths/paths.z42
{{#include ../../../../examples/stdlib/files/paths/paths.z42:join}}
```

```console
{{#include ../../../../examples/stdlib/files/paths/run.console:join}}
```

两个反直觉的地方值得先记住：

- ⚠️ **后一段是绝对路径时，前面的全被吃掉**（`Join("data", "/etc/passwd")` → `/etc/passwd`）。
  这和多数语言的 join 一致，但如果那一段来自用户输入，就是一个逃出目录的口子——拼之前先
  用 `Path.IsRooted` 挡一道。
- ⚠️ **`Join` 不做任何规范化**：`.` 和 `..` 原样留在结果里。要干净的路径自己调 `Normalize`。

`Normalize` 做的是**纯词法**的整理：折叠重复分隔符、去掉 `.`、抵消 `..`、去掉末尾分隔符：

```z42
// examples/stdlib/files/paths/paths.z42
{{#include ../../../../examples/stdlib/files/paths/paths.z42:normalize}}
```

```console
{{#include ../../../../examples/stdlib/files/paths/run.console:normalize}}
```

它**不碰磁盘**——不解析符号链接，也不要求路径存在。

### 拆开一个路径

```z42
// examples/stdlib/files/paths/paths.z42
{{#include ../../../../examples/stdlib/files/paths/paths.z42:split}}
```

```console
{{#include ../../../../examples/stdlib/files/paths/run.console:split}}
```

| 方法 | 给 `data/raw/report.tar.gz` |
|---|---|
| `GetFileName` | `report.tar.gz` |
| `GetExtension` | `gz`——**不含点**，而且只取最后一段 |
| `GetFileNameWithoutExtension` | `report.tar` |
| `GetDirectoryName` | `data/raw` |

两个容易踩的边界：

```z42
// examples/stdlib/files/paths/paths.z42
{{#include ../../../../examples/stdlib/files/paths/paths.z42:dotfile}}
```

```console
{{#include ../../../../examples/stdlib/files/paths/run.console:dotfile}}
```

`.bashrc` 这种以点开头的名字**整体是文件名**，扩展名为空；`notes.` 结尾那个点也不产出扩展名。

> `Path.Separator` 恒为 `'/'`，在 Windows 上也是。而**解析**类方法（`GetFileName` 等）
> 同时把 `/` 和 `\` 当分隔符，所以 Windows 风格的路径也拆得开。

## 目录

`Directory.Create` 的行为是 `mkdir -p`：中间目录一起建，**目录已存在也不报错**。

```z42
// examples/stdlib/files/dirs/dirs.z42
{{#include ../../../../examples/stdlib/files/dirs/dirs.z42:create}}
```

列出内容有两个方法，返回的东西不一样：

```z42
// examples/stdlib/files/dirs/dirs.z42
{{#include ../../../../examples/stdlib/files/dirs/dirs.z42:list}}
```

```console
{{#include ../../../../examples/stdlib/files/dirs/run.console:list}}
```

- `Enumerate` 只看**直接子项**，给的是**名字**（`index.html`），不是路径。
- `EnumerateRecursive` 深度展开，给的是**相对传入目录的子路径**（`posts/2026/hello.md`），
  而且**中间目录本身也是一条**（`posts`、`posts/2026` 都在结果里）。

> ⚠️ **两者的顺序都不保证**（取决于操作系统）。上面示例里显式调了 `Array.Sort` 才有稳定输出；
> 你的代码若依赖顺序，也要自己排。

### 「存在」有两个问题

```z42
// examples/stdlib/files/dirs/dirs.z42
{{#include ../../../../examples/stdlib/files/dirs/dirs.z42:exists}}
```

```console
{{#include ../../../../examples/stdlib/files/dirs/run.console:exists}}
```

⚠️ **`File.Exists` 对目录也返回 `true`**——它问的是「这个路径上有东西吗」。要判断「是不是一个
普通文件」得两个一起用：

```text
bool isRegularFile = File.Exists(p) && !Directory.Exists(p);
```

`Directory.Exists` 反过来只对目录为真。

删目录要显式说清楚删不删内容：

```z42
// examples/stdlib/files/dirs/dirs.z42
{{#include ../../../../examples/stdlib/files/dirs/dirs.z42:delete}}
```

```console
{{#include ../../../../examples/stdlib/files/dirs/run.console:delete}}
```

第二个参数是 `recursive`：`true` 相当于 `rm -rf`；`false` 只能删空目录，目录非空时抛异常。

## 按模式找文件：`Glob`

```z42
// examples/stdlib/files/glob/glob.z42
{{#include ../../../../examples/stdlib/files/glob/glob.z42:glob}}
```

```console
{{#include ../../../../examples/stdlib/files/glob/run.console:glob}}
```

两个都返回**排好序的完整路径**（拼上了传入的目录），通配符只有 `*`（任意序列）和 `?`（单字符），
大小写敏感，**没有 `**`**——递归版本本身就是全展开。

⚠️ `GlobRecursive` 里的 `*` **会跨 `/`**：`"*/x.txt"` 能匹配到 `a/b/c/x.txt`。要「只跨一层」
得自己再过滤。

```z42
// examples/stdlib/files/glob/glob.z42
{{#include ../../../../examples/stdlib/files/glob/glob.z42:nodir}}
```

```console
{{#include ../../../../examples/stdlib/files/glob/run.console:nodir}}
```

## 出错时抛什么

**这套 API 没有 `IOException`**。文件不存在、权限不足、目录非空、内容不是 UTF-8——全部抛基类
`Std.Exception`，`Message` 是操作系统的原文：

```z42
// examples/stdlib/files/errors/errors.z42
{{#include ../../../../examples/stdlib/files/errors/errors.z42:missing}}
```

```console
{{#include ../../../../examples/stdlib/files/errors/run.console:missing}}
```

所以 `catch` 只能按 `Std.Exception` 抓，要区分原因就得读 `Message`——它是 OS 文本，
**不适合当程序逻辑的判据**。能先问就先问：用 `File.Exists` / `Directory.Exists` 提前判断，
比事后解析消息可靠。

### 写文件不会顺手建目录

```z42
// examples/stdlib/files/errors/errors.z42
{{#include ../../../../examples/stdlib/files/errors/errors.z42:parent}}
```

```console
{{#include ../../../../examples/stdlib/files/errors/run.console:parent}}
```

先 `Directory.Create` 再写——这是最常见的踩点。

### 「不存在」时谁抛谁不抛

同样是「东西不在」，不同方法的反应不一样，值得记一下：

```z42
// examples/stdlib/files/errors/errors.z42
{{#include ../../../../examples/stdlib/files/errors/errors.z42:tolerant}}
```

```console
{{#include ../../../../examples/stdlib/files/errors/run.console:tolerant}}
```

> 熟悉 C# 的读者请注意：C# 的 `File.Delete` 对不存在的文件是**静默成功**的，z42 这里会抛。
> 想要「有就删」的语义，自己先判一下 `File.Exists`。

## 临时文件

跑测试、做中间产物时常要一个不会撞名的位置：

```z42
// examples/stdlib/files/temp/temp.z42
{{#include ../../../../examples/stdlib/files/temp/temp.z42:temp}}
```

```console
{{#include ../../../../examples/stdlib/files/temp/run.console:temp}}
```

两个都返回**绝对路径**，名字里带随机段和进程号，所以并发跑也不会撞。

⚠️ **它们不会自己消失**：z42 没有 RAII，也没有 finalizer——用完自己 `File.Delete` /
`Directory.Delete(dir, true)`。

## 小结

- `File` / `Directory` / `Path` 都在 `Std.IO`，属 `z42.core`，单文件模式直接可用。
- 文本：`WriteAllText`（覆盖）/ `AppendAllText`（追加）/ `ReadAllText`（**严格 UTF-8**，
  不合法就抛）。字节：`ReadAllBytes` / `WriteAllBytes`。都是**整读整写**；要流式处理得用
  `z42.io` 的 `FileStream`，那需要工程依赖。
- `Path` 是纯字符串运算：`Join` **不规范化**、后段是绝对路径会**吃掉前面**；要干净路径调
  `Normalize`。`GetExtension` **不含点**、只取最后一段，`.bashrc` 的扩展名是空。
- `Directory.Create` 是 `mkdir -p`；`Enumerate` 给**名字**、只一层，`EnumerateRecursive`
  给**相对子路径**、**中间目录也算一条**；两者**顺序都不保证**，要稳定自己 `Array.Sort`。
- ⚠️ `File.Exists` 对目录也是 `true`；判「普通文件」要 `File.Exists(p) && !Directory.Exists(p)`。
- `Path.Glob` / `GlobRecursive` 返回排好序的完整路径，只有 `*` 和 `?`，递归版的 `*` 跨 `/`；
  目录不存在返回空数组。
- 出错一律 `Std.Exception` + OS 原文消息，**没有 `IOException`**；写文件不会建父目录；
  `File.Delete` / `Directory.Enumerate` 对不存在的目标会抛，而 `Path.Glob` / `Exists` 不会。
- 临时文件 / 目录用完要**自己删**。

下一章讲**数据格式**——把 JSON、TOML、YAML 读成对象，再写回去。
