# 语法定制：三层配置机制

> **页型**: 决策页 ｜ **状态**: 📋 **设计已定 / 未实施** ｜ **代码**: `src/libraries/z42c.core/src/LanguageFeatures.z42`（全仓唯一实现物，且无调用方）
> **相关**: [源代码编译流程](source-compile.md) · [架构总览](architecture.md) · [工程模型、依赖解析与工作区编译](project-model.md) · [元编程 / 编译期代码生成](metaprogramming.md) · [脚本化 charter（未实施）](scripting-charter.md) ｜ **对齐**: 2026-09-17

> **状态：设计已定 / 未实施。** 现状只有第 1 层的**数据结构**，第 1 层的**配置来源**、第 2 层、第 3 层全都没有：
>
> | 设计物 | 现状 | 证据 |
> |--------|------|------|
> | `LanguageFeatures` 类 | **存在但零调用方** —— 定义在 `src/libraries/z42c.core/src/LanguageFeatures.z42`，全仓只有它自己的单测 `src/libraries/z42c.core/tests/features.z42` 引用它。`z42c.syntax` / `z42c.ir` / `src/compiler` / `src/runtime` / `src/toolchain` 无任何命中，编译器从不读它 | `grep -rn LanguageFeatures src/` |
> | `ParseTable` 类 | **不存在**。全仓唯一字面命中是 `src/libraries/z42.toml/src/TomlParser.z42` 的 `ParseTableHeader()`——TOML 解析器的无关同名方法 | `grep -rn ParseTable .` |
> | `z42.toml` 的 `[syntax]` 节 | **不存在**。`src/libraries/z42.project/src/` 下 21 个 manifest 模型文件里搜 `syntax` 零命中 | `grep -rni syntax src/libraries/z42.project/src/` |
> | `using syntax` 指令 | **不存在**，词法器与解析器均无此形态 | `grep -rn "using syntax" src/` |
> | `operator` / `keyword` 用户声明 | **不存在** | 同上 |
>
> 另有两个遗留物需要知道：
>
> - **优先级数值已经存在，但硬编码在代码里**，不是数据表。`src/libraries/z42c.syntax/src/ExprParser.z42` 的 `_infixBp()` 是一串 `if` 链，三目 / 赋值 / `is` / `as` / `switch` / `with` 则以 `minBp <= N` 的形式内联在 `_parseExpr()` 中。也就是说**解析器是 Pratt 式优先级攀升（这点属实），但并非表驱动**——第 2 层要做的正是把这些数值提取成数据。
> - 曾有两个 `features.toml` sidecar（`src/tests/control_flow/switch/` 与 `src/tests/exceptions/exceptions/`）号称能 override `LanguageFeatures`，实则**全仓没有任何代码读取**——唯一读过它们的是随自举删除的 C# `GoldenTests.cs`。已于 2026-09-23 删除；第 1 层接线时若需要按用例覆盖特性，重新设计即可，不必迁就那两个空壳。

---

## 设计原则

> **代码提供机制，配置提供策略。**

- 编译器**内核**只管"怎么词法/语法/类型检查/生成代码"；
- **哪些特性可用**、**哪些运算符有效**、**检查有多严**，由**数据驱动的配置**决定；
- 新增一个运算符、禁用一类语句、收紧一条规则，理想形态下**只改配置，不改内核代码**。

这条原则要服务的是同一套编译器的两个身份：既是**完整的系统编程语言**，又能当**可裁剪的嵌入式脚本引擎**（后者与 [脚本化 charter](scripting-charter.md) 描述的 `Compile(source)` / `Eval(source)` 目标形态互为前后件）。

---

## 三层总览

```
第 1 层  特性开关   LanguageFeatures  —— 开关内置特性（布尔集合）
           配置来源 ① z42.toml [syntax] 节     → 整个项目
           配置来源 ② using syntax "..." 指令  → 单个源文件
第 2 层  规则表     ParseTable        —— 运算符优先级与语句规则的数据化单一真相源
           表项可挂 feature 名 → 第 1 层的开关在此生效
第 3 层  用户定义   operator / keyword —— 运行时向第 1/2 层注册全新 token 与规则
```

> **命名冲突（已裁决）**。被合并的两份设计稿对"三层"的切法不同：一份按**编译器内部扩展点**切（`LanguageFeatures` / `ParseTable` / Handler 函数），另一份按**配置来源**切（项目级 `z42.toml` / 文件级 `using syntax` / 用户定义语法）。两者是正交的两条轴，并排堆会让"第 2 层"一词有两个含义。本页采用**扩展点轴**作骨架（上表），把配置来源轴降为第 1 层内部的两个**来源**；原属"第 3 层 Handler 函数"的内容并入第 2 层，因为 handler 只是表项指向的实现，不是独立的定制面。

---

## 第 1 层：特性开关（LanguageFeatures）

顶层定制点。每个特性由一个 `snake_case` 字符串标识，取布尔值。

### 已实现的数据结构

`src/libraries/z42c.core/src/LanguageFeatures.z42` 定义 `Z42.Core.LanguageFeatures`：

- 受语言限制（当时无 enum、类字段不支持泛型），内部是**并行数组** `string[] _names` + `bool[] _enabled` + `int _count`，而非字典；
- `Set(name, on)` 为 upsert；
- **`IsEnabled(未知名) = false`** —— 拼错的特性名不会静默启用，这是刻意的安全默认；
- 另有 `int Phase` 字段（1 = Phase1，2 保留）；
- 两个预置 profile：`Phase1Profile()`（21 个特性全开）与 `MinimalProfile()`（仅 `interpolated_str` / `ternary` / `cast` / `bitwise`）。

再次提醒：**没有任何编译器代码构造或查询它**。解析器不接受 `LanguageFeatures` 参数，因此今天关掉任何开关都不会改变解析行为。

### 特性名清单

以 `Phase1Profile()` 的实际内容为准（这是唯一有代码背书的名单）：

| 特性名 | 控制内容 |
|--------|----------|
| `control_flow` | `if` / `while` / `do` / `for` / `foreach` / `break` / `continue` |
| `exceptions` | `try` / `catch` / `finally` / `throw` |
| `pattern_match` | `switch` 表达式与语句 |
| `oop` | `class` / `interface` / `struct` / `record` / `new` |
| `generics` | 泛型类型与方法 |
| `arrays` | 数组创建与下标 |
| `bitwise` | `&` `\|` `^` `~` `<<` `>>` 及其复合赋值 |
| `nullable` | `T?` 空检查标记（引用类型；值类型 `int?` 报 E0476）|
| `ternary` | `? :` 三目运算符 |
| `cast` | `(Type)expr` 显式转换 |
| `lambda` | Lambda 表达式 `=>` |
| `delegates` | 函数类型 `(T) -> R`、`delegate` / `event` |
| `tuples` | 元组类型与字面量 |
| `interpolated_str` | `$"..."` 字符串插值 |
| `reflection` | `typeof` / `methodof` |

> 🔴 **这张表只列真实存在的语法构造。** 2026-09-23 删掉了 6 个名字，它们指向的语法
> **在 z42 里根本不存在**——而它们曾以「已启用」的姿态躺在 `Phase1Profile()` 里：
>
> | 删掉的名字 | 事实 |
> |---|---|
> | `using_stmt` | z42 没有 `using` 语句，`using` 只做 import / 别名 |
> | `null_coalesce` | `??` / `?.` 已从语言移除，parser 见到报 E0480 |
> | `async` | `await` 从不被任何 parser 消费，也没有 `Task` 类型 |
> | `list_patterns` | 全仓零 `ListPattern`，模式解析器没有 `[` 分支 |
> | `threading` | 库有（`z42.threading`），但 `lock` 连关键字都不是 |
> | `native_interop` | FFI 真有（`[Native]` + dlopen），但挂它名下的 `pinned` 是个没人消费的死 token |
>
> 之所以能长期没人发现，是因为**开关零调用方**：没有任何门能检验「这个名字背后真有语法」。
> 第 1 层接线之后才谈得上真门 —— 判据见下面「实施路径」的第 4 条（开启/关闭**两个**测试）。
> 在那之前，加名字前请自己确认 parser 真有消费该语法的代码路径。

### 配置来源 ①：项目级 `z42.toml [syntax]`

```toml
[syntax]
# 关闭不需要的特性（未列出的沿用 profile 默认）
exceptions      = false
pattern_match   = false
lambda          = false
bitwise         = true
```

由 manifest 加载路径读入，构造出该项目的 `LanguageFeatures`，交给编译流水线。落地点是 `src/libraries/z42.project/`（manifest 模型 + `ManifestLoader`）与 `src/compiler/z42c.pipeline/`（把它传进编译单元），详见 [工程模型、依赖解析与工作区编译](project-model.md)。

### 配置来源 ②：文件级 `using syntax`

单个源文件可在**所有命名空间声明和实际代码之前**覆盖项目配置：

```z42
using syntax exceptions = false;
using syntax bitwise = true;
```

- 作用范围：当前编译单元；
- 与项目配置取**并集**：文件可以额外开启特性，但**不能突破工作区级的禁用**——否则 `[syntax]` 的禁用就不再是安全边界。

---

## 第 2 层：规则表（ParseTable）

`ParseTable` 是运算符优先级与语句解析规则的**单一真相源**。今天这些信息散落在 `ExprParser` / `StmtParser` 的代码里，本层的工作就是把它们提出来。

### 表达式规则

每个运算符一条表项，概念形态：

```
[TokenKind.Plus] = ParseRule(
    leftBp: 70,              // 作为中缀时的绑定力
    nud:    Nuds.Unary,      // 前缀处理器（无则 null）
    led:    Leds.BinaryLeft, // 中缀/后缀处理器
    feature: null            // null = 永远启用；否则为第 1 层的特性名
)
```

`feature` 字段就是第 1 层与第 2 层的接缝：解析器在查表时跳过 `IsEnabled(feature) == false` 的表项，该运算符对当前编译单元等同于不存在。**特性门控只发生在解析期，运行期零开销。**

### 绑定力分级

下表是 `ExprParser.z42` 中**当前实际生效**的数值（`_infixBp()` 的 `if` 链，加上 `_parseExpr()` 内联的 `minBp <= N` 判断）。第 2 层不改变这些数值，只是把它们搬进表里。层级之间留 10 的间隔，便于插入：

```
10  赋值 / 复合赋值      =  += -= *= /= %= &= |= ^=   （右结合）
20  三目                ? :                          （右结合）
30  逻辑或              ||
40  逻辑与              &&
44  按位或              |        [feat:bitwise]
46  按位异或            ^        [feat:bitwise]
48  按位与              &        [feat:bitwise]
50  相等                == !=
60  比较 / 类型测试      < <= > >= is as
65  移位                << >>    [feat:bitwise]
70  加减                + -
80  乘除模              * / %
85  后缀 switch / with  subject switch { ... } / target with { ... }
```

后缀链（`.` / `?.` / `[]` / 调用）由 `_parseExpr()` 的后缀循环无条件处理，没有对应的 bp 常量——这是与"每个运算符一条表项"的理想形态的一处现存偏差，第 2 层需要决定是否把它也表格化。

### 语句规则

每个引导关键字一条表项：

```
[TokenKind.For] = StmtRule(
    handler: Stmts.For_,     // 解析函数
    feature: "control_flow"  // 可选的特性门
)
```

新增一条语句因此变成两步：**（1）在 `StmtRules` 里加表项并写上可选特性名；（2）实现对应的 handler 函数。**

### Handler 函数

表项指向的实现，按 Pratt 解析的三个角色分组（这是**内核扩展点**，不是面向用户的配置面——加 handler 要改编译器源码）：

| 角色 | 职责 | 形态 |
|------|------|------|
| **Nud**（null denotation） | 前缀与原子表达式 | `(ctx, token) -> Expr` |
| **Led**（left denotation） | 中缀与后缀表达式 | `(ctx, left, token) -> Expr` |
| **Stmt** | 语句解析 | `(ctx, token) -> Stmt` |

### 一致性守门

规则表一旦成为数据，就需要一条自动检查：**`ParseTable` / `StmtRules` 中出现的每个 `feature` 名，必须在 `LanguageFeatures` 的已知名单里声明**。没有它，一个拼错的特性名会让该表项被 `IsEnabled` 静默判为 `false`，对应语法凭空消失且无任何报错——这正是 `IsEnabled(未知名) = false` 这条安全默认的代价。此检查目前不存在（`LanguageFeatures` 也还没有"已知名单"这个概念，只有两个 profile）。

---

## 第 3 层：用户定义语法

前两层只能开关**编译器已实现**的语法。第 3 层允许源码声明**编译器未内置**的词法/文法规则，运行时注册进第 2 层的表。

### 3a. 自定义中缀运算符

```z42
// 声明一个新的中缀运算符，绑定力与加法同级（bp 70）
operator infix "++" as "strcat" bp 70;

var s = "hello" ++ " world";
```

语法：

```
operator infix  <symbol> as <内置运算符 | 函数名> bp <int>;
operator prefix <symbol> as <内置运算符 | 函数名> bp <int>;
```

- `symbol` —— 由非字母数字字符组成的新 token，不得与现有 token 冲突；
- `as` —— 映射目标，可以是已有运算符名（`"+"`、`"-"`，即直接别名），也可以是用户函数名（展开为 `f(left, right)`）；
- `bp` —— 绑定力，建议落在上表的现有层级上。

### 3b. 自定义关键字语句

```z42
// 把 "unless" 声明为 if (!cond) 的语法糖
keyword stmt "unless" (cond: Expr) (body: Block)
    = if (!cond) body;
```

语法：

```
keyword stmt <name> (<param>: <AstKind> ...) = <脱糖模板>;
keyword expr <name> (<param>: <AstKind> ...) = <脱糖模板>;
```

- `AstKind` 取值：`Expr` / `Block` / `Type` / `Ident`；
- 脱糖模板用参数名组合，在 Parse 阶段展开成标准 AST。

### 3c. 关键字别名

```z42
keyword alias "fn"    = "void";
keyword alias "elsif" = "else if";
```

### 作用域与可见性

| 配置来源 | 作用范围 |
|----------|----------|
| `z42.toml [syntax]` | 整个项目 |
| `using syntax` | 当前源文件 |
| `operator` | 当前源文件及其依赖方 |
| `keyword` | 当前源文件 |

`operator` / `keyword` 声明可以集中放在独立文件里，由 `using` 引入：

```z42
using "my_syntax.z42";   // 导入语法扩展
```

### IR 映射：一切在 Parse 阶段收敛

用户定义语法**只在 Parse 阶段展开**，不引入新 AST 节点，也不引入新 IR 指令：

```
source.z42
  └─ 词法分析（含用户注册的 token）
       └─ Parse 阶段展开（operator / keyword 脱糖）
            └─ 标准 AST（与不用扩展时逐字节相同）
                 └─ TypeCheck → IrGen → IR（完全不感知语法扩展）
```

这是本设计最重要的一条边界：**后端对语法扩展一无所知**，因此 zbc 格式、类型检查器、VM 都不因第 3 层而变动。详见 [源代码编译流程](source-compile.md)。

### 约束与限制

1. **不允许**覆盖核心关键字（`if` / `while` / `class` / `return` 等）；
2. **不允许**定义绑定力为 0 的中缀运算符（`_infixBp` 用 0 表示"不是中缀"，会破坏解析循环的终止条件）；
3. 自定义 `symbol` 必须由 `!@#$%^&*|<>?~` 中的字符组成，且**不能是现有 token 的前缀**；
4. `keyword stmt` 的展开必须是确定性的，不得引用外部状态；
5. 循环引用的语法扩展（A 的展开引用 B，B 的展开引用 A）在编译期报错；
6. **没有任何一层能绕过类型安全** —— 特性门只开关语法，类型检查器始终是严格的。

---

## 使用场景

### 场景 1：嵌入式游戏脚本引擎

只放开算术、控制流、数组和基础 I/O，禁掉 OOP、异常、反射、线程：

```toml
[syntax]
control_flow     = true
arrays           = true
interpolated_str = true
oop              = false
exceptions       = false
threading        = false
reflection       = false
```

结果：脚本能写 `if` / `for` / 数组 / `Console.WriteLine`，但写不出类、异常和线程；宿主拿到的 zbc 因此有一个可静态论证的能力上界。

### 场景 2：教学环境（特性渐进放开）

从 `MinimalProfile()` 起步，每周在 `[syntax]` 里多打开一项：第 1 周只有基础类型与输出，第 2 周加 `control_flow`，第 3 周加 `oop`。学生越界使用尚未讲授的语法会在**解析期**直接报错，而不是写出一段看不懂的程序。

### 场景 3：受限生产环境

高安全服务端禁用反射、限制可空引用、强制单线程：

```toml
[syntax]
reflection    = false   # 无 typeof / nameof
threading     = false   # 单线程
nullable      = false   # 无 T?，只用 Result / Option 一类显式载体
pattern_match = false
```

### 场景 4：实验性运算符（幂运算 `**`）

在不影响既有代码的前提下加一个运算符，三步：

1. **加表项** —— `[TokenKind.StarStar] = ParseRule(leftBp: 75, nud: null, led: Leds.BinaryLeft, feature: "pow_operator")`；75 落在加减（70）与乘除（80）之间，是刻意留出的插入位；
2. **注册特性** —— 在 `MinimalProfile()` 里置 `false`，在 `Phase1Profile()` 里置 `true`；
3. **使用** —— `int power = 2 ** 8;`，仅当 `pow_operator` 开启时成立。

若不想改编译器，同一件事可以走第 3 层，在源码里声明：

```z42
operator infix "**" as Math.Pow bp 75;

var x = 2 ** 10;   // 展开为 Math.Pow(2, 10)
```

> 注意**不能**用 `^` 做幂运算：`^` 已是按位异或（bp 46）的既有 token，与上面的约束 3 直接冲突。被合并的设计稿之一曾用 `^` 举例，此处已改正为新 token `**`。

---

## 实施路径

分三批，与三层对应：

| 批次 | 内容 | 落点 |
|:----:|------|------|
| **1** | 第 1 层配置来源 ①：`z42.toml` 解析出 `[syntax]` 节并构造 `LanguageFeatures`；把它一路传进解析器，**让第一个特性门真正生效**（当前所有开关都是死的） | `src/libraries/z42.project/`（manifest 模型 + loader）、`src/compiler/z42c.pipeline/`、`src/libraries/z42c.syntax/` |
| **2** | 第 1 层配置来源 ②：词法/解析入口识别 `using syntax` 指令，按并集规则叠加到项目配置上 | `src/libraries/z42c.syntax/src/Lexer.z42` · `Parser.z42` |
| **3** | 第 2 层：把 `_infixBp()` 与 `StmtParser` 的分发提取成 `ParseTable` / `StmtRules` 数据表，表项挂 `feature`；补一致性守门 | `src/libraries/z42c.syntax/src/ExprParser.z42` · `StmtParser.z42` |
| **4** | 第 3 层：词法器支持运行时注册 token pattern，`ParseTable` 支持运行时插入表项，实现 `operator` / `keyword` / `keyword alias` 的解析与展开 | 同上 + 新增脱糖模块 |

顺序不可换：批 3 的表格化是批 4 运行时插入的前提，批 1 不落地则任何特性门都无从验证。

（两个无人读取的 `features.toml` sidecar 已于 2026-09-23 删除，批 1 不必再迁就它们。）

新增一个特性的完整清单（批 1–3 落地后）：

1. 在 `Phase1Profile()` / `MinimalProfile()` 中声明该名字；
2. 在 `ParseTable` 或 `StmtRules` 中把表项的 `feature` 指向它；
3. 实现 Nud / Led / Stmt handler；
4. 写**两个**测试：开启时能解析、默认关闭时报错——只写前者等于没测门控；
5. 一致性守门自动校验名字拼写。

---

## 与其他方案的对比

| 方案 | 运行时特性门 | 配置驱动 | 适合嵌入 |
|------|:---:|:---:|:---:|
| **z42 三层机制** | ✅ | ✅ | ✅ |
| pest（`.pest` 文法文件） | ❌（编译期固定） | ✅ | ❌ |
| nom（组合子） | ❌ | ❌ | ❌ |
| Roslyn Scripting | ❌ | ❌ | ✅（但无法裁剪） |
| Python（`sys.modules` 级裁剪） | ✅ | ✅ | ✅（但模型不同：裁库不裁语法） |

z42 的差异点是**在解析期完成门控**：同一个编译器二进制可服务多种方言，而生成的字节码与 VM 执行路径不含任何特性判断，因此**运行期零开销**。

---

## 未来扩展（L3+）

- **属性级门控** —— 在单个函数上标 `[Strict]` 施加更严的规则；
- **能力式安全** —— 比布尔开关更细的权限（"可调用 native"、"可做 I/O"），与 [访问权限强制](access-control.md) 的模型合流；
- **ABI 版本锁** —— 冻结结构体布局，防止二进制破坏；
- **语言 profile** —— 预置组合（`safe` / `performance` / `embedded`），避免每个项目手写一长串 `[syntax]`。

语法层定制与**元编程**（`[derive(...)]`、编译期代码生成）是正交的两件事：本页管"能写出什么形状的源码"，[元编程 / 编译期代码生成](metaprogramming.md) 管"已合法的源码如何自动展开出更多代码"。两者都在 Parse/Bind 阶段收敛，都不改动 IR。
