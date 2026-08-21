# Design: Attribute 与编译期 Handler 体系

> 背景与动机见 [proposal.md](proposal.md)。本文件是技术设计 SoT：模型 / 接口 / pipeline / 决策 / 验证。
> PR 分解与逐 PR scope 见 [tasks.md](tasks.md)。
>
> **核心原则（贯穿全文）**：声明位只有一个表面 `[X]`，**不引入任何新关键字**；`[X]` 的 kind 完全由
> "X 解析到什么类型"决定。表达式位另有 `name!()` 宏用于值注入（另一根轴）。

## Architecture

### 设计不变量

1. **单一表面**：声明位一律 `[X]`；不加 `attribute`/`extern`/`layout`/`deprecated` 等关键字。kind 靠解析。
2. **枚举扩展点，不枚举 attribute**：编译器暴露一小撮稳定 phase hook；handler 是开放注册表。加新机制 =
   往注册表加一项，不碰文法。
3. **开放度对齐机制**：可扩展元数据（store-meta）与源生成（handler）给用户；codegen 原生封闭集用 directive。
4. **持久性可预测**：从 kind 即知是否活到运行时、以什么形式持久。
5. **确定性 = 自举字节不动点生死线**：任何 handler 输出 / 多标记处理，同输入→同输出，遍历按稳定键 sort
   （common-pitfalls §1），产物纳入增量 cache 指纹。
6. **自举红线**：外部 handler 绝不上 z42c 自建路径（bootstrap-seed 轴④）；内建 handler 在 z42c 内，天然规避。

### 两条 litmus + 三消费模型

**litmus A —「删掉所有运行时反射查询，发射出的代码会不会变？」**
**litmus B —「第三方能有意义地实现新行为吗，还是编译器必须原生懂？」**

`[X]` 应用时，编译器解析 X 的类型（本就要解析以派发），按下表三分：

| X 解析到 | kind | litmus A | 持久化形式 | 谁读 | 可扩展 |
|---------|------|:---:|-----------|------|:---:|
| 普通 attribute 类（`: Attribute`，无 handler 接口） | **store-meta** | 不变 | IrAttrRef blob | 运行时反射 | 用户 ✅ |
| directive 注册表里的已知类型（`Std.Meta` 封闭集） | **built-in directive** | 变 | 烘进 descriptor 字段 | codegen/loader/下游编译器 | 编译器原生 ❌ |
| 实现 `Generator`/`ModuleGenerator`/`Analyzer` 的类型 | **open handler** | 变 | 无（active，消费即弃） | 编译期 handler | 用户 ✅ |

这个三路判定**替换掉现有 `_isUserAttr` 名字白名单**：先查 directive 注册表（canonical 名，豁免后缀）→
再对用户 kind 按后缀展开解析 → 都不是即报错。**不靠名字白名单、不靠 marker，靠类型解析 + 后缀约定（D8）。**

**用名 → 类型解析（D8 后缀约定）**：`[X]` 先查 directive 注册表（`Native`/`Layout`… canonical、不带后缀）；
未命中则按后缀展开找用户类型 `X+{Attribute|Generator|Analyzer}`，匹配到的后缀即决定 kind。多个后缀同时匹配
（如 `DataAttribute` + `DataGenerator` 都 → `[Data]`）→ **确定性报错**。

三种**持久化模型**（消除 C# pseudo-attribute 隐藏坑）：
- **store-meta**：持久化**注解本身**（IrAttrRef），运行时反射重建实例。
- **built-in directive**：定义点消费注解、把**烘焙结果**（offset/size/binding 名/flag）写进 descriptor 字段；
  下游读 descriptor，不再读注解。参数在定义点常量折叠，无需跑工厂、无"静态读工厂参数"难题。
- **open handler**：编译期消费即弃，产物是生成的源码/诊断，注解本身**不持久**。

### 两条可扩展基底

**基底一 · 注解 `[Name(args)]`**：单一表面。解析 Name → 三路判定（上表）。命名空间约定：directive/handler
可命名空间化，`[tool.skip]` 编译器忽略、留外部工具。

**基底二 · 表达式位编译期宏 `name!()`**（值注入）：`caller_member!()`/`caller_line!()`/`caller_file!()`/
`module_path!()` 是注册表里的内建宏，可用于参数默认值。v1 C# 式 call-site 注入、无 ABI 改动；Rust
`[track_caller]` 多层传播留作宏注册表将来追加。

## Handler 打包与加载：独立编译期 zpkg 类别（决策 D9，C# analyzer 对齐）

**核心**：`Generator`/`ModuleGenerator`/`Analyzer` 这类 **open handler 不随普通依赖走**——它们打包为
**独立的"编译期 handler zpkg"类别**，在 z42.toml 里用**专门的引用段**（本设计定名 `[analyzers]`，覆盖
analyzer + generator 两类，与 C# 把 source generator 也归 analyzer 引用类别同源）声明，语义 =
「**加载进编译器、编译期运行、绝不链入目标程序**」。

**对齐 C#/Roslyn**：analyzer / source generator 程序集经 `<Analyzer>` item（或 analyzer NuGet 包的
`analyzers/` 目录）被 **Roslyn 加载进编译器进程**跑，**不进**目标项目的运行时/元数据引用——用户程序运行期
不依赖 analyzer。z42 照搬这条边界。

| | 普通依赖 `[dependencies]` | handler zpkg `[analyzers]` |
|---|---|---|
| 引用段 | `[dependencies]` | **独立段** `[analyzers]` |
| 加载到 | 目标程序运行时 + 编译期类型解析 | **仅编译期 VM** |
| 链入产物 | 是 | **否**（编译期消费即弃） |
| 发现 | —— | 在该段声明的 zpkg 内**限定反射**找 `: Analyzer`/`: Generator` 实现 |

**为什么这样定（三重收益）**：
1. **红线结构性满足**（设计不变量 6，替代原运行期特判 gating）：外部 handler 只在 `[analyzers]` 段声明时才加载；
   **z42c 自建自己时不声明任何 handler zpkg** → 外部 handler 天然不上自举路径，无需运行期 gating 特判。
2. **发现精确、确定**：发现面 = 该段列出的 zpkg（限定反射），**不是**「扫所有已加载 zpkg」（后者非确定 + 会误伤
   运行时依赖里碰巧实现接口的类）。遍历按 zpkg 稳定键 sort（common-pitfalls §1）。
3. **职责清晰**：普通依赖 = 目标程序的一部分；handler zpkg = 构建工具。二者物理分开，不混。

**内建 handler 不走此类别**：`BenchmarkDesugar`（内建 generator）/ 测试索引 / directive（`[Native]`）是 z42c
**内部**实现，硬编码在编译器里（现状），与外部 handler zpkg 加载是**两条独立路径**——内建的永远在，外部的 opt-in。

**不改文法、不改 zpkg 二进制格式**：handler zpkg 就是**普通 zpkg**（含实现接口的类），"特殊"只在**引用方式**
（哪个 toml 段声明 + 加载到哪 + 是否链入），非格式。故 handler 加载本身**无 bump**。

### 统一模型：handler 是独立 build 目标 + 类型化引用 + 不入消费产物（User 细化 2026-08-20）

三点，与 test/bench「独立 zpkg」（[[strip-test-bench-to-separate-zpkg]] Model B）**同一套机制**：

1. **handler 独立输出 zpkg**：analyzer/generator 像 test 一样，是**独立 build 目标**——在其自身 manifest 声明
   `kind = "analyzer"` / `kind = "generator"`（类比 `kind = "exe"`/`"lib"`），独立编译、独立产出 zpkg。
2. **消费方类型化引用**：消费方在依赖配置里**设置该引用的类型/类别**（标记「这是 handler 依赖」，本设计用
   独立段 `[analyzers]` 承载，即「按段设类型」）；编译器据此**加载 + 执行对应 handler**，而非链入目标程序。
3. **handler 不入消费产物**：handler 的**类型本身**（`DeriveGenerator`/`NoGotoAnalyzer` 类）+ **applied-handler
   注解**（kind=handler 的 `[Derive]` 等）**绝不写进消费方 zpkg**——编译期消费即弃（litmus A）。**唯一例外 =
   store-meta attribute**（`[Route]` → `RouteAttribute : Attribute`）：它是 store-meta，照常持久成 IrAttrRef
   blob 供运行时反射；即便某 module-generator 编译期也读它，也不改变它 store-meta 的持久性。区分靠 kind，不靠
   谁读它。

> 收益：analyzer/generator/test 三类「构建工具 zpkg」共享一条「独立目标 + 类型化引用 + 不入消费闭包」的路子，
> 概念统一、实现复用。红线（不变量 6）由此更牢：消费方产物里根本不含 handler 代码/注解，自举链更不可能被污染。

## Attribute 类型与可贴位置

**不引入 `attribute` 关键字**（决策 D1）。attribute 类型沿用 `class Foo : Attribute {}`；`Std.Attribute` 基类保留。

**类名后缀强制约定（D8，反转 z42 旧"无后缀"决定）**：用户 kind 的类名**必须**带角色后缀，用名**剥离**后缀：
| kind | 类名（定义） | 用名（`[X]`） | 实现 |
|------|-------------|--------------|------|
| store-meta attribute | `RouteAttribute` | `[Route]` | `: Attribute` |
| applied generator | `DeriveGenerator` | `[Derive]` | `: Generator` |
| module generator | `RouteTableGenerator` | （不贴，无用名） | `: ModuleGenerator` |
| analyzer | `NoGotoAnalyzer` | （不贴，观察） | `: Analyzer` |

实现了 `Generator`/`Analyzer`/`: Attribute` 却无对应后缀 → **编译错误**（约定强制、不靠自觉）。directive
（`[Native]`/`[Layout]`）是编译器封闭集，**豁免后缀**，保留 canonical 短名。代码里引用类型仍用全名
（`MethodsWith<RouteAttribute>()`）；剥离只发生在 `[X]` 用名处。

> **与 C# 的关系（勿误读为"抄 C#"）**：D8 是 **z42 自主选择**，只有 store-meta 那半与 C# 语言规则同源，
> 且 z42 比 C# 更严：
> - **`*Attribute` 剥后缀**：C# 有真语言规则（`[Obsolete]` ↔ `ObsoleteAttribute`），但**不强制加后缀**
>   ——无后缀 attribute 类完全合法，"应带后缀"只是**风格分析器警告 CA1710**（可关），非编译错；唯一硬错是
>   `Foo`/`FooAttribute` **歧义 → CS1614**。z42 把这半从"软警告"提成"**硬编译错**"。
> - **`*Generator`/`*Analyzer` 后缀**：**C# 完全没有此机制**。Roslyn 靠 **marker attribute**
>   （`[Generator]`/`[DiagnosticAnalyzer]`）**+ 接口/基类**判 kind，类名后缀**纯命名约定、零编译器语义**。
>   z42 故意**砍掉 marker**（见下「Handler 契约」），改用**后缀当 kind 信号**——这是 z42 自创，不是 C# 先例。

### 位置限制 `[Targets]`（对应 C# `[AttributeUsage]`）

```z42
public enum Target { Class=1, Struct=2, Interface=4, Enum=8, Method=16, Field=32, Param=64 }

[Targets(Target.Method | Target.Field)]      // 只能贴方法/字段，违规 → 编译错误
public class MemoizeGenerator : Generator { ... }   // 用 [Memoize]
```
- **读取无 bump**：handler/attribute 的 zpkg 是编译期依赖、被加载+反射，编译器直接从内存里的类读 `[Targets]`，
  不需要专门元数据字段（利用"handler 跑同一 VM"）。
- **当前位置集**：class/struct/interface/enum/method/field/param。**局部变量当前 parser 不支持贴 attribute**
  （`StmtParser` 无解析）——要支持需先扩 parser，属独立语言变更，不在本 change。
- **richer 约束靠 handler 自校验**（Rust proc-macro 式）：位置之外的约束（如"只能贴全 public 字段的 struct"）
  由 `Generate`/`Register` 里 `ctx.Diags().Report(...)` 自己判。位置声明式、语义自校验，分工清楚。

## Handler 契约（无 Triggers、无 marker）

- **无 `[Generator]`/`[Analyzer]` marker**：实现接口即声明 kind，靠反射"找实现接口的类型"发现。
- **无 `Triggers()` 方法**：
  - **applied generator**：**类名剥后缀即注解名**——`[Derive]` 解析到 `DeriveGenerator`，其应用处即触发。
  - **module generator**：`ModuleGenerator`，不贴在任何处，注册（依赖）即跑一次、扫全编译。
  - **analyzer**：trigger 藏在 `ctx.OnSyntaxNode(kind, …)` 等注册调用里，其并集即 trigger。
- **执行顺序（gap1）**：同 phase 内多 handler 按 handler 稳定 Id 排序后执行；产物/诊断顺序确定。

## 诊断 / severity / 开关（Analyzer + Generator 共享）

```z42
public struct DiagRule {
    string Id; string Title; string MessageFormat;
    string Category; Severity DefaultSeverity; bool EnabledByDefault; string? HelpUri; DiagTag Tags;
}
enum Severity { Hidden, Info, Warning, Error }   // 对齐 Roslyn
public interface DiagSink { DiagBuilder Report(DiagRule rule, Span at, ..args); /* 可带 related / WithFix */ }
```

**开启 + 全局配置**（`packages.toml [lints]`，对应 C# `.editorconfig`）：
```toml
[lints]
Z9002 = "warning"          # 显式设级
Z9100 = "error"            # 升级为硬错误
Z9003 = "none"             # 关掉
"webgen.*" = "none"        # 关整包
warnings-as-errors = true
```
依赖 analyzer 包 = 自动注册；单规则由 `EnabledByDefault` + `[lints]` 覆盖。

**局部关闭**（对应 C# `#pragma warning disable/restore` + `[SuppressMessage]`）：
```z42
#suppress Z9002 "这段是生成代码"
try { risky(); } catch (Error e) { }
#restore Z9002

[Suppress("Z9100", "热路径已 profile")]  public void Hot() { ... }
```
`Error`=编译失败；`Hidden`=不显示但供 `--fix`；`Info`=建议。

> **PR3b 落地（2026-08-21）**：`[lints]` 解析 = z42.project 中性 `LintNames`/`LintSeverities`/
> `LintWarningsAsErrors`（`ManifestLoader._parseLints`，`warnings-as-errors` 从逐规则串拎出）；severity
> **决策**在编译器侧 `LintConfig.Resolve`（z42c.semantics）——精确 Id 优先于 `pkg.*` 前缀通配、`"none"`
> 抑制、`EnabledByDefault` 门、WAE 升级。`DiagSinkImpl` 抑制则丢弃、不进 `DiagnosticBag`；仅 Error 级
> 增 `ErrorCount`。局部关闭（`#suppress`/`[Suppress]`）仍留 PR3c。z42c 自建无 `[lints]` → byte-identical。

## Analyzer 接口（visitor 模型，PR3a 实现——偏离 Roslyn 回调注册）

**⚠️ 偏离决策（PR3a 实测，2026-08-21）**：设计初稿照 Roslyn `Register(AnalysisContext ctx)` +
`ctx.OnSyntaxNode(kind, lambda)` **回调注册**模型。实现时发现 z42 自举编译器**无法承载它**：
① z42 自举编译器**刻意规避 delegate**（`BinaryTypeTable` 用 int tag 替 `Func<>`）；② **命名 delegate
跨 zpkg 丢 FQ 名**（`Bound.z42` / `ExprEmitter.z42`：delegate 塌成结构化 `Z42FuncType`）→ 契约里的
命名回调 delegate（`SyntaxNodeAction`）**在消费方无法按名解析**（实测 E0443）。故改用**无 delegate 的
visitor 模型**：analyzer 声明 `ObservedKinds()` + 实现 `OnSyntaxNode(kind, node, sink)`，driver 遍历 AST、
对命中节点**纯虚接口调用**该方法。全走接口派发（跨包已验证）+ 纯虚调用，driver 侧零 delegate/零闭包，
analyzer 侧也无需 lambda。**契约在 `z42c.syntax/src/Analysis.z42`，driver 在 `z42c.semantics/src/AnalyzerDriver.z42`。**

```z42
public interface Analyzer {
    DiagRule[] SupportedRules();                          // 声明能发哪些规则（config/工具枚举）
    int[] ObservedKinds();                                // 声明观察哪些 SyntaxKind（省无关派发）
    void OnSyntaxNode(int kind, object node, DiagSink diags);   // driver 对命中节点调用；内部 as-cast + 报告
}
```
观察面（照 Roslyn `Register*Action`，visitor 化——每类观察面 = 一对 `Observed*Kinds()` + `On*()`）：

| 观察方法 | 观察对象 | 语义? | 用途 | 状态 |
|---------|---------|:---:|------|:---:|
| `OnSyntaxNode(kind,node,sink)` | AST 节点 | 否 | 禁 goto、空 catch、括号风格 | **PR3a ✅** |
| `OnSymbol(...)` | 符号 | 是 | 命名规范、public 面 | 后续增量 |
| `OnOperation(...)` | 语义操作（调用/赋值/转换/new） | 是 | 不依赖语法形状的健壮检查 | 后续 |
| `OnBody(...)` | 方法体 | 是 | 数据流、未初始化 | 后续 |
| `OnReference(...)` | use-site | 是 | `[Deprecated]` 引用告警 | 后续 |
| `OnCompilationStart/End(...)` | 跨编译累积 | 是 | "声明了但全程序无人引用" | 后续 |
| `OnIrOp(...)` | IR 操作（additive） | 是 | **超出 C#**：循环内分配等 perf lint | 后续 |

示例（PR3a 实测通过——`z42c.semantics/tests/analyzer/analyzer_tests.z42`）：
```z42
class NoEmptyCatchAnalyzer : Analyzer {
    public DiagRule[] SupportedRules() { ... return [Z9002 规则]; }
    public int[] ObservedKinds() { ... return [SyntaxKind.CatchClause]; }
    public void OnSyntaxNode(int kind, object node, DiagSink diags) {
        if (kind == SyntaxKind.CatchClause) {
            CatchClause c = node as CatchClause;
            if (c.Body is BlockStmt) {                     // 空 catch → 报告
                if ((c.Body as BlockStmt).Count == 0) { diags.Report(_rule(), c.Span); }
            }
        }
    }
}
```

## Generator 接口（两种激活模式）

```z42
// applied：类名 = 注解名；[X] 贴处触发；ctor 收注解参数，Generate 读 this
public interface Generator {
    void Generate(GenTarget t, GenSink sink);       // t = 被贴的声明
}
// module：不贴任何处；注册即跑一次、扫全编译
public interface ModuleGenerator {
    void Generate(GenContext ctx, GenSink sink);
}
public interface GenSink {
    void AddSource(string hint, string src);        // 追加（≈ C#）
    void Replace(DeclId id, string src);            // 替换被贴声明（C# 做不到，免 partial）
    void Augment(DeclId id, string membersSrc);     // 往已有类型/体注入成员
}
public interface GenContext {                        // module 用
    Seq<TypeSymbol> TypesWith<T>();  Seq<MethodSymbol> MethodsWith<T>();   // 强类型查询
    TypeSymbol? Resolve(string fqn); DiagSink Diags();
}
public interface GenTarget {                          // applied 用
    DeclId DeclId; SymbolKind Kind; SourceView Original; /* 字段/成员访问 */
    DiagSink Diags();
}
```

**"注解即 generator"**：`[Derive(Trait.Equals)]` 解析到 `class Derive : Generator`，编译器用注解参数**实例化** it
（ctor 参数 = 注解参数），对被贴声明跑 `Generate`，`Generate` 读 `this.字段`——无需 `GetAttribute`。

**splice 模型**：被贴声明有稳定 **DeclId**（`module.type.member#idx`）；generator 只吐**源码文本**、按 DeclId
引用，不碰 AST/IR；合并器施加 replace/augment → 重新 parse+typecheck「用户码 ∪ 生成码」。
**护栏**：generator 只能改它 trigger 命中（opt-in）的声明；两 handler 碰同一 DeclId → 确定性报错（非 last-wins）。
**增量**：编译器从 applied 站点 / observed kinds / queried 集自动推增量 key + 产物纳入 cache 指纹——免手写 provider。

示例（applied，枚举参数）：
```z42
public enum Trait { Equals=1, Hash=2, ToString=4 }   // flags 枚举，编译期检查+补全（非字符串）
public class DeriveGenerator : Generator {            // 后缀 Generator；[Derive] 剥后缀即触发
    Trait traits;
    public DeriveGenerator(Trait traits) { this.traits = traits; }   // 注解参数 = 构造参数
    public void Generate(GenTarget t, GenSink sink) {
        var buf = new StrBuilder();
        if (traits.Has(Trait.Equals))   buf.Append(_renderEquals(t));
        if (traits.Has(Trait.ToString)) buf.Append(_renderToString(t));
        sink.Augment(t.DeclId, buf.ToString());
    }
}
// 用户：[Derive(Trait.Equals | Trait.ToString)] public struct Point { public int X; public int Y; }
```
> 扩展注：枚举 = 封闭集。将来若要**用户可扩展 derive**，参数改 Rust 式**类型引用**（每个 derivable 是自己的
> generator，`[Derive(Equals, Hash)]`），是独立演进。

示例（module，聚合成表）：
```z42
public class RouteAttribute : Attribute { public string Path; public RouteAttribute(string p){ Path=p; } } // passive，用 [Route]
public class RouteTableGenerator : ModuleGenerator {
    public void Generate(GenContext ctx, GenSink sink) {
        var rows = new StrBuilder();
        foreach (var m in ctx.MethodsWith<RouteAttribute>())               // 代码用全名，强类型查询
            rows.Append($"  t.Add(\"{m.GetAttribute<RouteAttribute>().Path}\", {m.FullName});\n");  // 像 ctor 拿 .Path
        sink.AddSource("__RouteTable.g.z42", $"...{rows}...");
    }
}
```

## Built-in directive

**全部是 `[X]` 注解，不是关键字**（决策 D7）。directive 是编译器原生、封闭的一小撮，放 `Std.Meta`，编译器内部
有张 directive 注册表（= Rust `builtin_attrs.rs`）：

| directive | 对应 | 作用 | 持久化（烘进 descriptor） |
|-----------|------|------|------------------------|
| `[Native("__x")]` | 现状保留（**暂不改名**） | native/builtin 绑定 → `StubEmitter` | binding 名（现有 stub 机制） |
| `[Layout(Sequential\|Explicit\|Packed)]` `[FieldOffset(8)]` | StructLayout | 内存布局/ABI | offset/size 字段（bump，做时） |
| `[Repr(C)]` | Rust repr | interop 表示 | descriptor 位（bump，做时） |
| `[Inline]` / `[NoInline]` | inline hint | 喂现有跨包内联 | inline 位 |
| `[Deprecated("msg")]` | 废弃 | use-site 告警 | flag+msg 串池（bump，D2） |

- 消费：IrGen 静态读参数（常量，编译器原生解析）→ 烘进 descriptor。下游读 descriptor 不再读注解。
- 默认**不进反射**；interop 工具需要 size/offset 可选镜像进反射（additive）。
- **不可用户扩展**（故意，litmus B）：codegen 必须原生懂每个。用户想要"编译器生成点什么" → 用 **Generator**
  （源码层），不是 directive（codegen 内部）。

> `[Native]` → `[Extern]` 的改名**本 change 暂不做**（User 裁决 2026-08-19），延后为独立小 change。

## 超出 C# 的能力（契约 vs 追加，D4）

| C# 局限 | z42 目标 | 契约 now / additive |
|---------|---------|:---:|
| 生成器只能追加，逼 `partial` | `Replace`/`Augment` | 契约 now |
| 分析器/修复拆两类型，修复仅 IDE | 同套 splice，`--fix` build 期应用 | additive |
| 分析停在 IOperation | 下探 IR 层（`OnIrOp`）性能 lint | additive |
| 分析器需独立 assembly | 同 VM/zpkg/反射加载 | 契约 now |
| 增量需手写 provider 管线 | 编译器自动从 trigger 推导 | 契约 now |

## Pipeline 挂载 + 有界多轮 + 护栏

```
z42c.syntax    parse ──▶ [Analyzer.OnSyntaxNode/Tree]
z42c.semantics bind  ──▶ [Analyzer.OnSymbol/Operation/Body/Reference] + [OnCompilationStart→累积]
                     └─▶ [Generator.Generate / ModuleGenerator.Generate]  (GenSink：Add/Replace/Augment)
z42c.pipeline  merge 生成源 ──▶ 重新 parse+typecheck「用户码∪生成码」──▶ [Analyzer.OnCompilationEnd]
z42c.IrGen     lower ──▶ [directive: Native/Layout/… → descriptor 字段] + [Analyzer.OnIrOp] ──▶ zpkg
```
**有界多轮**：默认单轮。generator 声明 `consumes(tag)/produces(tag)` → 拓扑序、后轮含前轮生成符号；硬上限
轮数、成环报错；轮内遍历按稳定键 sort。

**护栏（红线，v1 强制）**：① replace/augment 冲突→确定性报错；② 多轮有界+拓扑序+产物纳入指纹；
③ 外部 handler 禁上 z42c 自举路径；④ 信任（D5）：直接信任+记账 defer 沙箱，但补确定性约束——handler 须为其
声明输入的纯函数，禁读环境态（fs/时钟/随机），先文档化、将来用 VM 能力限制强制。

## Implementation Notes：持久化 / bump / 4 pass 迁移

| 机制 | 进 zpkg | 载体 | bump? |
|------|:---:|------|:---:|
| store-meta | 是 | IrAttrRef blob（已有） | 否 |
| directive: Native | 是 | 现有 stub | 否 |
| directive: Layout/Repr（E2） | 是 | descriptor 布局字段 | 是（做时） |
| directive: Deprecated（D2） | 是 | flag+msg 串池 | 是 |
| caller-kind 宏（D3） | 是 | param 枚举 flag | 是 |
| Generator 产物 / Analyzer | 否 | —— | 否 |

**现有 4 pass 收敛**（勘察确认分两层）：
- AST 级（`CompilationUnit→CompilationUnit`）：`AttributeSynth`（→ store-meta 默认路径，判定改三路）+
  `BenchmarkDesugar`（→ 内建 Generator）。挂载点 `IncrementalDriver.z42:52` / `IrDump.z42:82`。
- IR 级（IrGen 子构建器）：`TestIndexBuilder`（→ store-meta，反射发现测试，TIDX 退休另评）+ `StubEmitter`
  （→ directive `[Native]`，逻辑不变，识别改注册表）。挂载在 `IrGen.Generate`。
- **字节不动点陷阱**：`_isUserAttr` 黑名单缺 `Setup/Teardown/Ignore`（它们现会被合成工厂）。PR1 纯重构，
  三路判定的 store-meta 分支必须**逐字节复刻此现状**，不得顺手对齐——修不一致是独立非重构变更。

## Decisions（决策记录，已定）

| # | 决策 | 定案 |
|---|------|------|
| D1 | attribute 定义形态 | **不引入 `attribute` 关键字**；沿用 `class Foo:Attribute`；位置用 `[Targets]`（反射读，无 bump），richer 约束靠 handler 自校验（Rust 对齐） |
| D2 | deprecated | `[Deprecated("msg")]` built-in directive，持久化 flag+msg（跨包+IDE，带 bump） |
| D3 | caller 形态 | C# 式表达式宏默认值（最简、无 ABI）；Rust `[track_caller]` 留作宏注册表将来追加 |
| D4 | v1 契约范围 | 契约 now：DeclId+merge+Add/Replace/Augment+有界多轮；additive：`--fix`、`OnIrOp` |
| D5 | 外部 handler 信任 | 直接信任 + 记账 defer 沙箱；补确定性约束（handler 须纯函数，禁读环境态） |
| D6 | 命名 | 无 marker（接口发现）；lint config = `packages.toml [lints]` 段；`Target` 枚举 |
| D7 | directive 表面 | 全部 `[X]` 注解、**不引入关键字**；directive 注册表（Rust builtin_attrs 式）；`[Native]` **暂不改名** |
| D8 | 类名后缀约定 | **强制**用户 kind 类名带角色后缀（`*Attribute`/`*Generator`/`*Analyzer`），用名剥离；缺后缀→编译错误；directive 豁免；剥离后跨 kind 撞名→报错。**反转 z42 旧"无后缀"决定**——实现 PR 须同步改 `Attribute.z42`/`basic.z42` 头注 |
| D9 | handler 打包/加载 | open handler（Generator/Analyzer）= **独立 build 目标 + 独立编译期 zpkg 类别**：handler 自身 manifest 声 `kind="analyzer"/"generator"`（像 test 一样独立编译输出 zpkg）；消费方**类型化引用**（z42.toml 独立段 `[analyzers]` 承载「这是 handler 依赖」），编译器据此加载进编译期 VM 运行、**不链入目标程序**（C# `<Analyzer>` 对齐）。发现 = 该段 zpkg 内**限定反射/元数据**（非扫全部已加载 zpkg）。**handler 类型 + applied-handler 注解绝不写进消费方 zpkg**（唯 store-meta attribute 例外，照常持久 blob；区分靠 kind）。**红线（不变量 6）由类别分离 + 不入产物双重结构性满足**——z42c 自建不声明 handler zpkg。内建 handler（BenchmarkDesugar/test/directive）走编译器内部路径，与外部加载独立。handler zpkg 是普通 zpkg，"特殊"只在引用方式 + 是否链入 → **无 bump**。与 test/bench「独立 zpkg」（Model B）同一套机制 |
| gap1 | handler 执行序 | 同 phase 内按 handler 稳定 Id 排序 |
| gap2 | analyzer 启用 | 依赖即注册；规则级由 `EnabledByDefault` + `[lints]` 控制（含整包开关） |
| Q1/2/3 | kind 判定 | 三路：directive 注册表 → 实现 handler 接口 → else store-meta；替换 `_isUserAttr` |

## Testing Strategy

- **PR1（纯内部重构）**：无语义输出变化 → gen1==gen2 逐字节；`xtask test` 全 stage gate；self-host 5/5；
  确认 zbc-format golden 零漂移；**Setup/Teardown/Ignore 行为逐字节保持**。
- **带 bump 的 PR（D2/E2/D3）**：两阶段自举 + `xtask test bootstrap`；fixture 重生按 escape-stack 经验。
- **Generator/Analyzer**：生成物 golden（源码+产物双验）、诊断 golden（Id/severity/span）、产物/诊断确定性
  （同源多跑逐字节一致）。
- **红线自检**：外部 handler 不在 z42c 自建路径；generator 产物进增量 cache 指纹。

## Deferred

用户可写 `macro`/自定义 derive（→ 参数改类型引用）；handler 沙箱；`[Layout]`/`[Repr]`(E2)；`OnIrOp` perf lint；
Rust `[track_caller]` 多层传播；`[Native]`→`[Extern]` 改名；局部变量 attribute（需扩 parser）。
