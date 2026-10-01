# 命名空间与 `using`

> 对齐：2026-10-01 ｜ 实测基准：`./artifacts/.z42/z42 run`

## 语法

```
namespace_decl ::= "namespace" dotted_name ";"
using_decl     ::= "global"? "using" ( alias "=" type_expr | dotted_name ) ";"
dotted_name    ::= IDENT ( "." IDENT )*
```

- 每个文件至多一条 `namespace`（违反 → `E0457`），且必须在**所有类型/函数声明**之前（违反 → `E0457`）。
- ⚠️ **`namespace` 与 `using` 的相对顺序不受约束** —— `using` 写在 `namespace` 之前照样合法，
  且 `namespace` 正常生效。这是刻意的：`using` / `global using` / 类型别名**不算声明**
  （`z42c.syntax` 的 parser 单测钉着这条）。
- ⚠️ **`using` 出现在顶层声明之后目前不报错**（能编过）。习惯上仍应全部写在文件顶部。
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
5. **外围命名空间隐式可见**（与 C# 相同）：写在 `namespace A.B` 里的代码，不写 `using` 就能用 `A.B` 与 `A`
   里的类型和函数。名字查找由内向外：`A.B` → `A` → `using` 进来的命名空间（含 prelude）。外围命名空间里的
   那一份**胜过** `using` 进来的同名者，不算歧义；多层外围都有时**最内层**胜出。外围按段算：`AB` 不是
   `A` 的外围。

```z42
new Object()                      // ✓ Object 在 Std（prelude）
new List<int>()                   // ✗ List 在 Std.Collections —— 必须 using
Console.WriteLine(...)            // ✗ Console 在 Std.IO —— 必须 using
```

### `using` 是**文件级**的

每个源文件**实际用到的命名空间**——不论在依赖包里还是**同一个包**里——必须被**本文件**的 `using` 覆盖
（再并上 prelude 的 `{Std, Std.Runtime}`、全局命名空间，与本文件 `namespace` 及其外围命名空间），否则报：

```
E0436: namespace `Std.Collections` is used but not imported in this file; add `using Std.Collections;`
```

源码里写出的类型名（如 `Console`、`List<int>`）报在**引用处**；按 C# 规则，这样的名字在本文件根本解析不到，
编译器只是认出它在哪个命名空间、替你指路。

「用到」按 C# 的口径算：源码里**写出**的类型名（局部变量 / 字段 / 形参 / 返回 / 基类与接口列表 / 约束 /
`new` / 转换 / `is` / `as` / `typeof` / 泛型实参 / delegate 类型）、静态成员与静态调用的类名、enum 常量、自由函数调用与
函数引用。限定写法 `A.W` 同样算用到 `A`（规则 4）。编译器**合成**的类型不算：`[1, 2]` 生成 `List<int>`、`(1, 2)` 生成
元组类型，都不要求 `using Std.Collections;`。`using Id = 全限定名;` 别名的目标也不算。

> 2026-10-01 前，同包内的跨命名空间引用不受此约束；按 C# 规则收紧后（using-csharp-rules）同包也要 `using`。

> 历史上 `using` 事实上是**包级泄漏**的：一个文件写了 `using Std.Text;`，整个包的文件都能用
> `StringBuilder`。删掉兄弟文件的 `using` 会让不相关的文件神秘编译失败。现改为强制文件级，
> 与 C# / Rust / Python / Go / TS 一致。

### 多余的 `using` 会告警

与 C# 一样，多余的 `using` 报 warning（不阻断编译）：

```
W0607: unnecessary `using Std.Text;` — nothing in this file uses it
W0607: unnecessary `using Std;` — `Std` is part of the prelude and always visible
W0608: duplicate `using A;` — it is already imported above in this file
```

本文件已有编译错误时不报这两条（解析不全，「用到」的集合不可信）。

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

全包没有任何文件用到它时报 `W0607`（报在声明那一行）。写在它自己命名空间的文件里（`namespace A;` 的文件里写
`global using A;`）是正常写法，不报——它是给包里别的文件用的。

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
| `E0436` | 本文件用到某命名空间（依赖包或同包）却没 `using` 它 | 生效 |
| `W0607` | 不必要的 `using`：没用到，或指向 prelude / 本文件 namespace 及其外围；`global using` 全包没有文件用到 | 生效 |
| `W0608` | 重复的 `using`：同文件写了两次，或已有同名 `global using` | 生效 |
| `E0401` | 用到未激活包里的符号，或写了没有 `using` 的全限定名 | 生效 |
| `E0601` | 同一个**全限定名**被两个以上的**依赖包**声明——限定名也分不开它们，谁都选不中 | 生效 |
| `E0606` | 本包声明的类型**遮蔽**了某个导入包的同全限定名类型——被遮的那份无论怎么写都指不到 | 生效 |
| `E0494` | `using <ns>;` 指向的命名空间不存在（依赖的包里没有它，本包也没声明） | 生效（`using Z42.Totally.Bogus;` → E0494）|
| `E0602` | 同上语义的旧码 | **未实现且已被 E0494 取代**：有码无发射点 |
| `W0603` | 非 stdlib 包（不以 `z42.` 开头）占用 `Std` / `Std.*` 命名空间 | **未实现**：有码无发射点 |

`E0601` 与 `E0606` 的分工：前者是两个**第三方依赖**打架，下游既没用到也无权修，所以只在实际引用该类型时才报；
后者冲突的两份里**有一份是本包自己写的**，改名随时可以，因此是 error 而非 warning。

### 语法错误

| 情形 | 消息 |
|---|---|
| `namespace` 出现在类型/函数声明之后 | `E0457: \`namespace\` must appear before any type or function declaration in the file` |
| 同一文件两条 `namespace` | `E0457: a file may declare only one namespace (\`A\` was already declared above) — split the file, or move all declarations under a single namespace` |
| `using` 出现在顶层声明之后 | **不报错**（见上方「语法」小节的 ⚠️）|

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
