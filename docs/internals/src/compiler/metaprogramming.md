# 元编程 / 编译期代码生成

> ⚠️ **前瞻设计底稿（未实施，L3+）**。本文沉淀"为什么这样设计 + 分期"，落地各档各自开 spec。
> 相关：[reflection.md](../../../design/language/reflection.md)（共用语义 API）、[syntax-customization.md](syntax-customization.md)（语法层定制，正交）。
> 受众友好：没接触过 Rust 宏 / C# Source Generator 也能读，先看「概念扫盲」。

---

## 解决什么问题

很多代码是**机械重复**、且**随字段变化要同步改**的，手写又烦又易错：

```z42
public class Point { public int X; public int Y; }
// 想要：Equals / GetHashCode / ToString / ToJson … 全得照着 X、Y 手写一遍
// 加个字段 Z？上面每个方法都得记得改 —— 漏一个就是 bug
```

**元编程 = 让编译器在编译时替你把这些代码生成出来。** 你只写：

```z42
[derive(Eq, Hash, ToString, Json)]      // ← 一行标注，其余编译器生成
public class Point { public int X; public int Y; }
```

加字段 `Z`，生成的代码自动跟着变。这就是目标。问题只在于：**这套"生成机制"怎么设计，
才能既自由、又有类型信息、还简单好写。**

---

## 概念扫盲（没接触过宏先读这节）

**① AST（抽象语法树）** —— 代码的"结构化形态"。`x + 1` 在编译器眼里不是字符串，是一棵树：

```
  (+)
 /   \
x    1        // 加法节点，左=变量 x，右=字面量 1
```

宏操作的是这种**树**，不是字符串。为什么？字符串拼代码（`"this." + name + " == "`）没人帮你
查括号配不配、类型对不对，拼错了到处是坑；操作树则结构天然合法、能带类型信息。

**② 编译期执行** —— 普通函数在程序**运行时**跑；宏函数在**编译时**跑（z42 用自己的 VM 跑），
它的"返回值"是**一段代码（AST）**，编译器把这段代码插回你的程序继续编译。

**③ quote / splice（准引用）** —— 既然手搓 AST 节点很累，就用"把代码当模板"的写法：

```z42
quote { return true; }          // = 这段代码作为 AST 数据（不是执行它）
quote { x == ${expr} }          // ${...} = splice：把变量 expr 里的 AST 插进这个位置
```

类比你熟悉的模板字符串 `$"hello {name}"`——只不过产出的是**结构化代码**而非字符串。
`quote` 是"代码变数据"，`splice ${}` 是"数据填回代码"。

**④ hygiene（卫生）** —— 宏里引入的临时变量不会和用户代码撞名。比如宏生成了 `var __t = ...`，
而用户碰巧也有个 `__t`，编译器自动把宏的那个改名隔离，互不干扰。没有卫生，宏就会"偷偷踩到"
你的变量，是 C 宏的经典灾难。

**⑤ derive（派生）** —— 最省事的一档：`[derive(Eq)]` 让编译器**照类的字段**自动生成 `Equals`。
你一行不写。Rust 的 `#[derive(...)]`、Java 的 Lombok 都是这个。

---

## 两个参照系：各牺牲了什么

| 维度 | C# Source Generator | Rust 过程宏（proc macro）|
|---|---|---|
| 语言 | 同语言（C#）✓ | 独立 crate + `syn`/`quote` 库 ✗ |
| **语义信息** | **全类型模型**（能问"这是什么类型"）✓ | **仅 token，跑在类型检查前 → 没有类型信息** ✗ |
| **变换自由** | **只能加**（partial），不能改已有代码 ✗ | **可任意重写** ✓ |
| 好不好写 | 用字符串拼 C# 代码 ✗ | `quote!` 还行，但 `syn` 解析 token 很绕 ✗ |
| 简单场景 | 要搭一套 generator 管线样板 ✗ | `macro_rules!` ↔ proc 之间有断崖 ✗ |
| 看得见生成码 | 能 ✓ | 展开不透明、报错难懂 ✗ |

**一句话**：SourceGen「有语义但只能加、还得拼字符串」；过程宏「能任意改但没类型、工具链重」。
你的两个直觉正好戳中——**SourceGen 自由度不够，过程宏上手太难**。三者（语义 / 自由 / 易写）
它俩都只拿到两个。

---

## 同一个任务，三种语言怎么写（生成 `Equals`）

**C# Source Generator**（节选——还要先注册 IIncrementalGenerator 管线）：

```csharp
// 用字符串拼出 C# 源码，最后 AddSource 进编译
var body = string.Join(" && ", fields.Select(f => $"this.{f} == other.{f}"));
ctx.AddSource($"{name}.Eq.g.cs",
    $"public partial class {name} {{ public bool Equals({name} o) => {body}; }}");
```

**Rust 过程宏**（要单独建 proc-macro crate）：

```rust
#[proc_macro_derive(MyEq)]
pub fn derive_eq(input: TokenStream) -> TokenStream {
    let ast: DeriveInput = syn::parse(input).unwrap();   // 先把 token 解析成结构
    let fields = /* 从 ast 里掏字段，约十几行样板 */;
    quote! { impl Eq for #name { fn eq(&self,o:&Self)->bool { #(self.#fields==o.#fields)&&* } } }.into()
}
```

**z42（本设计）**——同语言、输入有类型、`quote` 吐 AST、没有独立 crate / 没有字符串拼：

```z42
[macro]
public Ast DeriveEq(TypeInfo t) {                  // t 有类型信息，能枚举字段
    Ast body = quote { true };
    foreach (Field f in t.Fields)
        body = quote { ${body} && this.${f.Name} == other.${f.Name} };
    return quote { public bool Equals(${t.Name} other) { return ${body}; } };
}
```

读起来像普通 z42，却拿到了「类型信息 + 任意生成 + 不拼字符串」。

---

## z42 的设计

### 三张别人没有的底牌

z42 能同时拿下"语义 + 自由 + 易写"，靠三样它本来就有的东西：

1. **VM** → 编译期能**直接跑 z42 代码**（宏 = 编译时执行的普通 z42 函数；类比 Zig `comptime`）。
2. **自举编译器（z42c）** → 它的 AST 本身就是个 **z42 库**；用户操作 `Ast` 节点 = 写普通 z42。
3. **反射**（正在建的 reflection 线）→ 编译期的"类型模型"复用同一套 `typeof`/`GetFields`/attribute。

> Rust 拿不到第 2、3（rustc 不是这样自举成库的，宏只有 token）；C# 为了 IDE 选了"只能加"。
> z42 三样齐全，所以不必牺牲任何一维。

### 设计主张（一句话）

> **同语言 · 编译期执行 · 操作类型化 AST · 用 quote/splice · 分层无断崖。**

### 分层：简单的事简单做（回应"上手难"）

| 层 | 覆盖 | 你要写什么 | 对标 |
|---|---|---|---|
| **T0 派生** | ~80%（相等/哈希/序列化/builder）| **只标注** `[derive(Eq, Json)]`，零代码 | Rust derive / Lombok |
| **T1 模板宏** | ~15% | 一个编译期函数，`quote{}` 拼生成 | Scala3 `Expr[T]` / Nim 宏 |
| **T2 变换宏** | ~5% | 直接模式匹配 / 重写 `Ast` 节点 | 过程宏，但带语义 |

从"只标注"→"写模板"→"裸 AST"**平滑加码**，没有 Rust（macro_rules↔proc）或 SourceGen
（一步跌进 Roslyn）那种断崖。绝大多数人**只用到 T0**。

---

## 三层，每层一个完整例子

### T0 — 你只标注

```z42
[derive(Eq, Hash, ToString)]
public class Point { public int X; public int Y; }
```

编译器生成（你可以用 `z42c expand Point.z42` 看到真实展开）：

```z42
public bool Equals(Point other) { return this.X == other.X && this.Y == other.Y; }
public int  GetHashCode()        { return Hash.Combine(this.X, this.Y); }
public string ToString()         { return "Point { X=" + this.X + ", Y=" + this.Y + " }"; }
```

`derive(Eq)` 背后就是一个内置/库提供的 T1 模板宏（见下）。

### T1 — 你写一个模板宏（就是上面 `DeriveEq` 那段）

逐行白话：
- `[macro]`：标记它编译期执行；输入 `TypeInfo t` 是**被标注类型的语义信息**（能 `t.Fields` 枚举字段）。
- `quote { true }`：起一个"恒真"的 AST 作累加器。
- 循环里 `quote { ${body} && this.${f.Name} == other.${f.Name} }`：把已有 `body` splice 进来，
  再 && 上"这个字段相等"——逐字段叠成 `true && X相等 && Y相等`。
- 最后 `quote { ... }` 把它包成一个 `Equals` 方法 AST 返回，编译器插回 `Point`。

**没有字符串拼接、没有 token 解析、有完整字段类型**——这就是比两个参照系都好写的地方。

### T2 — 变换宏：重写一个函数（过程宏的自由，但读得懂）

给函数自动加计时（环绕织入，SourceGen 的"只能加"做不到这个）：

```z42
[macro]
public Ast Timed(MethodDecl m) {                    // 输入是整个方法的 AST
    return m.WithBody(quote {                        // 用新函数体替换原来的
        var __t = Clock.Now();                       // __t 卫生隔离，不会撞用户变量
        try { ${m.Body} } finally { Log(Clock.Since(__t)); }
    });
}

// 用法：
[Timed] public void Work() { /* 原逻辑 */ }
```

---

## 为什么能"支持全部语法机制"

这是**自举的红利**：宏 API 用的就是 z42c 的**真实 AST**（一个 z42 库），所以语言里有的**每种
构造**都可被表示、可被生成；`quote { ... }` 内部**接受任意合法 z42 语法**——要生成什么照着写就行，
只在需要"拆开看 / 模式匹配"时才下沉到裸节点。**"全语法覆盖"不是额外工作，是自举顺带给的。**

唯一要管的副作用：别让用户宏死锁在编译器**内部** AST 的形状上（跨版本脆弱）。对策：暴露一层
**版本化的 surface AST**，日常靠 `quote` 不碰裸节点。

---

## 安全与可见性（既要自由，又不丢"看得见"）

| 取法 | 说明 |
|---|---|
| **additive 默认** | 鼓励"在旁边生成"（像 partial），可读、可预期 |
| **transform 显式** | 要重写已有代码（如 `[Timed]`）是允许的，但语法上明确，不偷偷发生 |
| **展开可 dump** | `z42c expand` 输出宏展开后的真实源码 → 拿回 SourceGen 的"生成码可见"，补上过程宏"展开不透明"的痛 |

即：**有过程宏的自由，又不丢 SourceGen 的可见性。**

---

## Decisions

| # | 决定 | 理由 |
|---|------|------|
| 1 | 宏 = 编译期执行的**同语言 z42 代码**（VM 跑），非独立宏语言/crate | 复用 VM；无 `syn`/`quote` 那层认知负担 |
| 2 | 操作**类型化 AST**（自举 AST 库）+ 复用**反射**做语义 | 同时拿到"任意变换"和"类型信息"，两个参照系各缺其一 |
| 3 | `quote`/`splice` 准引用，不用字符串拼 / 不手搓节点 | 结构天然合法、可读、可卫生 |
| 4 | **分层** T0 derive → T1 模板 → T2 变换，无断崖 | 简单场景零代码；复杂场景不封顶 |
| 5 | additive 默认 + transform 显式 + 展开可 dump | 自由与可见性兼得 |
| 6 | 卫生（hygienic）默认 | 杜绝 C 宏式变量踩踏 |

---

## Deferred / 分期（诚实：这是语言里最难的几件事之一）

按 ROI 从低到高分期，先摘低果：

### meta-future-derive（先做）
- T0 内置/库 derive（Eq/Hash/ToString/Json/Builder）。**最高 ROI、最便宜**，且能直接复用反射线成果。
- **前置**：反射完整化（字段/属性/attribute 枚举）。

### meta-future-template-macro
- T1 模板宏 + `quote`/`splice` 准引用 + 卫生。
- **前置**：derive 跑通、`Ast` surface API 定型。

### meta-future-transform-macro
- T2 全 AST 变换（属性宏重写目标）+ 展开轮次（宏生成的类型被别的宏看见的 fixpoint）。
- **难点**：卫生完整版、展开/类型检查交错、跨宏依赖的展开顺序。

### meta-future-ide-expand
- IDE 展示宏展开、跳转生成码、增量重算。
- **前置**：`z42c expand` 稳定 + 确定性展开。

> 与既有路线一致：**简单场景声明式（derive），任意逻辑用代码（编译期 z42），不发明独立宏语言/
> token DSL**——同 task DAG、condition 的结论一条哲学（[build-orchestrator.md](../../../design/toolchain/build-orchestrator.md) Decision #5/#8）。
