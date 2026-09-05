# Proposal: `methodof` —— 方法引用表达式（与 `typeof` 对称，零格式 bump）

## Why

z42 目前**没有任何在代码里精确指代一个方法的手段**。`typeof(T)` 能指代类型（`BoundTypeof` /
`TypeOpTyper.z42:57` / `TypeOpEmitter.z42:61-64`），但方法这一侧是空的：反射要拿一个
`MethodInfo` 只能走 `typeof(X).GetMethods()` 再**按字符串名筛**，重载还筛不开。

这个缺口在 attribute 场景最痛——想让 attribute 记录「这个声明关联哪个函数」（路由表、事件注册、
序列化字段选择器、测试桩、转发面声明），今天只能塞字符串。

**字符串方案的根本问题是它不是强类型的**：

| | `[Route("HandleGet")]` 字符串 | `[Route(methodof(Api.HandleGet))]` |
|---|---|---|
| 方法不存在 | 运行期才发现，或**静默找不到** | **编译错误** |
| 方法被重命名 | **静默失效**，无任何提示 | **编译错误**，改名处立刻红 |
| IDE 重命名重构 | 不跟随（字符串字面量） | **自动更新**（真符号引用） |
| 重载 | 指不明 | 参数类型列表精确选中 |
| 签名漂移（参数改了） | 运行期 `Invoke` 才炸 | 编译期匹配不上即报错 |

「函数名改了不容易检查出来」正是这个特性要根治的问题——把一个**运行期的、静默的**失效模式
变成**编译期的、必然暴露**的错误。

**这不是 z42 独有的困境，但 z42 有别人没有的解法。** C# 和 Java 都做不到，而原因**不在语言、在
元数据格式**：

- ECMA-335 II.23.3 的 CustomAttrib blob 只允许基元 / string / `System.Type` / object / 装箱值类型 /
  一维数组，**没有任何 MethodRef 编码**；连 `typeof(T)` 在 blob 里都是一个 SerString（类型的
  canonical name）。
- JVM 的 `element_value` tag 集合 `B C D F I J S Z s e c @ [` 同样没有方法引用。
- **两个平台的字节码都能表示方法常量**（IL 的 `ldtoken` 取 method token；JVM 常量池的
  `CONSTANT_MethodHandle_info` tag 15）——缺口纯粹在注解格式，格式在方法常量出现之前就冻结了。

csharplang 官方口径印证（[discussion #4771](https://github.com/dotnet/csharplang/discussions/4771)）：
「supported types … are limited by the **runtime**. The runtime would need to be modified … and
**establish how those types are encoded into the metadata**.」
`infoof` / `methodof` 提案（[roslyn#128](https://github.com/dotnet/roslyn/issues/128)）已 CLOSED，
[LDM 2014-10-15](https://github.com/dotnet/csharplang/blob/main/meetings/2014/LDM-2014-10-15.md)
正式否决。

**z42 不受这个约束，而且已经绕过去了**：z42 的 attribute 不走常量 blob，走**工厂函数**——
`AttributeSynth._synthFactory` 把 `at.Args`（原始 `Expr[]` AST）原样塞进
`ObjNewExpr` 合成 `public Attribute __attr$<key>$<i>() { return new XAttribute(args); }`，
元数据里 `IrAttrRef` 只存 `(attrTypeName, factoryFuncName)` 两个字符串，运行时
`GetCustomAttributes` 调工厂函数把对象造出来。**attribute 实参本来就是任意表达式，无常量约束**
（已 grep 确认无常量性校验；`ConstBlob`/`$Default` 是参数默认值机制，与此无关）。

所以 `methodof` 只需要是一个合法表达式，**元数据侧零工作**。

## What Changes

### 语法：`methodof(Type.Method(参数类型列表))`

```z42
methodof(Logger.Log)                      // 无重载时
methodof(Logger.Log(string))              // 参数类型列表消歧
methodof(Logger.Log(List<int>, int))      // 泛型零歧义（见下）
methodof(Logger.get_Level)                // 属性访问器
```

**括号内切换到签名语法，不是表达式语法。** 这是本设计的技术关键：在表达式文法里
`Logger.Log(List<int>)` 的 `List<int>` 与 `<` 比较运算真歧义；`methodof(...)` 的括号是一个封闭
作用域，里面 `<` 就是类型实参，歧义消失。语法是**局部的**，不污染表达式文法。

> Eric Lippert 当年吐槽 `infoof(Bar(int,int))` 是 *"you've just introduced new syntax; nowhere in
> C# previously did we have a parenthesized, comma-separated list of types"*
> （[In Foof We Trust](https://learn.microsoft.com/en-us/archive/blogs/ericlippert/in-foof-we-trust-a-dialogue)）。
> 对 C# 是负担，对 z42 不是——我们本来就在引入新语法，且它封闭在 `methodof` 括号内。

### 表达式类型：`Std.Reflection.MethodInfo`（已有，不新造）

```
Std.Reflection.MemberInfo   (Name)
  └─ MethodBase             (IsStatic / __qualified / __parameters)
       ├─ MethodInfo        (ReturnType / IsVirtual / 泛型 / __attrCache)
       └─ ConstructorInfo
```

与 `typeof(T)` → `Std.Type` 完全对称：两者都是堆对象、都是编译期已知运行期物化的反射对象、
走同一条 emit 路径、同一种驻留策略。

**不新增值类型 `MethodRef`**（.NET 的 `RuntimeMethodHandle` 那一层），两条理由：

1. 与 `typeof` 对称，用户不需要学第二套心智模型；
2. `types.rs:42-52 default_value_for` 对**任何** struct 名都返回 `Value::Null`（不只带引用字段的），
   值类型 `MethodRef` 会踩 `default(MethodRef)` → Null。要么先修这个 runtime 缺陷，要么绕开。
   轻量句柄层随时可以后加作为优化，反过来先做则要同时扛 struct 坑 + 两个概念。

### 序列化：走既有工厂函数路径，**零元数据改动、零格式 bump**

| 用途 | 路径 | 元数据工作量 |
|---|---|---|
| 运行时 attribute 记录函数 | 工厂函数体里 `methodof(...)` 求值 → `MethodInfo` | **零** |
| 编译期 directive | 编译器直接读 `Attr.Args` 的 AST | **零** |

`$Deprecated` / `$Default` 那套 `$` 哨兵是给「编译期需要读、又不能跑代码」的 directive 用的，
`methodof` 用不上。

#### 存储形态：字符串池索引，与普通 Call 同构

zbc 的 **STRS 段就是字符串池**（`ZbcWriter.z42:38,58` `InternPoolStrings`），所有引用点存 **u32 池
索引**而非裸串——`Call`(`ZbcInstr.z42:50`)、`VCall`(`:67,73`)、`FieldGet/Set`(`:80,86`)、
类型名(`:93`)、类型实参(`:125`) 一律 `w.WriteU32(pool.Idx(...))`。

**`methodof` 的重载解析全部发生在编译期**，落到 IR 时只剩**一个 qualified 名 → 一个池索引**：
签名匹配在运行期零痕迹。同一方法被 `methodof` 引用 N 次 = 池里 1 条 + N 个 u32。
**体积与一次普通函数调用等同。**

**稳定性 = 普通调用的稳定性。** 因为在编译期就解析成 qualified 名，`methodof` 与任何 `Call`
指令面对完全相同的 rekey 敞口——真发生 rekey 时所有调用点一起失效，`methodof` 不引入新的
脆弱性。（这也是为什么不采用「加载期解析」的 `$MethodRef` 哨兵方案：那种设计才会让方法引用
比调用点更脆弱。）

### FQN 保全：照抄 `BoundTypeof.TargetName`

`BoundExprOp.z42:184-188` / `TypeOpTyper.z42:57` / `TypeOpEmitter.z42:61-64` 已有一套
「保留 AST 原始名、发射时 `QualifyClass` 回 FQ 名」的手法，正是为了解决「结构化后 FQN 蒸发」。
`methodof` 需要同一件事，**照抄即可，不是新机制**。

### `&` 保留给将来的非托管函数指针

`&` **不用于**方法引用。分工按语义划线：

| | 产出 | 性质 |
|---|---|---|
| `methodof(Logger.Log(string))` | `MethodInfo` | 托管、带元数据、**可经工厂函数序列化** |
| `&Logger.Log`（将来） | `funcptr<void, string>` 之类 | 裸地址、无元数据、**不可序列化**、interop 用 |

**先例**：C++26 面对同一局面（既要 pointer-to-member 又要反射句柄），给了两个不同的符号——
`&Logger::log` 与 `^^Logger::log`。而且 attribute 位置**没有 target type**（C++ 的
[over.over](https://eel.is/c++draft/over.over) 把可用 target 上下文穷举成 7 条封闭列表，attribute
不在其中），用同一个 `&` 会造成无法消解的永久语法债。z42 的 native interop 需求真实存在
（`[Native]` / extern / `add-native-dep-config-program`），函数指针大概率会来。

### 位置：不限制，同 `typeof`

能在 attribute 里用 ⟹ 在普通代码里自动也能用——attribute 实参就是被塞进工厂函数体编译的普通
表达式，两者走同一条绑定/发射路径。「只允许在 attribute 里」反而要**额外加**一条人为位置检查。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/libraries/z42c.syntax/src/Lexer.z42` | MODIFY | 新增 `methodof`（照 `typeof` 同款，`_kw` 注册） |
| `src/libraries/z42c.syntax/src/Ast.z42` | MODIFY | 新 `MethodOfExpr { OwnerType: TypeExpr, Member: string, ParamTypes: TypeExpr[], HasParamList: bool }` |
| `src/libraries/z42c.syntax/src/ExprParser.z42` | MODIFY | `methodof(` 分支：括号内按**签名语法**解析 `Type.Member` + 可选 `(TypeList)`；不复用表达式解析 |
| `src/compiler/z42c.semantics/src/BoundExprOp.z42` | MODIFY | 新 `BoundMethodOf`，带 `TargetName`（照 `BoundTypeof:184-188` 保 FQN 的手法） |
| `src/compiler/z42c.semantics/src/TypeOpTyper.z42` | MODIFY | 解析 owner 类型 → 按 `Member` 收候选 → 有参数列表则按 `TypeNameResolver.SurfaceTypeName` 规范化后逐位匹配；零匹配/多匹配报诊断 |
| `src/compiler/z42c.semantics/src/TypeOpEmitter.z42` | MODIFY | 发射：产出 qualified 名 → 调 runtime builtin 得 `MethodInfo`（照 `typeof` 同款路径） |
| `src/compiler/z42c.semantics/src/TypeNameResolver.z42` | MODIFY(可能) | 参数类型匹配需要 alias 归一（用户写 `byte[]` vs 渲染出 `u8[]`）；确认 `PrimModel.SurfaceName` 映射可复用 |
| `src/runtime/src/corelib/reflection/` | MODIFY | 新 builtin `__methodof(qualified)` → 复用 `methods.rs:206 build_method_info` 造 `MethodInfo`；模块级驻留缓存 |
| `src/libraries/z42.core/src/DiagnosticCodes.z42` | MODIFY | 新诊断码（见下） |
| `src/tests/reflection/methodof_basic.z42` | NEW | e2e：无重载 / 参数列表消歧 / 属性访问器 / 静态方法 / `Invoke` 往返；jit 双验 |
| `src/tests/attributes/methodof_in_attribute.z42` | NEW | e2e：attribute 实参里放 `methodof(...)`，`GetCustomAttributes` 读回并 `Invoke` |
| `docs/book/src/language/methodof.md` | NEW | 语法 / 与 `typeof` 对称 / 重载消歧 / 指不了的方法 / `&` 的分工 |

**只读引用**（理解上下文必须读，不修改）：
- `src/compiler/z42c.semantics/src/AttributeSynth.z42:127-142` — `_synthFactory` 把 `at.Args` 原样塞进 `ObjNewExpr`，本提案「零元数据改动」的支点
- `src/libraries/z42.core/src/Reflection/MethodInfo.z42` / `MethodBase.z42` — 已有类型层级，`__qualified` 是身份载体
- `src/runtime/src/corelib/reflection/invoke.rs:169 invoke_qualified` — 反射调用核心
- `src/runtime/src/metadata/tokens.rs` — `MethodId`/`TypeId`/`VTableSlot` 是**运行期** token（进程内有效、
  明确不写进 zbc），故持久化形态是 STRS 池索引；运行期首次解析成 token 后由
  `bytecode.rs:530 resolved: OnceLock<ResolvedTokens>` 缓存
- `src/libraries/z42.ir/src/BinaryFormat/ZbcWriter.z42`(:38,58) / `ZbcInstr.z42`(:50,67,80,93) — STRS 池 +
  全引用点 `pool.Idx()` 的证据

## Out of Scope

- **`&` 非托管函数指针 / `funcptr<>` 类型**——独立特性，本提案只负责把符号让出来
- **`fieldof` / `propertyof`**——同家族，等 `methodof` 稳定后按各自收益立案
- **轻量值类型 `MethodRef`**（handle 层）——作为将来的性能优化，需先修 `default_value_for`
- **反射调用的 inline cache**——`methodof` 的驻留缓存会顺带改善，但系统性的反射 IC 是独立工作
- **泛型方法的签名形式消歧**（`methodof(Seq.Map<U>(U))`）——v1 只支持按名字指代泛型方法，
  类型参数名的匹配语义留待后续

## 指不了的方法（诚实的残余）

Lippert 的分析对 z42 同样成立，以下情形 **v1 明确报错、不假装支持**：

1. **泛型基类替换后同签名**：`class C<T> { void Bar(int,T); void Bar(T,int); void Bar(int,int); }`
   在 `D : C<int>` 上三个候选替换后全是 `(int,int)`，参数类型列表也分不开。
2. **用户定义的运算符与转换**——它们根本没有名字。
3. **显式接口实现**——同名不同 owner。
4. **私有成员**——元数据可见但重载决议不可见。

**必须留逃生口。** Swift 的 `@derivative(of:)` 没留，用户撞上只能报 ambiguous、无解
（编译器测试 `derivative_attr_type_checking.swift` 里没有任何 `as` 类型标注用法）。
本提案的逃生口：诊断中列出该名字下**全部可用重载**，并允许后续补一个规范化字符串形式兜底。

## 诊断（码待分配，E04xx 段）

| 场景 | 级别 | 要求 |
|---|---|---|
| `methodof` 目标方法不存在 | Error | **必须列出该名字下全部可用重载** |
| 参数类型列表匹配到多个 | Error | 列出全部候选 + 说明为何分不开 |
| 未给参数列表且存在重载 | Error | 提示补参数类型列表消歧 |
| 目标是运算符/转换/显式接口实现 | Error | 明确说明「无法指代」而非含糊报找不到 |

## 前置验证项（写代码前必须验，不猜）

| # | 验什么 | 为什么关键 |
|---|---|---|
| **0** | **`[Foo(typeof(Bar))]` 端到端能否工作** | 330 个 `typeof` 用例**没有一个在 attribute 实参位置**；现有 attribute 实参全是字面量。`methodof` 会是第一个在该位置放非平凡表达式的特性。最可能的坑是**作用域**——工厂函数被合成为**顶层 static 自由函数**（`_synthFactory` 传 `new Param[0], 0, true, body`），而 attribute 可能写在类内部并引用该类可见的类型。**typeof 不通则 methodof 更不通，须先修工厂路径** |
| 1 | `TypeNameResolver.SurfaceTypeName` 对泛型参数 `T` 能否正确拼回 | 参数类型匹配依赖它；已知它输出 TSIG 规范名（`byte[]`→`u8[]`），泛型参数路径未验 |
| 2 | `typeof` 的 emit 具体走哪条 runtime 路径 | `methodof` 要照抄；决定新 builtin 的形状 |
| 3 | 模块级驻留缓存放在哪层 | 反射侧现在**零缓存**（`invoke.rs:169-200` 每次查 HashMap），`FieldIC`/`VCallIC` 在 `corelib/` 零命中，没有现成 IC 可复用 |

## 自举纪律

`methodof` 是新语法 → 按 [bootstrap-seed.md](../../../.claude/rules/bootstrap-seed.md)
**support 先行、use 晚一个 nightly**：本提案只落「z42c 支持 `methodof`」，z42c 自身源码 / stdlib /
xtask **不得使用** `methodof`，等含本变更的 nightly 发布后才能用。落地后跑
`xtask test bootstrap` 确认无越界。

## 验证

- `xtask test` 全绿（interp）
- `xtask test stdlib --mode jit` 补跑（新增反射路径需 jit 双验）
- 自举字节不动点 gen1 == gen2
- `xtask test bootstrap` 无语法越界
