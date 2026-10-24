# doc 注释

> 代码：`src/compiler/z42c.syntax/src/DocBlock.z42`（收集与挂靠）、`src/compiler/z42.doc/src/DocParser.z42`（标记解析）、`src/compiler/z42.doc/src/`（`z42 doc` 查询）

doc 注释是写给 API **调用者**的说明：它挂在声明上，`z42 doc` 按名字查到的就是它。写给维护者的实现笔记
用普通注释 `//`。

## 三种注释

| 写法 | 挂到哪 | 写什么 |
|---|---|---|
| `///` | 紧随其后的声明 | 做什么、契约、边界、例外、示例 |
| `//!` | 所在文件的命名空间（写在 `namespace X;` 之上） | 命名空间 / 包的概述 |
| `//`、`/* */` | 不挂 | 实现笔记 |

```z42
//! 二维图形：面积、周长与缩放。
namespace Sample.Shapes;

/// 圆。
public class Circle {
    /// 半径，不小于 0。
    public double Radius;

    /// 返回面积 πr²。
    public double Area() { return 3.14159 * this.Radius * this.Radius; }
}
```

## 挂靠规则

| 规则 | 说明 |
|---|---|
| 只认行首 | `///` / `//!` 前面只能有空白；写在代码后面的是普通注释 |
| 四个及以上斜杠不算 | `////` 是普通注释，可以当分隔线 |
| 连续行合成一块 | 每行去掉 `///` 及其后一个空格；空 `///` 行是段落分隔；空行或普通注释行把块断开 |
| `///` 挂到下一个声明 | 两者之间只能有空白、普通注释和 attribute；doc 写在 attribute 之前或之后都认，约定写在之前 |
| 同一声明前有多块 | 只认最近的一块（被空行隔开的更早一块不挂） |
| `//!` 必须在文件开头 | 第一个 token 之前；多块按空行拼接 |
| 能挂 `///` 的声明 | 类 / 结构体 / 接口 / 枚举与枚举成员 / 委托 / 字段 / 属性 / 索引器 / 构造器 / 方法 / 顶层函数 / 嵌套类型 / 接口成员 / `impl` 里的方法 / 关联类型 |

没挂上任何声明的块（方法体里的 `///`、文件中间的 `//!`、文件末尾悬空的块）不进文档。

## 摘要

doc 的**第一段**（到第一个空行，或第一个以 `@` 开头的行为止）是摘要，用在 `z42 doc` 的成员轮廓与搜索结果里。
写成一句话。摘要之后、tag 区之前是正文（Markdown）。

## 标记（tag）

tag 区从第一个行首 `@` 开始，每个 tag 独占一行，续行缩进 ≥2 个空格。tag 是闭集，只有下面 8 个：

| tag | 写法 | 写什么 |
|---|---|---|
| `@param` | `@param 名 说明` | 签名表达不了的形参说明：取值范围、单位、所有权、边界行为 |


**类型位置参数**（`[Record]` / 主构造器的参数）不用 `@param`：参数直接各带一行 `///`，挂到展开后的字段上：

```z42
/// 平面上的点。
[Record] struct Point(
    /// 横坐标。
    double X,
    /// 纵坐标。
    double Y);
```
| `@typeparam` | `@typeparam T 说明` | 类型参数的约束与语义 |
| `@returns` | `@returns 说明` | 返回值说明 |
| `@throws` | `@throws 类型 何时` | 抛出的异常类型与触发条件 |
| `@see` | `@see [`符号`]` | 关联符号 |
| `@warning` | `@warning 说明` | 醒目提示（线程安全、非 CSPRNG 之类） |
| `@example` | `@example [run\|no_run\|compile_fail\|ignore]`，下一行起缩进 4 格写代码 | 示例；同时是 doctest（`z42 test --doc`） |
| `@hidden` | `@hidden` | public 但不进文档（跨包用的内部入口） |

全部 tag 都可选。只为签名表达不了的东西写，不为每个参数都补 `@param`。写了就会校验：`@param` 名必须是
真实形参、`@throws` 类型与符号链接必须能解析、未知 tag 报错（W08xx，见下）。

```z42
/// 返回 `[min, max)` 内的整数。
///
/// 用取模映射，不做 rejection sampling。
/// @param min 下界（含）
/// @param max 上界（不含）
/// @throws ArgumentException `max <= min`
public int NextIntRange(int min, int max) { ... }
```

## 不支持

- `/** */` 块形式的 doc 注释。
- 标记与链接目前**解析但不校验**：`@param` 名是否存在、`@throws` 类型与 ``[`Sym`]`` 链接能否解析、
  `@example` 是否可编译运行（doctest）都还没接上。

## 关联页面

- [`z42 doc`](../toolchain/cli-z42.md#z42-doc-options-符号) — 按名字查签名、doc 与位置
- [源文件的顶层结构](syntax.md)
