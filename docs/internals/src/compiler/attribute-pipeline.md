# attribute 管线（store-meta 一支）

> 对齐：2026-09-17 ｜ 代码：`z42c.semantics/src/AttributeSynth.z42`、`HandlerRegistry.z42`、
> `MacroRegistry.z42`、`AnalyzerDriver.z42`

用户面的写法（后缀约定、五个反射载体、`#suppress`、caller 宏）见 reference 的
[自定义 Attribute 与反射](../../../reference/src/language/attributes.md)。本页写**编译器怎么把
`[X]` 变成运行期能读回的活实例**。

> 范围：本页只讲 **store-meta** 一支——即「`[X]` 是纯元数据，运行期经反射读回」。
> 编译期 **handler** 体系（`Analyzer` / `Generator` / `ModuleGenerator` 契约、`AnalyzerDriver` /
> `GeneratorDriver`、外部 analyzer zpkg 加载、有界多轮）的全貌仍在
> `docs/spec/changes/attribute-handler-registry/design.md`，尚未上浮。

## 三路分流：`HandlerRegistry.KindOf`

每个 `[X]` 先分类，这是整条管线的**唯一入口判据**（`HandlerRegistry.z42:57-61`）：

| kind | 判定 | 处理 |
|---|---|---|
| **Directive** | `IsDirectiveAttr`：`Native` / `Suppress` / `Deprecated` / `Record`（`:78-83`）| 靠**名字**识别，不需要用户建 backing 类；不合成反射工厂 |
| **Handler** | `IsTestHandlerAttr`：test 全家族 8 名 | 编译器内建，由各自的 pass 消费 |
| **StoreMeta** | 其余全部 | 按 D8 后缀约定展开成 `<name>Attribute` 类，合成反射工厂 |

后缀展开只有一处：`HandlerRegistry.StoreMetaClassName(appliedName)`（`:69-71`），被
`AttributeSynth._synthFactory` 的 `new <cls>(args)` 与 `ClassDescBuilder` 的 `IrAttrRef` 类型名
两处消费。directive / handler **豁免后缀**，也不走这条路。

> Directive 里 `Suppress` 是纯编译期的，`Deprecated` / `Record` / `Native` 则各自有持久化。
> 「归 directive」只保证 `KindOf != StoreMeta`（⇒ 不合成反射工厂），不代表不写产物。

## Factory thunk：活实例不依赖 Activator / Invoke

attribute 的构造在编译期**全部已知**（已知类、已知构造器、常量实参），所以不需要运行时的泛型
实例化。`AttributeSynth`（`AttributeSynth.z42:19`，parse 之后 / typecheck 之前的 AST 级下降）
为每一处 `[Foo(args)]` 合成一个**无参工厂自由函数**：

```z42
[Route("/u", method: "POST")] class C { ... }
  ⇣
public Attribute __attr$cls$C$0() { return new RouteAttribute("/u", method: "POST"); }
```

工厂名记进 `Attr.FactoryFunc`。工厂 key 的前缀区分载体：
`cls$<C>` / `mth$<C>$<M>` / `fld$<C>$<F>` / `fn$<F>`，参数级再加 `$prm$<j>`。

**工厂的返回类型写成 `Attribute` 基类**——于是普通 typecheck 顺带把 attribute 契约全检了，
而且错误锚点落在**应用处**，不需要单独写一个 validator pass：

- 类不是 `Attribute` 派生 → `return` 上转型失败；
- 实参不是常量 → 在无参工厂的作用域里变成未知标识符；
- 构造器对不上 → 正常的重载解析报错。

代价是诊断文本是通用的（`cannot return X` / `unknown identifier`）而不是专用的
「X 不是 attribute」/「参数须为常量」。专用诊断是 Deferred（`attribute-future-dedicated-diagnostics`）。

挂载点是 `HandlerRegistry.RunAst(cu)`（`:46`），它规定了顺序：**内建 Generator
（`BenchmarkDesugar`）先跑，再跑 `AttributeSynth`**。

## 持久化：attr-ref 进 zbc，property 例外

编译期只往产物里写 (`type_name`, `factory_func`) 两个字符串引用，不写实例。各载体的落位：

| 载体 | zbc 段 | 运行期落进 |
|---|---|---|
| class | TYPE 段每 class 记录 | `TypeDescCold::custom_attributes` |
| field（实例 + 静态）| TYPE 段每字段记录 | `TypeDescCold::field_attributes` |
| method / 顶层函数 | SIGS 段每 function 记录 | `FunctionCold::custom_attributes` |
| parameter | SIGS 段每函数的**逐参数** attr 块（含实例方法的隐式 `this` 空槽）| `FunctionCold::param_attributes` |
| **property** | **没有自己的 wire** | —— 见下 |

逐字段偏移见 [zbc 格式](../formats/zbc.md)。

**property 走 backing 字段**：自动属性脱糖出私有 backing 字段 `__prop_<Name>`，编译器把属性上的
attribute 挂到那个字段上；`__property_custom_attributes(qualified)` 拿 accessor 的限定名
（优先 getter，否则 setter）、剥掉 `get_` / `set_` 前缀定位 backing 字段，从它的 `field_attributes`
里取。**零新增 wire、零格式 bump**，代价是**计算属性（无 backing 字段）永远读不到 attribute**。

这个设计还刻意避开了「依赖一个由 VM 写出的 property-qualified 字段」——bootstrap 种子可能没有它。

## 反射时构造 + 缓存

四个（现为五个）builtin 是运行期入口（`corelib/builtin_table.rs:206-212`）：
`__type_custom_attributes` / `__method_custom_attributes` / `__field_custom_attributes` /
`__param_custom_attributes` / `__property_custom_attributes`。

每个 builtin 对拿到的每条 attr-ref 用 `run_returning` 调它的工厂函数取活实例——`exec_function` 的
每调用状态都在栈局部 `Frame` 里，所以 native→VM 的重入是安全的。跨 zpkg 的工厂函数经 lazy loader
解析。

z42 一侧（`Type.z42` / `Reflection/*.z42`）把结果缓存在反射对象的 `__attrCache` 字段上，
所以对同一个反射对象重复调用返回**同一批实例**。这也是 z42 敢返回 `Attribute[]` 而不是 C# 那样
每次新分配 `object[]` 的前提：实例在 ctor 内一次写定、此后不可变，共享安全。

## `#suppress` / `[Suppress]`：两条路，都不写产物

- **`#suppress <Id> ["reason"]` / `#restore <Id>`** —— 源码指令。`#` 词法产 `Hash` token（此前无语义）。
  parser 在语句列表 / 顶层声明列表的边界收集成 `CompilationUnit.SuppressRegions`
  （`SuppressRegion{RuleId, Start, End}` 字节区间；**AST-only，不序列化**）。判定是
  `at.Start ∈ [Start, End)` 且规则 Id 全等——**精确匹配、无通配**（通配归 `z42.toml` 的 `[lints]`）。
  规则 Id 缺失时诊断 + 返回空串，抑制退化为无效（安全方向）。
- **`[Suppress("<Id>","reason")]`** —— directive：`HandlerRegistry.IsSuppressDirective` 认它、
  `KindOf` 判 `Directive`，于是 `AttributeSynth` 不合成工厂（**不写 blob**）、`StubEmitter` 不烘
  （**不入 descriptor**），也**不需要存在一个 `SuppressAttribute` 类**。`AnalyzerDriver` walk 到带
  `[Suppress]` 的声明时把 Id 压入**活跃栈**、退出时弹出。
  > ⚠️ 这里**不能用 `decl.Span`**：本 parser 的 decl span 只覆盖起始 token，用它圈范围会漏掉整个体。
  > 活跃栈按 AST 结构走，圈定的是真正的子树。

两条路汇合在 `DiagSinkImpl.Report`（`AnalyzerDriver.z42:26-38` 持有 `SuppressionSet Supp`）：
**severity 决策之前**先查 `SuppressionSet.IsSuppressed(ruleId, at.Start)`，命中即整条丢弃、
根本不进 `DiagnosticBag`。

## 编译期宏：哨兵译法

四个 caller 宏（`caller_member` / `caller_line` / `caller_file` / `module_path`）与
`available` 共用一套机制，注册表是 `MacroRegistry.z42`。

**parser 侧不新增任何 token 或 AST 节点**：`name!()` 被译成既有的
`IdentExpr("$macro:" + name)` 哨兵（`ExprParser.z42:493,496`）。`$` 在真标识符里非法 ⇒ 零撞名。
带参形态 `name!(x)` 由 postfix 循环自然产出 `CallExpr(IdentExpr(哨兵), args)`。

> 为什么要这么绕：这样 `z42c.semantics` 只靠既有的 `IdentExpr.Name` 就能识别，
> **不新增任何 syntax→semantics 的跨包符号** —— 避开 F2 冷启动 stale-cache
> （见 `two-gen-bootstrap-regressed-blocks-format-bumps` 的教训）。同理，`MacroRegistry` 全部
> 留在 `z42c.semantics` **包内**。

`MacroRegistry` 的两张表：

- `PositionOf(name)` —— **宏白名单的单一真相**：caller 族 → `"param-default"`，`available` → `"expr"`，
  未知 → `""`（即 `E0450`）。⚠️ `KindOf` 返回 `""` **不等价于**「未知宏」（`available` 也返回 `""`），
  判未知一律用 `PositionOf`。
- `KindOf(name)` —— caller 族内部的 kind（`member` / `line` / `file` / `module`）。
- `ParamTypeOk(kind, typeName)` —— 定义侧类型校验（`line` 要 `int`，其余要 `string`）。

**发码**：`DeclBinder._validateCallerMacroDefaults`（`:554`）用**字面量** `"E0450"` 发（`:566,569,577`），
不引用 `DiagnosticCodes.CallerMacroInvalid` 常量——同样是避 core→semantics 新跨成员符号撞 F2 冷启动
stale-cache。这条纪律对 `E0449`–`E0470` 一族普遍适用。

**持久化**：caller 宏的默认值编成 `$Caller:<kind>` 的 param attr-ref 哨兵（`FactoryFunc` 为空），
骑既有的 param attr 通道，**零格式 bump**。`IrParamDefault.Caller()` 读回、`ImportedSymbolLoader`
填 `Z42FuncType.ParamCallers`。
**注入**在调用点：`OverloadBinder._callerLiteral` 在实参省略时注入 enclosing member / line / file /
namespace 的字面量——所以跨包调用注入的是**消费方**的上下文。

`available` 不走哨兵持久化，它在 codegen 直接变成一条 `Builtin` 指令、加载期被 VM 折成常量，
见[符号可用性折叠](../runtime/availability-folding.md)。

## 活的设计约束：成员访问的静默 `Unknown` 松绑

`MemberResolver` 在几处对「解析不出来的成员访问」**不报错**，而是松绑成
`BoundMember(..., Z42UnknownType)` / `BoundCall(..., sig=null)`，把真正的解析交给运行期：

| 位置 | 场景 |
|---|---|
| `MemberResolver.z42:305` | 泛型型参（`TKey` / `TValue`）上的成员访问 |
| `MemberResolver.z42:330` | 泛型实例化类型上找不到字段也找不到 `get_X` |
| `MemberResolver.z42:347` | prim wrapper 类上找不到字段也找不到 `get_X` |
| `MemberResolver.z42:102` / `:193` | class 收者 / 泛型实例化收者的方法调用查不到 → `sig=null` |
| `MemberResolver.z42:231-232` | 型参收者的方法调用，Object 与约束接口都查不到 → `sig=null` |
| `MemberResolver.z42:271-272` | prim 收者的方法调用查不到 → `sig=null` |

**这不是可以顺手收紧的小 fix。** 这条逃生通道被大量**合法写法**依赖——enum 成员访问、类名静态成员、
异常内建属性、字符串上的 extern 属性、链式反射等。历史上实测过一次：即便只对非 Unknown/Error 接收者
收紧、并保留 poison cascade 抑制，也会把 **26 个合法 golden 程序**变成编译错误（横跨 enums /
statics / exceptions / strings / reflection）。真正的修法是让 typechecker 对上述每一类成员**完整静态
建模**，是多子系统工程。

> 注意 `sig == null` 的连带后果：`OverloadBinder.CheckArgTypes` 第一行就是 `if (sig == null) return;`
> ⇒ 走到松绑分支的调用**实参一律不检查**。`bind-self-param-and-constraint-members` 已经把「型参收者
> 查约束接口方法」这一支从松绑里救了出来（拿到真签名 + `E0463`），剩下的仍然是松的。
> 全落空的**属性**访问（非方法）今天已经会报 `E0402 member access on non-class`，不再静默。

## Deferred

- `attribute-future-dedicated-diagnostics` —— 契约违例现在借工厂 typecheck 报通用错误，
  需要专用 E09xx + stdlib-aware 的 negative 测试 harness。
- `attribute-future-attributeusage` —— target / 重复性限制。要做成声明上的一等子句（不是 C# 那种
  自循环元属性），默认 `AllowMultiple=true`、无隐式继承。
- `attribute-future-generic-and-typed-lookup` —— 泛型 attribute 类 + `GetAttribute<T>()` 泛型糖。
- `attribute-future-raw-args-view` —— factory-thunk 只给活实例，没有 C# `CustomAttributeData` 那种
  「不实例化就读原始 ctor 实参」的视图。

## 关联文档

- 用户面写法：[自定义 Attribute 与反射](../../../reference/src/language/attributes.md)
- wire 布局：[zbc 格式](../formats/zbc.md)
- `available!()` 的运行期折叠：[符号可用性折叠](../runtime/availability-folding.md)
- handler / generator / analyzer 体系全貌：`docs/spec/changes/attribute-handler-registry/design.md`
