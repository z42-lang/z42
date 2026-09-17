# 命名空间与 `using`

> 对齐：2026-09-17 ｜ 实测基准：`./.z42/z42 run`

## 语法

```
namespace_decl ::= "namespace" dotted_name ";"
using_decl     ::= "global"? "using" ( alias "=" type_expr | dotted_name ) ";"
dotted_name    ::= IDENT ( "." IDENT )*
```

- 每个文件至多一条 `namespace`，必须在所有 `using` 和所有顶层声明之前。
- 所有 `using` 必须在顶层声明之前。
- 不支持 block-scoped namespace（`namespace Foo { ... }`）。

## 命名空间声明

```z42
namespace Demo;

class Point { }
void Helper() { }
```

声明的限定名随之改变：

| 声明 | 无 namespace | `namespace Foo` 下 |
|---|---|---|
| 顶层函数 `void Bar()` | `Bar` | `Foo.Bar` |
| 类 `class Baz` | `Baz` | `Foo.Baz` |
| 类方法 `class Baz { void M() }` | `Baz.M` | `Foo.Baz.M` |
| 构造函数 `class Baz { Baz() }` | `Baz.Baz` | `Foo.Baz.Baz` |

无 `namespace` 的文件归属默认命名空间 `main`。限定名就是栈跟踪里看到的名字，
也是清单里 `entry` 要写的名字。

## `using` 导入

```z42
using Std.IO;
using Std.Collections;
```

**规则**：

1. `z42.core` 是唯一隐式 prelude，它提供的 `Std` 与 `Std.Runtime` 两个命名空间无需 `using`。
2. **其它一切都必须显式 `using`**——包括 `Std.IO`、`Std.Collections`、`Std.Text`、`Std.Math`、`Std.Test` 等
   同样以 `Std.` 打头的 stdlib 命名空间。
3. `using X;` 激活所有声明了命名空间 `X` 的包。
4. **没有全限定名逃生口**：`Std.IO.Console.WriteLine("hi")` 不写 `using` 也不行，报 `E0401: undefined: Std`。

```z42
new Object()                      // ✓ Object 在 Std（prelude）
new List<int>()                   // ✗ List 在 Std.Collections —— 必须 using
Console.WriteLine(...)            // ✗ Console 在 Std.IO —— 必须 using
```

### `using` 是**文件级**的

每个源文件**实际用到的跨包依赖命名空间**必须被**本文件**的 `using` 覆盖
（再并上 prelude 的 `{Std, Std.Runtime}` 与本文件自己的 `namespace`），否则报：

```
E0436: namespace `Std.Collections` is used but not imported in this file; add `using Std.Collections;`
```

同包内的跨命名空间引用不受此约束（同一包的多个文件通过符号收集互见，那由工作区链接解析，不是 `using` 的事）。

> 历史上 `using` 事实上是**包级泄漏**的：一个文件写了 `using Std.Text;`，整个包的文件都能用
> `StringBuilder`。删掉兄弟文件的 `using` 会让不相关的文件神秘编译失败。现改为强制文件级，
> 与 C# / Rust / Python / Go / TS 一致。

### `global using`（逃生舱）

```z42
// prelude.z42，包内写一处
global using Std.IO;
global using Std.Collections;
// 包内其它文件无需再 using 即可用 Console / List<T>
```

`global using` **包级生效**：注入到包内每个文件的 `using` 集合，从而满足各文件的文件级检查。
`global` 是**上下文 token**，不是新关键字——只在文件顶层紧跟 `using` 时被识别。

适合团队 prelude、真正处处要用的命名空间；其余仍建议逐文件 `using`。

## 类型别名 `using Id = T;`

给类型起一个**文件级**别名，用来压缩长泛型类型或做语义命名：

```z42
using UserId = int;                       // 基本类型
using Row    = Dictionary<string, int>;   // 压缩长泛型
using Names  = List<string>;

class Account { public UserId Id; }       // 字段 / 参数 / 返回 / 局部 / new 位置皆可
UserId next(UserId c) { return c + 1; }
Row r = new Row();                        // 泛型别名可作 new 的类型
```

- **语法**：`using` 后紧跟 `Identifier =` 即别名声明（区别于 `using ns;` 与 `global using`）。
  目标可以是任意类型表达式，含泛型实参。
- **语义**：别名与目标类型**完全互通**——`UserId` 就是 `int`，可以互相赋值。
- **作用域**：文件级，只在声明它的文件里生效；别名本身**不导出**。
- 别名在类型解析处即被替换，因此它对重载、赋值兼容性都不产生任何额外区分。
- 类型形参优先于别名：类里有 `T` 时，`using T = int;` 不会影响 `T` 的解析。

## 多文件 / 多包编译

包编译器把每个源文件当一个编译单元（CU）处理，分三步：

1. **全量解析**——所有 CU 解析成 AST，收集各自的 `using`。
2. **符号收集**——用「激活包过滤后的导入符号」收集每个 CU 的类 / 接口 / 函数形状。
3. **类型检查 + 代码生成**——绑定函数体；用到未激活包的类型即报错。

**激活包的计算**：prelude（`z42.core`）恒激活；用户每条 `using <ns>;` 激活声明了该命名空间的所有包；
同包内多个 CU 互相可见，无需彼此 `using`。

## 入口函数

**优先看清单**：`[project].entry` 或 `[[exe]].entry` 写的全限定名直接生效。
`[[exe]]` 目标**必须**写 `entry`，否则报 `[[exe]] <name> 缺少 entry（全限定 Main 函数名）`。

清单没写时才自动探测导出函数，按以下四级优先取第一个命中的层级：

1. 以 `.Main` 结尾的全限定名
2. 裸 `Main`
3. 以 `.main` 结尾的全限定名
4. 裸 `main`

**同一层级出现多个候选即为歧义**，编译失败：

```
z42c build: multiple Main candidates; set [project].entry explicitly
```

`kind = exe` 但一个候选都没有：

```
z42c build: kind=exe but no Main() found
```

入口名会被烤进 `.zpkg`。直接用 `z42vm` 跑一个没有烤入口的模块，必须把函数名作为第二个位置参数传进去。

## 诊断

| 码 | 何时出现 | 状态 |
|---|---|---|
| `E0436` | 本文件用到某依赖命名空间却没 `using` 它 | 生效 |
| `E0401` | 用到未激活包里的符号，或写了没有 `using` 的全限定名 | 生效 |
| `E0601` | 同一个**全限定名**被两个以上的**依赖包**声明——限定名也分不开它们，谁都选不中 | 生效 |
| `E0606` | 本包声明的类型**遮蔽**了某个导入包的同全限定名类型——被遮的那份无论怎么写都指不到 | 生效 |
| `E0602` | `using <ns>;` 无任何已加载包提供 | **未实现**：有码无发射点，`using NoSuch.Pkg;` / `using System;` 都静默通过 |
| `W0603` | 非 stdlib 包（不以 `z42.` 开头）占用 `Std` / `Std.*` 命名空间 | **未实现**：有码无发射点 |

`E0601` 与 `E0606` 的分工：前者是两个**第三方依赖**打架，下游既没用到也无权修，所以只在实际引用该类型时才报；
后者冲突的两份里**有一份是本包自己写的**，改名随时可以，因此是 error 而非 warning。

### 语法错误

| 情形 | 消息 |
|---|---|
| `namespace` 出现在顶层声明之后 | `namespace declaration must appear before any top-level declarations` |
| 同一文件两条 `namespace` | `duplicate namespace declaration` |
| `using` 出现在顶层声明之后 | `using directive must appear before any top-level declarations` |

### 缺 `using` 怎么补

z42c 的报错会精确点名缺失的命名空间，按提示手动补一行即可。
（旧的正则启发式自动补齐工具 `xtask audit` 已于 2026-07-07 移除。）

## 示例

```z42
namespace Demo;

using Std.IO;

class Point {
    int X;
    int Y;
    Point(int x, int y) { this.X = x; this.Y = y; }
    string ToString() { return $"({this.X}, {this.Y})"; }
}

void Main() {
    var p = new Point(3, 4);
    Console.WriteLine(p.ToString());
}
```

生成的限定名：`Demo.Point.Point`、`Demo.Point.ToString`、`Demo.Main`；入口 = `Demo.Main`。

## 相关

- [访问控制](access-control.md)——`public` / `internal` 与包边界
- [错误码](../appendix/error-codes.md)
