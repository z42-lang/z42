# 字符串

`$"…"` 你从第一章就在用了。这一章把字符串讲全：三种写法各管什么、常用的操作有哪些、
以及几个容易栽的地方。

`string` 是**不可变**的——所有"修改"方法都返回新串，原串不动。

## 三种字面量

```z42
// examples/basics/strings/literals/literals.z42
{{#include ../../../../examples/basics/strings/literals/literals.z42:code}}
```

```console
{{#include ../../../../examples/basics/strings/literals/run.console:run}}
```

- **普通串 `"…"`**——日常默认，处理 `\n` 这类转义。
- **插值串 `$"…"`**——花括号里嵌表达式。
- **原始串 `"""…"""`**——里面写什么就是什么，转义和引号都不用操心。

## 转义

普通串里 `\` 开头的组合有特殊含义：

| 写法 | 含义 | 写法 | 含义 |
|------|------|------|------|
| `\n` | 换行 | `\t` | 制表符 |
| `\"` | 双引号 | `\\` | 反斜杠 |
| `\r` | 回车 | `\0` | 空字符 |

```z42
// examples/basics/strings/escapes/escapes.z42
{{#include ../../../../examples/basics/strings/escapes/escapes.z42:code}}
```

```console
{{#include ../../../../examples/basics/strings/escapes/run.console:run}}
```

**不认识的转义是编译错误**，不会被悄悄吞掉。Windows 路径最容易撞上：

```z42
// examples/basics/strings/escapes/badescape.z42
{{#include ../../../../examples/basics/strings/escapes/badescape.z42}}
```

```console
{{#include ../../../../examples/basics/strings/escapes/run.console:bad}}
```

要么写 `"C:\\Users\\bin"`，要么用下面的原始串。

> 熟悉 C# 的读者请注意：**`\uXXXX` 和 `\xXX` 不支持**，写了同样报 `E0102`。
> 完整转义表见参考手册的
> [字符串](https://z42-lang.github.io/z42/reference/language/strings.html)。

## 插值串 `$"…"`

花括号里可以是任何表达式——变量、算术、方法调用都行：

```z42
// examples/basics/strings/interp/interp.z42
{{#include ../../../../examples/basics/strings/interp/interp.z42:code}}
```

```console
{{#include ../../../../examples/basics/strings/interp/run.console:run}}
```

想打印花括号本身，写两个：`{{` 和 `}}`。

### 🔴 格式说明符不起作用，而且不报错

C# 里 `$"{x:X2}"` 能把数字按十六进制两位输出。**z42 不支持这个写法，但也不会拦你**——
冒号后面的部分被直接丢掉：

```z42
// examples/basics/strings/interp/format.z42
{{#include ../../../../examples/basics/strings/interp/format.z42:code}}
```

```console
{{#include ../../../../examples/basics/strings/interp/run.console:format}}
```

`#FF` 没出现，`3.14` 也没出现。需要定宽或换进制时，自己拼接或调用相应的转换方法。

## 原始串 `"""…"""`

三个双引号包起来，**里面的内容逐字保留**：不处理转义，引号不用躲，换行和缩进原样进字符串。
写正则、JSON、Windows 路径时特别省事：

```z42
// examples/basics/strings/raw/raw.z42
{{#include ../../../../examples/basics/strings/raw/raw.z42:code}}
```

```console
{{#include ../../../../examples/basics/strings/raw/run.console:run}}
```

注意输出里的缩进——**原始串不会帮你剥掉缩进**，`第一行` 前面那两个空格是字符串的一部分。

> 熟悉 C# 11 的读者请注意三点不同：分隔符**固定三个** `"`（没有变长形式，所以内容里不能出现
> 连续三个引号）；**没有 `$"""…"""`**，要插值就用 `+` 拼；**不剥缩进也不剥首尾换行**。

## 常用操作

```z42
// examples/basics/strings/members/members.z42
{{#include ../../../../examples/basics/strings/members/members.z42:basic}}
```

```console
{{#include ../../../../examples/basics/strings/members/run.console:basic}}
```

> ⚠️ **`Length` 是属性，不是方法**——写 `s.Length`，不是 `s.Length()`。

`IndexOf` 找不到时返回 `-1`，这是个约定俗成的写法，记住它。所有这些方法都**返回新串**，
`s.Trim()` 不会改变 `s` 自己。

### 切分与拼接

```z42
// examples/basics/strings/members/splitjoin.z42
{{#include ../../../../examples/basics/strings/members/splitjoin.z42:code}}
```

```console
{{#include ../../../../examples/basics/strings/members/run.console:splitjoin}}
```

`String.Join` 和 `String.IsNullOrEmpty` 是**静态方法**，写类名调用，不是 `s.Join(...)`。

### `Split` 只收字符串，不收字符

C# 里 `s.Split(',')` 是常见写法，**z42 没有 `Split(char)`**，编译器会拦下来：

```z42
// examples/basics/strings/split/splitchar.z42
{{#include ../../../../examples/basics/strings/split/splitchar.z42}}
```

```console
{{#include ../../../../examples/basics/strings/split/run.console:err}}
```

**用双引号**：`csv.Split(",")`。这是从 C# 过来最容易带错的一个习惯，
好在编译器帮你挡着。

### 字符数 ≠ 字节数

`Length` 数的是**字符**，`ByteLength` 数的是 UTF-8 **字节**，两者对非 ASCII 文本不同：

```z42
// examples/basics/strings/members/bytes.z42
{{#include ../../../../examples/basics/strings/members/bytes.z42:code}}
```

```console
{{#include ../../../../examples/basics/strings/members/run.console:bytes}}
```

按人眼看到的字数算就用 `Length`；算存储大小、网络长度就用 `ByteLength`。

## 逐字符遍历

`foreach` 和下标都可以，两者都按**字符**走（不是字节）：

```z42
// examples/basics/strings/iterate/iterate.z42
{{#include ../../../../examples/basics/strings/iterate/iterate.z42:code}}
```

```console
{{#include ../../../../examples/basics/strings/iterate/run.console:run}}
```

## 小结

- 三种字面量：普通 `"…"` 处理转义、插值 `$"…"` 嵌表达式、原始 `"""…"""` 逐字保留。
- **不认识的转义是编译错误**（`E0102`）；`\uXXXX` / `\xXX` 不支持。
- 插值里 `{{` `}}` 表示字面花括号；**`{x:F2}` 这类格式说明符静默失效**，别用。
- 原始串固定三个引号、不能插值、**不剥缩进**。
- `Length` 是**属性**且数的是字符；字节数用 `ByteLength`。
- **`Split` 要传字符串 `","`**，传字符 `','` 编译期就报错。
- `String.Join` / `String.IsNullOrEmpty` 是静态方法。
- 字符串不可变，所有方法都返回新串。

下一章讲**数组与集合**——怎么装一批数据。
