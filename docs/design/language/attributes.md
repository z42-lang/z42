# 自定义 Attribute 与反射

> 状态：class-level（C3a）+ method-level（C3b）均已落地（2026-06-09）。**D8 后缀约定**（类名强制
> `*Attribute`、应用剥后缀、`E0444`）2026-08-20 落地（attribute-handler-registry PR2）。**编译期 handler**
> 体系（`Analyzer` 契约 + `AnalyzerDriver` + 外部 analyzer zpkg 加载：z42.toml `[analyzers]` 段声明、
> 编译期加载运行、不链入目标程序——D9）随 PR3a / PR3a-load 落地；诊断 severity 开关（z42.toml `[lints]`
> 段：逐规则覆盖 + `pkg.*` 通配 + `warnings-as-errors`，结合规则 `EnabledByDefault`）随 PR3b 落地。
> **局部抑制**（`#suppress`/`#restore` 源码指令 + `[Suppress]` directive，纯编译期不写 zpkg）随 PR3c 落地
> （见下「局部抑制诊断」）。本页主讲 **store-meta attribute**（`[X]`
> 反射元数据），handler 全貌见下方 design 链接。
> 相关：[reflection.md](reflection.md)（GetType / typeof / 反射对象）；
> [attribute-handler-registry design](../../spec/changes/attribute-handler-registry/design.md)（handler 体系全貌）。

z42 的 attribute 是**用户自定义的元数据注解**，应用到声明上，运行期经反射读回**活实例**。设计取自 C#，但修正了 C# 的几处长期缺陷（见下「对 C# 的改进」）。

## 用法

attribute 是一个继承 `Std.Attribute` 的普通类，**类名必须以 `Attribute` 后缀结尾**（D8 后缀约定，
编译器强制为 `E0444`），应用时**剥去后缀**（类 `RouteAttribute` 写作 `[Route]`）：

```z42
using Std;

class RouteAttribute : Attribute {        // 定义名带后缀
    public string Path;
    public string Method;
    // 全部状态走构造器（命名参数 + 默认值）——单一初始化路径
    public RouteAttribute(string path, string method = "GET") {
        this.Path = path;
        this.Method = method;
    }
}

[Route("/users", method: "POST")]          // 用名剥后缀
class UsersController { }

void Demo() {
    Type t = typeof(UsersController);

    // 全部 attribute：活实例，按应用顺序
    Attribute[] all = t.GetCustomAttributes();   // [ RouteAttribute 实例 ]

    // 按类型单查（不存在返回 null）——反射用真实类名（带后缀）
    RouteAttribute r = (RouteAttribute) t.GetAttribute(typeof(RouteAttribute));
    Console.WriteLine(r.Path);    // "/users"
    Console.WriteLine(r.Method);  // "POST"

    // 缓存：对同一 Type 重复调用返回同一批实例
    Attribute[] again = t.GetCustomAttributes();   // 同一数组 / 同一实例
}
```

- attribute 参数限**编译期常量**（字面量 / enum 成员 / `typeof`）。
- 应用位剥后缀：类 `RouteAttribute` 写作 `[Route]`（编译器把 `[X]` 展开成 `XAttribute` 解析）。缺后缀的
  `: Attribute` 类（如 `class Route : Attribute`）→ 编译错误 `E0444`。
- 反射引用类型用**真实类名**（`typeof(RouteAttribute)`），`GetAttribute(Type)` 按实例运行期类型（FullName）
  比较，故子类 attribute 也匹配。

### API（class-level，z42.core）

| 类型 / 成员 | 说明 |
|------------|------|
| `Std.Attribute` | 所有用户 attribute 的基类 |
| `Type.GetCustomAttributes() : Attribute[]` | 该类型的全部 attribute 活实例（缓存）|
| `Type.GetAttribute(Type) : Attribute?` | 指定类型的单个 attribute，无则 null |

### method-level（C3b）

方法（含顶层函数）上的 attribute 同样可反射，经 `MethodInfo`：

```z42
class Service {
    [Doc("lists users")] [Route("/users")]
    public void List() { }
}

Type t = typeof(Service);
foreach (MethodInfo m in t.GetMethods()) {
    if (m.Name == "List") {
        Attribute[] attrs = m.GetCustomAttributes();        // [ DocAttribute, RouteAttribute ]
        DocAttribute d = (DocAttribute) m.GetAttribute(typeof(DocAttribute));  // d.Text == "lists users"
    }
}
```

机制完全镜像 class-level：编译期合成工厂（key `mth$<Class>$<Method>`）；元数据载体改为 **SIGS section**（per-function，vs class 的 TYPE section）；运行期 `__method_custom_attributes(qualified)` 按方法限定名解析 `Function.custom_attributes` → 调工厂 + 缓存。`MethodInfo` 携带隐藏 `__qualified` 供 builtin 解析。

## 对 C# 的改进（z42 取舍）

| # | C# 缺陷 | z42 |
|---|---------|-----|
| 1 | `Attribute` 后缀**可选** + 双拼法（`class FooAttribute` 写作 `[Foo]` 或 `[FooAttribute]`；无后缀类也合法，只有 CA1710 软警告）| **后缀强制、单拼法**（D8）：类名必须 `*Attribute`（否则 `E0444` 硬错），应用只接受剥后缀的 `[Foo]`。比 C# 更严——消除"两种拼法 / 加不加后缀"的自由度 |
| 2 | 双初始化路径（positional→ctor，named→public 字段直写）| **单一 ctor 路径**：全部参数走构造器（复用 named-arg + 默认值）|
| 3 | 每次 `GetCustomAttributes()` 重新分配新实例 + 返 `object[]` | **缓存单例**：首次实例化一次，后续返同一批 |
| 4 | 实例可变（正是 #3 必须复制的根因）| **不可变实例**（ctor 内一次写定）→ 缓存安全 |
| 5 | `AttributeUsage` 是自循环元属性 + 反直觉默认 | **推后**；将来做一等声明子句（非元属性）|

**刻意保留的 C# 约束**：参数限编译期常量（安全 + "attribute 是数据非行为"心智模型）；attribute 被动（数据，不主动变换被注解项——主动注解属 z42 customization/macro 层，与此分开）。

## 实现原理

### Factory thunk：活实例不碰 0.5.x-deferred 的 Activator/Invoke

attribute 构造**全编译期已知**（已知类、已知构造器、常量参数），无需运行时泛型实例化。编译器为每个 `[Foo(args)]` 应用合成一个**无参工厂函数**（[AttributeFactorySynthesizer](../../../src/compiler/z42.Semantics/Codegen/AttributeFactorySynthesizer.cs)，pre-typecheck，复用 BenchmarkDesugar 合成模式）：

```z42
public Attribute __attr$cls$UsersController$0() { return new RouteAttribute("/users", method: "POST"); }
```

工厂的**返回类型是 `Attribute` 基类**——于是普通 typecheck 顺带强制了 attribute 契约，错误锚定在应用点：
- 非 `Attribute` 派生类 → return 上转型失败（"不是 attribute"）；
- 非常量参数 → 在无参工厂作用域里成为未知标识符；
- 构造器不匹配 → 正常重载解析报错。

无需独立 validator pass。

### 元数据持久化（zbc 1.10）

每个 class 在 `.zbc` TYPE section 记 `attr_count: u16` + 每条 (`type_name`, `factory_func`) 字符串引用（见 [runtime/zbc.md](../runtime/zbc.md) 1.10）。运行期 loader 把它们载入 `TypeDescCold.custom_attributes`。

### 反射时构造 + 缓存

native `__type_custom_attributes(type)` 对每条 ref 用 `run_returning` 调工厂函数拿活实例（`exec_function` 的每调用状态在栈局部 `Frame`，native→VM re-entrancy 安全）。z42 层 `Type.GetCustomAttributes()` 把结果缓存到 Type 对象（`__attrCache`），故重复调用返回同一批实例。跨 zpkg：工厂函数经 lazy loader 跨模块解析。

## 局部抑制诊断：`#suppress` / `[Suppress]`（PR3c）

关闭某个 analyzer 诊断规则（对应 C# `#pragma warning disable/restore` + `[SuppressMessage]`）。**两机制皆纯编译期、零 zpkg 持久化**——抑制信息只在编译该文件时存在，不写进产物：

```z42
#suppress Z9002 "这段是生成代码"      // 区间起
try { risky(); } catch (Error e) { }   // 此处 Z9002 被抑制
#restore Z9002                          // 区间止（省略则延伸到文件尾）

[Suppress("Z9002")] void Hot() { ... }  // 声明级：整个 Hot 及其内部抑制 Z9002
```

- **`#suppress <Id> ["reason"]` / `#restore <Id>`**：源码指令（`#` 词法产 `Hash` token，此前无语义）。parser 在语句列表 / 顶层声明列表边界收集成 `CompilationUnit.SuppressRegions`（`SuppressRegion{RuleId,Start,End}` 字节区间；AST-only，不序列化）。`at.Start ∈ [Start, End)` 且规则 Id 相等 → 抑制。精确 Id 匹配、无通配（通配是 `[lints]` config 层的事）。
- **`[Suppress("<Id>","reason")]`**：**directive**（非 store-meta）——`HandlerRegistry.IsDirectiveAttr` 认它、`KindOf → Directive`，故 `AttributeSynth` 不合成反射工厂（**不写 blob**）、`StubEmitter` 不烘（**不入 descriptor**），也**无需 `SuppressAttribute` 类**（同 `[Native]` 靠名字识别）。`AnalyzerDriver` walk 到带 `[Suppress]` 的声明时把 Id 压入活跃栈、退出弹出（**不用 decl.Span**——本 parser 的 decl span 只覆盖起始 token，活跃栈按 AST 结构精确圈定子树）。`Suppress` 是保留 directive 名，用户不能再定义同名 store-meta 类。
- 判定统一在 `DiagSinkImpl.Report`（z42c.semantics `AnalyzerDriver.z42`）：severity 决策**之前**先查 `SuppressionSet.IsSuppressed(ruleId, at.Start)`，命中即整条丢弃、不进 `DiagnosticBag`。
- 全貌见 [attribute-handler-registry design](../../spec/changes/attribute-handler-registry/design.md) §诊断/severity/开关「PR3c 落地」。

## Deferred / Future Work

### ~~attribute-future-method-level（C3b）~~ — 已落地
- **状态**：method-level（`MethodInfo.GetCustomAttributes()` / `GetAttribute`）已落地 2026-06-09（add-attribute-reflection-methods，SIGS section per-function attr refs，zbc 1.11）。见上文「method-level（C3b）」。

### ~~attribute-future-fqn-class-resolution（编译器 bug，C2/C3b 共因）~~ — 已修
- **状态**：已修 2026-06-09（[archive/2026-06-09-fix-fqn-class-resolution](../../spec/archive/2026-06-09-fix-fqn-class-resolution/)）。
- **根因**：限定名 `Std.Type` 被解析为 `MemberType`，走 `SymbolTable.ResolveMemberType` —— 该函数原只查嵌套 delegate，未识别命名空间限定的类 → 返 `Z42Type.Unknown`；其上成员访问命中 [TypeChecker.Exprs.Members.cs](../../../src/compiler/z42.Semantics/TypeCheck/TypeChecker.Exprs.Members.cs):174 的**静默 fallback** `BoundMember(Unknown)` → 运行期 null 字段读，属性 getter 永不派发。C2 typeof + C3b MethodInfo 共因（都曾用短名/委托绕过）。
- **修复**：`ResolveMemberType` 先把 dotted-path 拍平 + `ResolveQualifiedType`（namespace-aware：短名是已知类 **且** `ImportedClassNamespaces` 记录的命名空间 == FQN 前缀才解析；零回归）。C3b 的 `Type.FindByType` workaround + `MethodInfo` 的 `using Std` 已移除，恢复自然 inline `Std.Type at = ...; at.FullName`。验证：GoldenTests 1545/1545。
- **残留项（follow-up，2026-06-09 实测复核）**：Members.cs:174 对未匹配任何解析路径的成员访问静默返 `BoundMember(Unknown)`。**实测确认：这不是"小 fix"。** 该静默路径是 typechecker 的**"不静态建模、交 VM 运行期解析"逃生通道**，被 enum 成员（`Direction.North`）、类名静态成员（`Counter.count`）、异常内建属性（`e.Message`）、字符串 `.ByteLength`、链式反射（`GetType().__fullName`）等**合法写法**依赖。把它收紧为报错(即便只对非 Unknown/Error 接收者、保留 poison cascade 抑制)会把 **26 个合法 golden 程序**（横跨 enums / statics / exceptions / strings / reflection）变成编译错误。真正修复需 typechecker 完整静态建模上述每类成员——多子系统工程，**非局部改动，保持延后**；不要尝试一行收紧。

### attribute-future-dedicated-diagnostics
- **来源**：add-attribute-reflection（C3a）。
- **触发原因**：契约违例当前借工厂 typecheck 报通用错误（"cannot return X"/"unknown identifier"），非专用 E09xx（"X 不是 attribute"/"参数须为常量"）。
- **触发条件**：需要更清晰诊断 + stdlib-aware negative 测试 harness 时。

### attribute-future-attributeusage
- **触发原因**：MVP 不校验 target（attribute 可标任意支持位）。
- **触发条件**：需要 target 限制时；做成声明上的一等子句（非 C# 元属性自循环），默认 AllowMultiple=true、无隐式继承。

### ~~attribute-future-targets-fields（field 目标）~~ — 已落地（2026-06-10）
- **状态**：字段级用户 attribute 已落地 2026-06-10（add-field-attribute-reflection）。`[Doc("x")] public int f;`（实例 + 静态字段）经 `FieldInfo.GetCustomAttributes()` / `GetAttribute(Type)` 反射活实例。机制同 C3a/C3b（factory-thunk，key `fld$<Class>$<Field>`）；attr refs 进 zbc TYPE section 每字段记录（zbc 1.14 / zpkg 0.16），运行期索引进 `TypeDescCold.field_attributes`。

### ~~attribute-future-targets-params（parameter 目标）~~ — 已落地（2026-06-10）
- **状态**：参数级用户 attribute 已落地 2026-06-10（add-parameter-attribute-reflection）。`void M([Tag("x")] int p)` 经 `ParameterInfo.GetCustomAttributes()` / `GetAttribute(Type)` 反射活实例。机制同 field（factory-thunk，key `__attr$<funcKey>$prm$<idx>$<n>`）；attr refs 进 zbc SIGS section 每函数记录的 per-parameter attr 块（zbc 1.15 / zpkg 0.17，每参数 `u16 count` + 对偶，含实例方法 `this` 空槽），运行期载入 `FunctionCold.param_attributes`，`__param_custom_attributes(qualified, position)` 按源参数位置取（wire 索引 = position + this 偏移）。parser 在参数列表解析 leading `[Attr]`（此前丢弃）。

### attribute-future-generic-and-typed-lookup
- **触发原因**：泛型 attribute 类 + `GetAttribute<T>()` 泛型糖依赖 0.5.x 泛型方法实例化。MVP 用 `GetAttribute(typeof(T))`。

### attribute-future-raw-args-view
- **触发原因**：factory-thunk 只给活实例，无 C# `CustomAttributeData` 式不实例化读原始 ctor 参数的视图。
