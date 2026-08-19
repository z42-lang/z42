# Design: Attribute 与编译期 Handler 体系

> 背景与动机见 [proposal.md](proposal.md)。本文件是技术设计 SoT：模型 / 接口 / pipeline / 决策 / 验证。
> PR 分解与逐 PR scope 见 [tasks.md](tasks.md)。

## Architecture

### 设计不变量

1. **单一语义**：`[X]` ⟺ 运行时可反射元数据、编译期无副作用、必然活到运行时。
2. **枚举扩展点，不枚举 attribute**：编译器暴露一小撮稳定 phase hook；handler 是开放注册表。加新机制 =
   往注册表加一项，不碰文法。
3. **开放度对齐机制**：可扩展元数据（`[X]`）给用户；编译器原生封闭集用 directive；可注册编译期行为用 handler。
4. **持久性可预测**：从机制即知是否活到运行时。
5. **确定性 = 自举字节不动点生死线**：任何 handler 输出 / 多标记处理，同输入→同输出，遍历按稳定键 sort
   （common-pitfalls §1），产物纳入增量 cache 指纹。
6. **自举红线**：外部 handler 绝不上 z42c 自建路径（bootstrap-seed 轴④）；内建 handler 在 z42c 内，天然规避。

### 三消费模型 + 两条 litmus

**litmus A —「删掉所有运行时反射查询，发射出的代码会不会变？」**
**litmus B —「第三方能有意义地实现新行为吗，还是编译器必须原生懂？」**

| 类 | litmus A | 消费者 | 可扩展 | 例 |
|----|:---:|--------|:---:|----|
| **store-meta** | 不变 | 运行时反射 | 用户自由定义 ✅ | `[Serializable]` `[Route]` `[Test]` |
| **built-in directive** | 变 | 固定编译器 phase | 编译器原生 ❌ | `layout` `extern` `repr` `inline` |
| **open handler** | 变 | 可注册 handler | 用户/团队注册 ✅ | `derive`(Generator)、lint(Analyzer) |

- layout 删反射 offset 照旧 → 编译期输入，非 store-meta；codegen 必须原生懂每种 layout → 非 open handler
  → **built-in directive**（IrGen 读 descriptor 位，不走 handler 回调）。extern 同理。
- **可扩展注册表里只剩 Generator + Analyzer 家族。**

### 两条可扩展基底

**基底一 · 注解 `[Name(args)]`**：单一表面。解析 `[Name(args)]` → 解析 Name → 拿 handler → 派发到它声明的
phase。裸 `attribute` 类型且无特殊 handler → 默认 store-meta。认不出且无 handler → 报错。命名空间约定：
编译期 handler 命名空间化（`[perf.HotPath]`），`[tool.skip]` 编译器忽略、留外部工具。

**基底二 · 表达式位编译期宏 `name!()`**（D3）：`caller_member!()`/`caller_line!()`/`caller_file!()`/
`module_path!()` 是注册表里的内建宏，可用于参数默认值：
```z42
fn Log(string msg, string who = caller_member!(), int line = caller_line!()) { ... }
Log("hi")   // 编译期把省略的实参展开成调用点字面量
```
新增同类 = 再注册一个宏，不碰 lexer/parser。v1 采 C# 式表达式宏默认值（纯编译期 call-site 注入、无 ABI
改动，只 param 加 caller-kind flag）。

## `attribute` 声明文法（D1）

`attribute` 是**声明种类关键字**（与 `class`/`struct`/`interface`/`enum` 同族、互斥），**不是修饰符**；
`public`/`sealed` 位置语义不变。

```
public   attribute   Route(string path, string method: "GET")   targets(class, method) single inherited ;
└─修饰符─┘ └─声明种类─┘ └────── 名字 + 主构造参数 ──────┘        └─────────── 声明子句 ───────────┘
```

两种等价写法：
```z42
// A) 主构造式（简单 attribute 首选）：主构造参数隐式成同名 public 字段（record 式）
public attribute Route(string path, string method: "GET") targets(class, method) single;

// B) body 式（需额外字段/多 ctor/逻辑）
public attribute Route targets(class, method) single {
    public string Path;  public string Method;
    public Route(string path, string method: "GET") { Path = path; Method = method; }
}
```

**子句语义**：

| 子句 | 取值 | 默认 | 语义 |
|------|------|------|------|
| `targets(...)` | `class` `struct` `interface` `enum` `method` `field` `param` 子集 | 全部 | 只允许贴列出目标；违规→编译错误 |
| `single` / `multi` | —— | `single` | 同一目标可否重复贴 |
| `inherited` | 出现即启用 | 不继承 | 反射时子类/override 是否可见该 attr |

**apply-site 校验（新增语义 pass）**：每处 `[Foo]` 查 Foo 的 usage（目标命中 / `single` / `inherited`）。
**跨包**：usage 随 attribute 类型元数据进 zpkg（`IrClassDesc.attr_usage`：target-mask+flags）→ zbc/zpkg minor
bump，走两阶段自举。

**迁移**：`class Foo : Attribute {}` → `attribute Foo {}`，面很小。`Std.Attribute` 基类保留（`attribute` 声明
隐式派生它，store-meta 工厂返回类型统一），用户不再手写 `: Attribute`。**运行时路径（工厂→IrAttrRef→反射）
原样不动。**

## Handler 契约

```z42
public interface Handler { TriggerSpec Triggers(); }   // 只对匹配项触发（增量 key + 性能）
```
`TriggerSpec`：`Trigger.Annotation/NodeKind/SymbolKind/OpKind/Implements/NameMatches/Module` + `and/or/where`。
三种 kind 靠**实现哪个接口**区分，据此派发；不靠名字、不靠外部标记表。

**执行顺序（gap1）**：同 phase 内多 handler **按 handler 稳定 Id 排序**后执行；产物/诊断顺序确定。

## 诊断 / severity 模型（Analyzer + Generator 共享）

```z42
public struct DiagRule {
    string Id; string Title; string MessageFormat;
    string Category; Severity DefaultSeverity; bool EnabledByDefault; string? HelpUri; DiagTag Tags;
}
enum Severity { Hidden, Info, Warning, Error }   // 对齐 Roslyn
public interface DiagSink {
    DiagBuilder Report(DiagRule rule, Span at, ..args);
    DiagBuilder Report(DiagRule rule, Span at, Seq<Span> related, ..args);
}
```
**severity 三层生效**：① 规则默认级 `DefaultSeverity`；② 用户 config `packages.toml [lints]`
（`Z9001 = error|warning|info|none` + 全局 `warnings-as-errors`）；③ 就地 `#suppress Z9001 "理由"` /
`[Suppress("Z9001","理由")]`。`Error`=编译失败；`Hidden`=不显示供 `--fix`；`Info`=建议。

## Analyzer 接口

```z42
public interface Analyzer : Handler {
    Seq<DiagRule> SupportedRules();
    void Register(AnalysisContext ctx);
}
```
`AnalysisContext` 观察面（照 Roslyn `Register*Action`）：

| 注册方法 | 观察对象 | 语义? | 用途 |
|---------|---------|:---:|------|
| `OnSyntaxNode(trig,cb)` | AST 节点 | 否 | 禁 goto、括号风格 |
| `OnSyntaxTree(cb)` | 整棵树 | 否 | 文件级格式 |
| `OnSymbol(kinds,cb)` | 符号 | 是 | 命名规范、public 面 |
| `OnOperation(kinds,cb)` | 语义操作 | 是 | 不依赖语法形状的健壮检查 |
| `OnBody(cb)` | 方法体 | 是 | 数据流 |
| `OnReference(cb)` | use-site | 是 | `deprecated` 引用告警 |
| `OnCompilationStart(→State)`/`OnCompilationEnd(State,sink)` | 跨编译累积 | 是 | "全程序无人引用" |

**启用模型（gap2）**：`packages.toml` 声明依赖 → **依赖即注册**；规则级由 `EnabledByDefault` + `[lints]`
控制（含整包开关 `pkg.* = none`）。

示例：
```z42
[Analyzer]
public class NoGoto : Analyzer {
    static readonly DiagRule Rule = new DiagRule {
        Id="Z9001", Title="禁用 goto", MessageFormat="goto 被禁用",
        Category="Design", DefaultSeverity=Severity.Error, EnabledByDefault=true };
    public Seq<DiagRule> SupportedRules() => [Rule];
    public TriggerSpec  Triggers()       => Trigger.NodeKind(SyntaxKind.Goto);
    public void Register(AnalysisContext ctx) =>
        ctx.OnSyntaxNode(Triggers(), (n, d) => d.Report(Rule, n.Span));
}
```

## Generator 接口

```z42
public interface Generator : Handler {
    void PostInit(GenSink sink);                 // 用户码分析前注入固定源（marker 注解）
    void Generate(GenContext ctx, GenSink sink);
}
public interface GenSink {
    void AddSource(string hint, string src);     // 追加（≈ C#）
    void Replace(DeclId id, string src);         // 替换被注解声明（C# 做不到）
    void Augment(DeclId id, string membersSrc);  // 注入成员，免 partial（C# 做不到）
}
public interface GenContext {
    Seq<TypeSymbol> TypesWith(string attr); Seq<MethodSymbol> MethodsWith(string attr);
    SourceView OriginalOf(DeclId id); TypeSymbol? Resolve(string fqn); DiagSink Diags();
}
```
**splice 模型**：被注解声明有稳定 **DeclId**（`module.type.member#idx`）；generator 只吐源码文本、按 DeclId
引用，不碰 AST/IR；合并器施加 replace/augment → 重新 parse+typecheck「用户码 ∪ 生成码」。
**护栏**：generator 只能改它 trigger 命中（opt-in）的声明；两 handler 碰同一 DeclId → 确定性报错（非 last-wins）。
**增量**：作者精确声明 `Triggers()`，编译器用它当增量 key + 产物纳入 cache 指纹——免手写 provider 管线。

示例：
```z42
[Generator]
public class EqualsGen : Generator {
    public void PostInit(GenSink s) => s.AddSource("Derivable.g.z42", "public attribute Derivable targets(struct,class);");
    public TriggerSpec Triggers()   => Trigger.Annotation("Derivable");
    public void Generate(GenContext ctx, GenSink sink) {
        foreach (var t in ctx.TypesWith("Derivable")) {
            if (t.Fields.IsEmpty()) { ctx.Diags().Report(NoFieldsRule, t.NameSpan, t.Name); continue; }
            sink.Augment(t.DeclId, RenderEqualsMembers(t));
        }
    }
}
```

## 超出 C# 的能力（契约 vs 追加，D4）

| C# 局限 | z42 目标 | 契约 now / additive |
|---------|---------|:---:|
| 生成器只能追加，逼 `partial` | `Replace`/`Augment` | 契约 now |
| 分析器/修复拆两类型，修复仅 IDE | 同套 splice，`--fix` build 期应用 | additive |
| 分析停在 IOperation | 下探 IR 层性能 lint | additive（加 `OnIrOp`） |
| 分析器需独立 assembly | 同 VM/zpkg/反射加载 | 契约 now |
| 增量需手写 provider 管线 | 声明式 triggers | 契约 now |

fix 附带复用 GenSink splice：
```z42
d.Report(ReadonlyRule, field.NameSpan, field.Name)
 .WithFix("加 readonly", s => s.Replace(field.DeclId, "readonly " + ctx.OriginalOf(field.DeclId)));
```

## Pipeline 挂载 + 有界多轮

```
z42c.syntax    parse ──▶ [Analyzer.OnSyntaxNode/Tree] + [Generator.PostInit]
z42c.semantics bind  ──▶ [Analyzer.OnSymbol/Operation/Body/Reference] + [OnCompilationStart→累积]
                     └─▶ [Generator.Generate]  (GenSink：Add/Replace/Augment)
z42c.pipeline  merge 生成源 ──▶ 重新 parse+typecheck「用户码∪生成码」──▶ [Analyzer.OnCompilationEnd]
z42c.IrGen     lower ──▶ [built-in directive: layout/extern → descriptor 位] ──▶ zpkg
```
**有界多轮**：默认单轮。generator 声明 `consumes(tag)/produces(tag)` → 拓扑序、后轮 GenContext 含前轮生成
符号；硬上限轮数、成环报错；轮内遍历按稳定键 sort。

**护栏（红线，v1 强制）**：① replace/augment 冲突→确定性报错；② 多轮有界+拓扑序+产物纳入指纹；
③ 外部 handler 禁上 z42c 自举路径；④ 信任模型（D5）：直接信任 + 记账 defer 沙箱，但补确定性约束——handler
须为其声明输入的纯函数，禁读环境态（fs/时钟/随机），先文档化、将来用 VM 能力限制强制。

## Implementation Notes：zpkg 持久化 + 4 pass 迁移

**持久化 / bump**：

| 机制 | 进 zpkg | 载体 | bump? |
|------|:---:|------|:---:|
| store-meta | 是 | IrAttrRef blob（已有） | 否 |
| attribute usage（D1） | 是 | IrClassDesc.attr_usage | 是 |
| extern | 是 | 现有 stub | 否 |
| layout（E2） | 是 | descriptor layout 字段 | 是（做时） |
| deprecated（D2 持久） | 是 | flag+msg 串池 | 是 |
| caller-kind（D3） | 是 | param 枚举 flag | 是 |
| Generator 产物 / Analyzer | 否 | —— | 否 |

**现有 4 pass 收敛**：`AttributeSynth`→store-meta（不变）；`StubEmitter`→directive `extern`；
`TestIndexBuilder`→store-meta（反射发现，退休 TIDX 段）；`BenchmarkDesugar`→内建 Generator；
struct Equals/Hash/ToString（在做）→内建 Generator（derive 样板）。

## Decisions（决策记录，已定）

| # | 决策 | 定案 |
|---|------|------|
| D1 | attribute 定义形态 | 引入一等 `attribute` 声明 + `targets/single/inherited` 子句；排 PR4，带 bump |
| D2 | deprecated 第一版 | 直接持久化版（跨包+IDE，带 bump）；廉价验范式已由 PR2 分析器达成，不做半残无 bump 版 |
| D3 | caller 形态 | C# 式表达式宏默认值（最简、无 ABI）；Rust `[track_caller]` 留作宏注册表将来追加 |
| D4 | v1 契约范围 | 契约 now：DeclId+merge+Add/Replace/Augment+有界多轮；additive later：`--fix`、IR-lint |
| D5 | 外部 handler 信任 | 直接信任 + 记账 defer 沙箱；补确定性约束（handler 须纯函数，禁读环境态） |
| D6 | 命名 | marker `[Generator]`/`[Analyzer]` 归 `Std.Meta`；lint config = `packages.toml [lints]` 段 |
| gap1 | handler 执行序 | 同 phase 内按 handler 稳定 Id 排序 |
| gap2 | analyzer 启用 | 依赖即注册；规则级由 `EnabledByDefault` + `[lints]` 控制（含整包开关） |

## Testing Strategy（自举不动点验证）

- **PR1（纯内部重构）**：无语义输出变化 → gen1==gen2 逐字节；`xtask test` 全 stage gate；self-host 5/5。
- **带 bump 的 PR（D1/D2/D3/E2）**：两阶段自举 + `xtask test bootstrap`；fixture 重生按 escape-stack 经验
  （临时 CI 步 + 本地对应 VM 验）。
- **Generator/Analyzer**：生成物 golden（源码+产物双验）、诊断 golden（Id/severity/span）、产物/诊断确定性
  （同源多跑逐字节一致）。
- **红线自检**：外部 handler 不在 z42c 自建路径；generator 产物进增量 cache 指纹。

## Deferred

用户可写 `macro`/自定义 derive；handler 沙箱；`layout`(E2)；IR 层性能 lint；Rust `[track_caller]` 多层传播。
