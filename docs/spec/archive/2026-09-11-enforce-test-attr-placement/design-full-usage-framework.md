# （存档 / Deferred）通用 Attribute Usage 框架

> ⛔ **本文档不是当前要实施的方案。** 2026-09-11 User 裁决：范围收窄回初衷——
> **只让 `[Test]` 家族在编译期被约束住**，不建通用框架。当前方案见 **[design.md](design.md)**。
>
> 本文存档完整的通用 usage 框架设计（`Target`/`Require` 词汇、`[Usage]` directive、谓词体系、
> 跨包取舍、编译期求值三档调研）。**触发条件**：出现**第二个**真实需要位置约束的 attribute 家族，
> 或用户 attribute 的误用成为实际问题时，回来取用。
>
> 其中这几节即便在最小方案下也仍然有效，已被 design.md 引用：
> §1.2（丢失的 E0911–E0917）、§1.4（全仓审计 + `with-tidx` fixture 实证）、
> §5.1（BenchmarkDesugar 相位约束）、§8（编译期求值调研）。

---

# Design: Attribute Usage —— attribute 可贴位置 / 多重性 / 伴随约束

> **状态**：设计定稿，未实施。
> **上游已决策**：[attributes.md](../../../design/language/attributes.md) Deferred `attribute-future-attributeusage`
> （一等声明、非 C# 元属性自循环、无隐式继承）+
> [attribute-handler-registry design §位置限制](../attribute-handler-registry/design.md)（`[Targets]` 雏形）。
> 本设计是二者的落地版，并补上它们未覆盖的三处：**静态/实例轴**、**内建 attribute（无 backing class）**、
> **伴随约束**。
> **配套独立变更**：§9 `add-comptime-eval`（可选，不阻塞本变更）。

---

## 0. TL;DR

| | |
|---|---|
| **修什么** | `[Test]` 贴实例方法编译期全绿、运行期报无关的 arity 错；`[Record]` 贴方法、`[Skip]` 无 `[Test]`、attribute 重复贴——全部静默通过 |
| **顺带修** | E0911–E0917 六个测试校验码在自举迁移中**整套丢失**，文档却仍写"已启用" |
| **机制** | **四层**声明式约束（Targets / **Signature** / Multiplicity / Companion）+ 一个纯语法强制 pass |
| **两个真相源** | 内建 → `HandlerRegistry` 表（无语法）；用户 → 类头 `[Usage(...)]` **directive** |
| **关键取舍** | Target **区分静态/实例**（C#/Java/Kotlin 都不分，而 z42 的执行模型要求分）；`[Usage]` 是 directive 而非 Attribute 子类（**消除 C# 的元属性自循环**） |
| **语法改动** | **零**（`[Usage(...)]` 的三种参数形态 parser 已全支持） |
| **格式改动** | **零 —— usage 完全不写进 zpkg**（纯编译期，同 `[Suppress]`）。代价：跨包用户 attribute 不校验（§7.1）|
| **存量破坏** | **一处**：`src/tests/zbc-format/with-tidx/` fixture 需改 `static` 并重冻结 |
| **分期** | P1 内建表（修 `[Test]` **+ 签名约束**）→ P2 实参语义检查 → P3 `[Usage]` 用户自声明 |
| **怎么用** | 用户视角的完整示例见 **[附录 A](#附录-a-用法示例用户视角)**（含 A.0 速查表、A.9「能不能调 severity」、A.10 升级影响）|

---

## 1. 问题

### 1.1 直接触发：`[Test]` 贴实例方法，编译期全绿、运行期炸

```
① MemberParser.z42:98          对任何成员的前置 `[X]` 一律包成 AttributedDecl —— 不看 X 是谁、不看被贴的是什么
② TestIndexBuilder.z42:48      扫类成员只判 `md.HasBody && _hasTestAttr(ad)` —— 不判 static，照写 TIDX entry
③ Runner.z42:6-8               不变量（只写在注释里）：「emitted as zero-arg FREE functions … invoked by
                               their TIDX fully-qualified name — not via instance + MethodInfo」
④ invoke.rs:234                __invoke_static → invoke_arity_check(qualified, f.param_count, 0)
                               实例方法 param_count 含 receiver → 1 ≠ 0
⑤ 运行期报错                    MethodInfo.Invoke: `X` expects 1 argument(s) (incl. receiver), got 0
```

Runner 依赖的「零接收者」不变量**没有任何一层强制**，违规成本推迟到运行期，且报错信息完全指不到病灶。

> **同类事故已发生过一次**：[BenchmarkDesugar.z42](../../../../src/compiler/z42c.semantics/src/BenchmarkDesugar.z42)
> 文件头记着——`[Benchmark] void f(Bencher b)` 的脱糖 pass 随 C# 编译器一起被删（f8ff73d5）、**没有移植到 z42c**，
> 于是 form-2 benchmark 从那时起一直在运行期挂，报的是**一模一样**的
> `MethodInfo.Invoke: expects 1 argument, got 0`，直到 2026-07-20 才补回。
> **同一个失败模式、同一个根因族（编译期不校验 → 运行期 arity）、两年内第二次。**

### 1.2 更大的洞：测试 attribute 的整套校验在自举迁移中丢了

[error-codes.md:139](../../../design/compiler/error-codes.md#L139) 至今写着
「E0911/E0912/E0914/E0915（R4.A **已启用**，2026-04-30）」，实施位置指向
`src/compiler/z42.Semantics/TestAttributeValidator.cs` —— 属**已退休的 C# 编译器**。

```
$ grep -rn "E0911|E0915|TestSignatureInvalid" --include=*.z42 src | grep -v DiagnosticCodes.z42
（无输出）
```

六个码（E0911/0912/0913/0914/0915/0917）在
[DiagnosticCodes.z42](../../../../src/libraries/z42c.core/src/DiagnosticCodes.z42) 定义齐全、**零引用**。
`[Test] int f(int x)`、`[Skip]` 无 reason、`[Setup]` 与 `[Test]` 同贴、`[Test][Benchmark]` 互斥违规——全部静默通过。
**文档与实现在这一块对不上。**

### 1.3 系统性缺口：没有任何 attribute 声明过"我能贴在哪"

| 今天写得出来 | 今天的结果 |
|---|---|
| `[Test] class Foo { }` | 通过 —— TestIndexBuilder 只扫方法，**静默无视** |
| `[Record] void f() { }` | 通过 —— ClassDescBuilder 只在 class 上读 bit3，**静默无视** |
| `[Test] [Test] void f()` | 通过 —— TIDX 写一条，**静默吞掉** |
| `[Skip(reason:"x")] void f()`（无 `[Test]`）| 通过 —— TIDX 凭空多一个 Kind=Test 的 skipped entry |
| `[Deprecated] int p` （参数位）| 通过 —— 哨兵只在类/方法/字段路径读，**静默无视** |
| `[Doc("x")] using Std;` | 通过 —— 无人消费 |

共性：**「这个 attribute 贴在这里有没有意义」这项知识，编译器里根本没有落点。**
[HandlerRegistry](../../../../src/compiler/z42c.semantics/src/HandlerRegistry.z42) 已是「attribute 名 → kind」
的唯一真相源，但它只答"是哪一路"，不答"能贴哪"。

### 1.4 全仓审计实测（2026-09-10，脚本见 [tasks.md](tasks.md) T0）

```
Benchmark    free-fn                 66      Record       class                   52
Ignore       free-fn                  1      Record       struct                  26
Native       instance-method         59      Setup        free-fn                  1
Native       property                27  ←①  ShouldThrow  free-fn                  7
Native       static-method          262      Skip         free-fn                 13
Test         free-fn               3807      Teardown     free-fn                  1
Test         instance-method          2  ←②  Timeout      free-fn                  1
（Deprecated / Suppress：src/** 下 0 处应用）
```

**① `[Native]` 有 27 处贴在属性上**（`[Native("__type_get_type")] public extern string FullName { get; }`，
[Type.z42:60](../../../../src/libraries/z42.core/src/Type.z42#L60)）→ `Native` 的 Targets **必须含 `Property`**。

**② `[Test]` 有 2 处真实存量违规**，且**冻结产物把 bug 固化了** ——
[src/tests/zbc-format/with-tidx/source.z42](../../../../src/tests/zbc-format/with-tidx/source.z42)：

```z42
class MathTests {
    [Test]  void test_add() { Assert.Equal(3, 1 + 2); }   // ← 无 static
    [Test]  void test_sub() { Assert.Equal(5, 10 - 5); }
}
```
```json
{ "name": "MathTests.test_add", "param_count": 1, "param_types": ["MathTests"], "is_static": false },
"has_test_index": true, "test_index_size": 2
```

编译器**确实给两个实例方法写了 TIDX entry**。这个 fixture 是字节级 golden、**从不执行**，所以没人发现——
一跑就撞 §1.1 的 arity 错。**这是 §1.1 推理链的直接实证，也是本变更唯一的存量改动点。**

---

## 2. 模型

### 2.1 四层声明式 + 一层外包

| # | 层 | 谁强制 | 声明式 |
|---|---|---|---|
| 1 | **Targets**（贴在哪：类/自由函数/static 方法/实例方法/字段/…） | 编译器 `_passAttributeUsageEnforce` | ✅ |
| 2 | **Signature**（签名形状：`void` 返回 / 0 参 / 非泛型 / 有 body） | 同上 | ✅ |
| 3 | **Multiplicity**（能贴几次） | 同上 | ✅ |
| 4 | **Companion**（必须/不能与谁同贴） | 同上 | ✅ |
| 5 | **深层语义**（实参值域、类型继承链、跨包一致性…） | 专用检查（E0913/0914/0917）或 `Analyzer` 契约 | ❌ |

**第 2 层（签名）为什么进声明**（本设计相对早期草案的修正）：`[Test]` 的真实约束是
**「零接收者 + 无返回值 + 无参数 + 非泛型 + 有体」**——把它拆成"位置声明式、签名硬编码"，
等于把同一条规则劈成两半放在两个地方，正是 §1.2 那套校验丢了都没人发现的土壤。
**一个 attribute 的应用约束应当在一处可读完。**

而且四条签名谓词**全是纯语法的**，进声明不降级 pass：

| 谓词 | 判定式 | 依据 |
|---|---|---|
| `VoidReturn` | `md.RetType is NamedType && (md.RetType as NamedType).Name == "void"` | [MemberParser.z42:372](../../../../src/libraries/z42c.syntax/src/MemberParser.z42#L372) 已有同款判定 |
| `NoParams` | `md.ParamCount == 0` | 既有字段 |
| `NotGeneric` | `md.TypeParams.Count == 0` | [TypeParamList](../../../../src/libraries/z42c.syntax/src/TypeExpr.z42#L71) |
| `HasBody` | `md.HasBody` | 既有字段 |

**第 5 层仍然外包**：实参值域（`[Skip]` 的 reason 非空）、类型继承链（`[ShouldThrow<E>]` 的 E 须继承
`Exception`——**需要符号表**）、跨包一致性，这些不是语法谓词，做进声明会长成迷你 DSL。内建的走专用检查
（E0913/E0914/E0917，P2）；用户的交
[`Analyzer` 契约](../attribute-handler-registry/design.md)（可外部加载、`[lints]` 可调 severity、`#suppress` 可局部关）。

### 2.2 Target 分类学：**静态与实例分家**

上游雏形是 `Class/Struct/Interface/Enum/Method/Field/Param`，其中 **`Method` 不分静态/实例**——抄的是
C# `AttributeTargets.Method`。而 §1.1 的坑**正是这个自由度**。

```
位  成员            命中的 AST 形态
 1  Class           ClassDecl.Kind == "class"
 2  Struct          ClassDecl.Kind == "struct"
 4  Interface       ClassDecl.Kind == "interface"
 8  Enum            EnumDecl
16  Delegate        DelegateDecl
32  Function        MethodDecl.IsFree                     （顶层自由函数，无接收者）
64  StaticMethod    MethodDecl + _hasWord(Mods,"static")  （无接收者）
128 Method          MethodDecl 其余                        （实例方法，有接收者）
256 Ctor            MethodDecl.IsCtor
512 Field           FieldDecl 其余
1024 StaticField    FieldDecl + static/const
2048 Property       PropertyDecl / IndexerDecl（不分静态，见下）
4096 Param          Param.Attrs

别名  Type         = Class|Struct|Interface|Enum|Delegate     = 31
      ZeroReceiver = Function|StaticMethod                    = 96     ← test 家族全用这个
      AnyMethod    = Function|StaticMethod|Method             = 224
      AnyField     = Field|StaticField                        = 1536
      Any          = 全部                                      = 8191
```

**为什么方法分静态/实例、而属性不分**：分家的理由是**执行模型**——`[Test]` 走 `__invoke_static` 的零接收者
FQN 调用路径，实例方法必崩。属性没有对应的零接收者调用路径，没有任何已知消费者需要这个区分
（审计里 `[Native]` 静态/实例属性都有），所以**不分**。原则是「**只在有真实失败模式的轴上分**」，不为对称而对称。

> **z42 会是第一个这么做的**：C# `AttributeTargets.Method`、Java `ElementType.METHOD`、Kotlin
> `AnnotationTarget.FUNCTION` **一律不分**静态/实例。在它们那里无害（C# 测试框架用实例 + 反射构造），
> 在 z42 **有害**。这是顺着 z42 自己的执行模型来的，不是抄谁；同 D8 后缀强制（把 C# 的软约定提成硬规则），
> z42 一贯用「消除自由度」换确定性。

### 2.2b Require 分类学：签名形状的第二个闭集

与 `Target` **同一套机制**（变长位置参 + 按名匹配 `MemberExpr{IdentExpr(ns), Name}`），只是限定符换成
`Require`，故 parser / pass 一行都不用额外改：

```
位  成员           判定                                   不满足时
 1  VoidReturn     RetType 是 NamedType("void")           E0457
 2  NoParams       ParamCount == 0                        E0457
 4  NotGeneric     TypeParams.Count == 0                  E0457
 8  HasBody        HasBody == true                        E0457
```

**不设别名**（不提供 `Require.TestShape` 之类的打包名）——四条全部拼出来，因为本设计的目的就是
**让约束在声明处可读完**，别名会把它再藏回去一层。

于是 `[Test]` 的完整约束就是一行、可读完：

```
Target.ZeroReceiver  +  Require.VoidReturn, Require.NoParams, Require.NotGeneric, Require.HasBody
     ↑ 自由函数或 static 方法        ↑ 无返回值   ↑ 无参数      ↑ 非泛型         ↑ 有体
```

> **谓词与 Target 的适用域必须相容**：贴在类上的 attribute 声明 `Require.VoidReturn` 恒不满足 →
> 在 `[Usage]` 解析期即报 E0455（矛盾组合），不等到用点才报。内建表里不会出现这种组合。

### 2.2c 谓词体系：两个轴（适用域 × 相位）+ 一条准入标准

上面四条只是"可调用"这一族。要**系统地**支持更多——尤其是**"只能贴在具有某些特征的类上"**——
不能继续往清单里堆，得先把体系钉住。

#### 轴一：适用域（谓词只对某类 Target 有意义）

| 域 | 谓词 | 判定 | 相位 |
|---|---|---|---|
| **可调用**<br/>`Function`/`StaticMethod`/`Method`/`Ctor` | `VoidReturn` | `RetType` 是 `NamedType("void")` | 语法 |
| | `NoParams` | `ParamCount == 0` | 语法 |
| | `NotGeneric` | `TypeParams.Count == 0` | 语法 |
| | `HasBody` | `HasBody` | 语法 |
| **类型**<br/>`Class`/`Struct`/`Interface`/`Enum` | `Sealed` | `_hasWord(Mods,"sealed")` | 语法 |
| | `NotAbstract` | `!_hasWord(Mods,"abstract")` | 语法 |
| | `NotGeneric` | `TypeParams.Count == 0` | 语法 |
| | `Instantiable` | 无显式 ctor，或存在 `public` 无参 ctor | 语法 |
| | `NoInstanceState` | 无非 static/const 字段 | 语法 |
| | `AllFieldsPublic` | 每个实例字段都带 `public` | 语法 |
| **类型（带参）** | `implements: typeof(T)` | 传递实现接口 `T` | **语义** |
| | `derivesFrom: typeof(T)` | 传递继承自 `T` | **语义** |
| **字段/属性** | `Readonly` / `Public` | `_hasWord(Mods, …)` | 语法 |

`NotGeneric` 跨两个域（方法和类型都能泛型），判定式不同但语义一致——**同名复用，按 Target 分派判定**。

#### 轴二：相位（决定它落在哪个 pass）

```
语法相谓词  →  _passAttributeUsageEnforce      与 E0444/E0445/E0447 同级、first-pass、不依赖符号表
语义相谓词  →  _passAttributeUsageSemantic     post-collect，与 E0913 同相位（§5.5）
```

**为什么必须分两个 pass 而不是把整个 pass 后移**：位置和签名违规要**尽早**报——它们不依赖任何解析结果，
后移只会让用户在一堆下游的连锁错误里找病灶。而 `implements:` 这类要等继承链建好。
分相位是代价最小的做法：语法相那条路径保持零依赖，语义相只处理声明了带参谓词的那少数 attribute。

#### 无参谓词走位置参，带参谓词走命名参

```z42
[Usage(Target.Class, Require.Sealed, Require.Instantiable,      // ← 无参：位置参，闭集按名匹配
       implements: typeof(IHandler),                            // ← 带参：命名参
       allowMultiple: false)]
public class HandlerAttribute : Attribute { }
```

这不是两套机制——`Attr.Args` 里 `AssignExpr` 归命名槽、其余归位置参，
[DeclParser.z42:56-70](../../../../src/libraries/z42c.syntax/src/DeclParser.z42#L56) 本来就这么分
（§4.2）。`typeof(T)` 也是既有的合法 attribute 实参形态
（[attributes.md](../../../design/language/attributes.md)：*「attribute 参数限编译期常量（字面量 / enum 成员 / `typeof`）」*）。

#### 准入标准：什么谓词可以进闭集

堆谓词是这类设计最容易失控的地方（会长成迷你 DSL）。**四条全满足才进**：

1. **纯声明性** —— 判定不需要执行用户代码（否则该走 `Analyzer`）。
2. **判定可在编译器内一两行写完** —— 复杂到要写算法的，说明它是语义分析、不是"用法约束"。
3. **有真实需求** —— 至少一个内建 attribute 或一个已知用例要用它；不为对称补全。
4. **违规后果明确** —— 说得清"贴错了会怎样"。`[Test]` 贴实例方法 → 运行期 arity 崩，明确；
   "attribute 只该贴在名字以 Handler 结尾的类上"→ 说不清，不收。

**兜不住的一律交 `Analyzer`**（§2.1 第 5 层）——它能跑任意判定、能跨包、severity 可调、可 `#suppress`。
上游 design 的原话就是这条分工：*「richer 约束靠 handler 自校验……位置声明式、语义自校验」*。
本设计把其中**满足上述四条**的那部分收回声明式，其余不动。

#### 扩展成本：加一个谓词 = 三处

```
① Target/Require 常量类加一位             （HandlerRegistry.z42）
② 判定分支加一个 if                        （DeclEnforcer 的 _checkRequires / _checkTypeRequires）
③ Std.Require enum 加一个成员 + 文档一行    （z42.core，给人和 IDE 看）
```

**不改 pass 结构、不改 parser、不改诊断码**（全部复用 E0457）。这就是"系统支持"的含义：
新增能力是填表，不是改机制。



### 2.3 三条语义决策

1. **缺省不校验**。未登记（非内建 + 无 `[Usage]`）→ `Targets = 0` → **跳过**。
   `[Usage]` 是 opt-in 的加严，**绝不制造存量破坏**；未知 attribute 名的诊断是另案（§12）。
2. **无隐式继承**。`class AdminRouteAttribute : RouteAttribute` **不**继承父类的 usage；
   未自己声明 `[Usage]` 就是"未登记"。（这条是 §8.4 否决 in-body 形态的根据。）
3. **不可 `#suppress`**。E04xx 是编译器硬错，`SuppressionSet` 只过滤 `AnalyzerDriver.DiagSinkImpl.Report`
   里的 analyzer 诊断，不经手 E 码。要放宽只能改 `[Usage]` 声明本身。

---

## 3. 真相源 A：内建 attribute（`HandlerRegistry` 表）

上游的 `[Targets]` 只解决了一半——它挂在**用户类**上，而 `[Test]`/`[Native]`/`[Record]`/`[Suppress]`/
`[Deprecated]` **全是靠名字识别的内建、没有类可挂**。

### 3.1 结构

```z42
// HandlerRegistry.z42 —— 与既有 AttrKind 同惯例（自举子集不用 enum、不用数组字面量）
public static class Target {
    public static int Class = 1;          public static int Struct = 2;
    public static int Interface = 4;      public static int Enum = 8;
    public static int Delegate = 16;      public static int Function = 32;
    public static int StaticMethod = 64;  public static int Method = 128;
    public static int Ctor = 256;         public static int Field = 512;
    public static int StaticField = 1024; public static int Property = 2048;
    public static int Param = 4096;
    public static int Type = 31;  public static int ZeroReceiver = 96;
    public static int AnyMethod = 224;  public static int AnyField = 1536;
    public static int Any = 8191;
}

public static class Require {                 // 声明形状谓词（§2.2b/§2.2c），无别名
    // 可调用域
    public static int VoidReturn = 1;   public static int NoParams = 2;
    public static int NotGeneric = 4;   public static int HasBody  = 8;
    // 类型域（§2.2c 轴一）
    public static int Sealed = 16;      public static int NotAbstract = 32;
    public static int Instantiable = 64; public static int NoInstanceState = 128;
    public static int AllFieldsPublic = 256;
    // 字段/属性域
    public static int Readonly = 512;   public static int Public = 1024;
}

public sealed class AttrUsage {
    public int    Targets;         // Target 位或；0 = 未登记 → 整条不校验
    public int    Requires;        // Require 位或；0 = 无签名约束
    public bool   AllowMultiple;
    public string Needs;           // 逗号分隔的**用名**；"" = 无（同一声明上须至少有其一）
    public string Excludes;        // 逗号分隔的用名；"" = 无
    public string ImplementsType;  // 带参谓词（语义相，§5.5）；"" = 无
    public string DerivesFromType; // 同上
    public static AttrUsage Of(int t, int req, bool multi, string needs, string exc) { ... }
    // 带参谓词经 WithImplements(...) / WithDerivesFrom(...) 链式追加（内建表用不到，用户 [Usage] 用）
    public static AttrUsage None;  // Targets = 0
}

public static AttrUsage UsageOf(string name) { ... }   // 内建表；未命中 → AttrUsage.None
```

> 字段名用 `Needs` 而非 `Requires`，避免与签名位 `Requires` 撞名——**伴随约束**（要跟谁一起贴）
> 与**签名约束**（自己长什么样）是两件事，名字必须分开，否则读代码的人一定会串。

**刻意用 if 链而非表驱动**，与既有 `IsDirectiveAttr` / `IsTestHandlerAttr` 完全同形（那里也是一串 `name ==`）；
`Needs`/`Excludes` 用**逗号串**而非 `string[]`，省掉数组在自举期的麻烦。

```z42
public static AttrUsage UsageOf(string name) {
    if (name == "Test") {
        return AttrUsage.Of(Target.ZeroReceiver,
                            Require.VoidReturn | Require.NoParams | Require.NotGeneric | Require.HasBody,
                            false, "", "Benchmark,Setup,Teardown");
    }
    if (name == "Skip") {
        return AttrUsage.Of(Target.ZeroReceiver, 0, false, "Test,Benchmark", "");
    }
    if (name == "Native") {
        return AttrUsage.Of(Target.StaticMethod | Target.Method | Target.Ctor | Target.Property,
                            0, false, "", "");
    }
    if (name == "Record") { return AttrUsage.Of(Target.Class | Target.Struct, 0, false, "", ""); }
    // …
    return AttrUsage.None;
}
```

**这就是 §1.2 那套丢失校验的落点**——原先散在已删除的 `TestAttributeValidator.cs` 里的
"`[Test]` 必须 `fn() -> void`、不能泛型、与 `[Benchmark]` 互斥"，现在是**一个 `AttrUsage.Of` 调用**，
和位置规则在同一行里，删不掉也漏不掉。

### 3.2 完整表值（审计定稿，§1.4）

`V`=VoidReturn `P`=NoParams `G`=NotGeneric `B`=HasBody

| 用名 | kind | Targets | Require | Multi | Needs | Excludes |
|---|---|---|---|---|---|---|
| `Test` | handler | `ZeroReceiver` | `V P G B` | ✗ | — | `Benchmark,Setup,Teardown` |
| `Benchmark` | handler | `ZeroReceiver` | `V P G B` ¹ | ✗ | — | `Test,Setup,Teardown` |
| `Setup` | handler | `ZeroReceiver` | `V P G B` | ✗ | — | `Test,Benchmark,Skip,Ignore` |
| `Teardown` | handler | `ZeroReceiver` | `V P G B` | ✗ | — | `Test,Benchmark,Skip,Ignore` |
| `Skip` | handler | `ZeroReceiver` | — ² | ✗ | `Test,Benchmark` | — |
| `Ignore` | handler | `ZeroReceiver` | — ² | ✗ | `Test,Benchmark` | — |
| `ShouldThrow` | handler | `ZeroReceiver` | — ² | ✗ | `Test,Benchmark` | — |
| `Timeout` | handler | `ZeroReceiver` | — ² | ✗ | `Test,Benchmark` | — |
| `Native` | directive | `StaticMethod\|Method\|Ctor\|Property` | — ³ | ✗ | — | — |
| `Record` | directive | `Class\|Struct` | — | ✗ | — | — |
| `Suppress` | directive | `Any` | — | ✓ | — | — |
| `Deprecated` | directive | `Any & ~Param` | — | ✗ | — | — |
| `Usage` | directive | `Class`（+ 基类须是 `Attribute`，§4.5） | — | ✗ | — | — |

**¹ `[Benchmark]` 的 `NoParams` 是 post-desugar 的**：form-2 `[Benchmark] void f(Bencher b)` **合法**，
[`BenchmarkDesugar`](../../../../src/compiler/z42c.semantics/src/BenchmarkDesugar.z42) 会把它改写成
`f$impl(Bencher b)`（attribute 剥掉）+ 零参 wrapper `[Benchmark] void f()`。本 pass 跑在 desugar **之后**
（§5.1 相位约束），看到的一律是零参 wrapper。**这条是整个设计里最容易踩的雷**，代码注释和相位回归
golden 都必须钉住。

**² 修饰类 attribute 不设 `Require`**：它们的签名约束由所修饰的 `[Test]`/`[Benchmark]` 承担
（`Needs` 已保证它们必须与之同贴），重复声明只会在两处维护同一条规则。

**³ `[Native]` 的"必须 extern / 无 body"由既有 E0903/E0904 管**（extern↔native 双向配对），
不在本表重复；这里只管**位置**。

> **`Property` 那一格是审计挣来的**：提案表原本漏了，实测 27 处 `[Native]` 贴在 `extern` 属性上。
> 表里每一行改动前都必须重跑 T0——**填错一行 = stdlib 编译崩**。

### 3.3 先例：rustc 就是这么做的

Rust 的内建 attribute 校验用的正是**编译器内表**：`BUILTIN_ATTRIBUTE_MAP` 给每个内建 attribute 配
`AttributeTemplate`（允许的语法形状）+ 允许出现的位置，非内建的 proc-macro attribute 则**完全不声明位置**。
z42 的 §3.1 表 = rustc 的内建表，§4 的 `[Usage]` = 给用户 attribute 补上 rustc 没有的那一半。

---

## 4. 真相源 B：用户 attribute（类头 `[Usage(...)]` directive）

### 4.1 拼写

```z42
// 只能贴类和 static 方法，同一处只能贴一次
[Usage(Target.Class, Target.StaticMethod, allowMultiple: false)]
public class RouteAttribute : Attribute { ... }

// 「只能贴无返回值、无参数的静态函数」—— 与内建 [Test] 完全同款约束，用户可自己写出来
[Usage(Target.ZeroReceiver,
       Require.VoidReturn, Require.NoParams, Require.NotGeneric, Require.HasBody,
       allowMultiple: false)]
public class SmokeTestAttribute : Attribute { }
```

```z42
// 「只能贴在 sealed、可实例化、且实现了 IHandler 的类上」—— 类特征约束（§2.2c）
[Usage(Target.Class, Require.Sealed, Require.Instantiable,
       implements: typeof(IHandler))]
public class HandlerAttribute : Attribute { }
```

**两个闭集混在同一个位置参列表里**，靠限定符区分（`Target.X` vs `Require.X`）——不需要分成两个 attribute、
不需要额外命名参，parser 侧也无差别（都是 `MemberExpr`）。**带参谓词走命名参**（`implements:`），
因为它要带一个 `typeof(T)`。

### 4.2 parser 兼容性：**零改动**（已实证）

| 形态 | 写法 | parser 已产出的节点 |
|---|---|---|
| 位置参 = target | `Target.StaticMethod` | `MemberExpr{ IdentExpr("Target"), "StaticMethod" }` |
| 命名参 = 标量 | `allowMultiple: false` | `AssignExpr("=", IdentExpr("allowMultiple"), <lit>)` |
| 命名参 = 名单 | `excludes: ["Setup"]` | `AssignExpr("=", IdentExpr("excludes"), ArrayLitExpr)` |

依据 [DeclParser.z42:56-70](../../../../src/libraries/z42c.syntax/src/DeclParser.z42#L56)：命名参 `name: value`
明确译成 `AssignExpr("=", IdentExpr(name), value)`（注释原话*「镜像 C# 属性命名参数」*），其余位置走
`_parseExpr(0)` 收**任意表达式**；`["Setup"]` 是已落地的
[`ArrayLitExpr`](../../../../src/libraries/z42c.syntax/src/Ast.z42#L51)。

读法 = 一个 while 走 `Attr.Args`：`is AssignExpr` → 按 `IdentExpr.Name` 分派命名槽；否则当 target 位置参。
与 [`HandlerRegistry.DeprecatedMsg`](../../../../src/compiler/z42c.semantics/src/HandlerRegistry.z42#L105)
读 `[Deprecated("msg")]` 位置参同形。

### 4.3 `Target.*` / `Require.*` 成员：**按名匹配，不做常量求值**

匹配 `MemberExpr.Target is IdentExpr`，看限定符：

| 限定符 | 查的闭集 | 不在闭集 |
|---|---|---|
| `Target` / `Std.Target` | §2.2 的 13 成员 + 5 别名 | E0455 |
| `Require` / `Std.Require` | §2.2b 的 4 成员 | E0455 |
| 其他 | —— | E0455（消息提示只接受 `Target.*` / `Require.*`）|

`Std.Target` / `Std.Require` 两个 enum **仍在 z42.core 定义**（位值与 §3.1 内部常量一一对应）——
不是给编译器求值的，是给**人和 IDE** 的：补全、跳转、可反射。**强制靠名字、可读性靠 enum。**
比 `[Native("__array_get")]` 的裸字符串 magic 强一格：拼错立刻 E0455，而非静默失效。

**矛盾组合就地报错**：声明了 `Require.*` 却没有任何可调用 `Target`（`Function`/`StaticMethod`/`Method`/`Ctor`）
→ 该约束恒不满足 → 在 `[Usage]` 解析期即报 E0455，不等到用点（§2.2b 注）。

> **不写 `Target.A | Target.B`**：位或要在 AST 级 pass 里对 `BinaryExpr` 做常量折叠。本 pass 与
> E0444/E0445/E0447 同级、纯语法、first-pass 可用，引常量折叠是净负债；逗号已足够表达"或"。
> （注：既有 [`ConstEval._evalBinary`](../../../../src/compiler/z42c.semantics/src/ConstEval.z42#L78)
> **确实支持 `& | ^`**，所以这不是"做不到"，是"不值得为此把本 pass 从纯语法降级"。见 §8.4。）

### 4.4 `needs` / `excludes`：用**剥后缀的用名字符串**

不用 `typeof(BenchmarkAttribute)`，因为**内建 attribute 没有类可以 typeof**。要让一套词汇同时覆盖内建与
用户两侧，只能落在用名上。代价是 stringly-typed；缓解是只接受内建名或存在 `<Name>Attribute` 类的名字，
否则 **E0456**。

> 🔸 **v1 决定：用户侧开 `Target.*` + `Require.*` + `allowMultiple`，不开 `needs`/`excludes`。**
> 伴随约束唯一真实的用户是内建 test 家族（`[Skip]` 须配 `[Test]`、`[Test]` 排斥 `[Benchmark]`），
> 它活在 §3.1 表里、不需要语法。用户侧的伴随约束交 `Analyzer`（§2.1 第 4 层），等真有需求再开。
> 这样 stringly-typed 的坑一开始就不用挖，`[Usage]` 表面积小一半，**E0456 随之推迟**。

### 4.5 `[Usage]` 自身的 usage —— 自举在这里收口

```
Usage → Targets = Class，且被贴类的直接基类须是 Attribute，AllowMultiple = false
```

**硬编码在 `_passAttributeUsageEnforce` 的入口**，不进 `UsageOf` 表——表是给"被校验者"的，
`[Usage]` 是"校验者"，两者不同层。贴到非 `: Attribute` 类 → E0452；贴两次 → E0453。

### 4.6 为什么是 directive 而非 `: Attribute` 类

这一条就是本设计对 C# 的全部改进：

- C# 的 `AttributeUsageAttribute` **自己是 attribute 类**，且身上贴着 `[AttributeUsage(AttributeTargets.Class)]`
  —— **自循环**，编译器必须为这一个类型开自举后门。这正是
  [attributes.md §对 C# 的改进 #5](../../../design/language/attributes.md) 记的那条缺陷。
- z42 归它为 **directive**（`HandlerRegistry.IsUsageDirective`，同 `[Native]`/`[Suppress]`/`[Record]`/
  `[Deprecated]`）：靠名字识别、**无 backing 类**、`KindOf → Directive` 故 `AttributeSynth` **不合成反射工厂、
  不写 store-meta blob**、**无需 `UsageAttribute` 类**。**自循环不是绕过去了，是压根不存在。**

---

## 5. 强制 pass：完整算法

### 5.1 挂载点与相位

`DeclEnforcer._passAttributeUsageEnforce(CompilationUnit cu)`，挂在既有三个后缀 pass 的紧邻位——
[SymbolCollector.z42:60-62](../../../../src/compiler/z42c.semantics/src/SymbolCollector.z42#L60)、`:173-177`、`:198-200`
共 **3 个挂载点**。

**相位约束（关键）**：必须在 `HandlerRegistry.RunAst` **之后**。RunAst 里的
[`BenchmarkDesugar`](../../../../src/compiler/z42c.semantics/src/BenchmarkDesugar.z42) 会把合法的
`[Benchmark] void f(Bencher b)`（form-2）改写成 `f$impl(Bencher b)` + **零参 wrapper `[Benchmark] void f()`**。
在 desugar **之前**校验会把 form-2 误判为违规。SymbolCollector 天然在 RunAst 之后（IncrementalDriver:52 先
RunAst，再进 collect），**挂载点已满足该序约束**——但这条必须写进代码注释，否则将来有人上移 pass 就踩雷。

### 5.2 遍历（伪码）

```
_passAttributeUsageEnforce(cu):
    for d in cu.Decls:
        _checkDecl(d, /*nested=*/false)

_checkDecl(d, nested):
    if d is AttributedDecl:
        _checkAttrList(d.Attrs, d.AttrCount, _targetOf(d.Inner, nested), d.Inner)
        inner = d.Inner
    else:
        inner = d
    if inner is ClassDecl:                     # 递归成员 + 嵌套类型
        for m in inner.Members: _checkDecl(m, true)
    if inner is ImplDecl:                      # impl 块内方法（port-z42c-impl-block）
        for m in inner.Methods: _checkDecl(m, true)
    if inner is MethodDecl or DelegateDecl:    # 参数位 attribute
        for p in inner.Params:
            _checkAttrList(p.Attrs, p.AttrCount, Target.Param, p)

_checkAttrList(attrs, n, tgt, decl):
    for i in 0..n:
        u = _usageOf(attrs[i].Name)            # 内建表 → 本 CU 的 [Usage]（跨包不查，§7）
        if u.Targets == 0: continue            # 未登记 → 跳过（§2.3 ①）

        # 层 1：位置
        if (u.Targets & tgt) == 0:
            报 E0452(attrs[i], tgt, u.Targets); continue      # 位置就错了，后续检查无意义

        # 层 2：签名（仅对可调用声明；decl 是 MethodDecl 时才有意义）
        if u.Requires != 0 and decl is MethodDecl:
            if (u.Requires & Require.VoidReturn) != 0 and !_isVoid(decl.RetType):
                报 E0457(attrs[i], "void return", decl)
            if (u.Requires & Require.NoParams)   != 0 and decl.ParamCount != 0:
                报 E0457(attrs[i], "no parameters", decl)
            if (u.Requires & Require.NotGeneric) != 0 and decl.TypeParams.Count != 0:
                报 E0457(attrs[i], "non-generic", decl)
            if (u.Requires & Require.HasBody)    != 0 and !decl.HasBody:
                报 E0457(attrs[i], "a body", decl)

        # 层 3：多重性（只在首次出现处报，避免 N 条重复）
        if !u.AllowMultiple and _countSame(attrs, n, attrs[i].Name) > 1 and _isFirst(i):
            报 E0453(attrs[i])

        # 层 4：伴随
        if u.Needs    != "" and !_anyPresent(attrs, n, u.Needs):     报 E0454(attrs[i], "needs")
        if u.Excludes != "" and  _anyPresent(attrs, n, u.Excludes):  报 E0454(attrs[i], "excludes")

_isVoid(te):  return te is NamedType and (te as NamedType).Name == "void"
```

**错误恢复**：位置违规（E0452）后**直接跳到下一条 attribute**——对贴错地方的 attribute 再谈签名/多重性
没有意义，只会刷屏。签名违规**每条谓词各报一次**（一个方法可能同时"有返回值"且"有参数"，两条都告诉用户
比只报第一条更有用）。多重性只在首次出现处报一次。全程不中断遍历，一次编译报全所有违规。

### 5.3 Target 判定：完整分支

| AST 形态 | 判定式 | Target |
|---|---|---|
| `ClassDecl` | `Kind == "class" / "struct" / "interface"` | `Class` / `Struct` / `Interface` |
| `EnumDecl` | | `Enum` |
| `DelegateDecl` | | `Delegate` |
| `MethodDecl` | `IsFree` | `Function` |
| `MethodDecl` | `IsCtor` | `Ctor` |
| `MethodDecl` | `IrGenFacts._hasWord(Mods,"static")` | `StaticMethod` |
| `MethodDecl` | 其余 | `Method` |
| `FieldDecl` | `static` 或 `const` | `StaticField` |
| `FieldDecl` | 其余 | `Field` |
| `PropertyDecl` / `IndexerDecl` | （不分静态，§2.2） | `Property` |
| `Param` | | `Param` |
| `UsingDecl` / `UsingAliasDecl` | **无 Target（返回 0）** | ⇒ 任何已登记 attribute 贴上去都报 E0452 |
| `ImplDecl` **自身** | **无 Target（v1）** | ⇒ 同上 |
| `EnumMember` | **parser 不收**（`EnumMember` 无 `Attrs` 字段）| 不适用，见 §12 |

判定**全部纯语法**——`IsFree` / `IsCtor` / `_hasWord(Mods,"static")` / `Kind`，不依赖类型解析，
与 E0444/E0445/E0447 同级、first-pass 可用。

### 5.4 与其他机制的交互

| 机制 | 交互 | 处理 |
|---|---|---|
| **`AttributeSynth`** | 若 `[Usage]` 走 store-meta，会被合成 `new UsageAttribute(...)` 工厂 → typecheck 报未知类型 | 归 directive（§4.6）即免疫；**这是 `[Usage]` 必须是 directive 的第二个理由** |
| **`BenchmarkDesugar`** | form-2 `[Benchmark] void f(Bencher b)` 合法，但表里 `[Benchmark]` 声明了 `Require.NoParams` | pass 必须在 RunAst 之后（§5.1）；desugar 产出的 wrapper 是零参自由函数 → 天然合规。**若相位搞反，全仓 66 处 benchmark 立刻全红** |
| **Generator（`Augment`/`Replace`）** | 产物经 post-bind **double-bind 重编**，会再过一遍 SymbolCollector | 生成代码**同样受 usage 约束**（合意：generator 不能绕过位置规则）；越界另有 E0448 守 |
| **partial 类型** | 同名类型的多个碎片各自带 attribute | v1 **按 `AttributedDecl` 逐个校验**，多重性**不跨碎片合并**（跨碎片合并语义留 §12） |
| **`[Record]` 位置参** | parser 就地展开成 public 字段 + 主构造器 | 合成字段**不带 attribute** → 不触发本 pass |
| **`#suppress` / `[Suppress]`** | 只过滤 `AnalyzerDriver.DiagSinkImpl.Report` 的 analyzer 诊断 | E04xx **不可抑制**（§2.3 ③） |
| **跨包 imported attribute 类** | 本 CU 找不到 `<Name>Attribute` | **一律"未登记 → 跳过"**（§7：usage 不持久化）。内建 attribute 不受影响——它们在编译器表里，处处可查 |

---

### 5.5 语义相 pass：`_passAttributeUsageSemantic`（带参谓词）

只有声明了 `implements:` / `derivesFrom:` 的 attribute 才进这条路径——**绝大多数 attribute 不进**，
故它对编译时间基本无影响。

```
_passAttributeUsageSemantic(cus, count, symbols):
    for cu in cus: for each (attr, decl) 收集自语法相 pass 的「待定清单」:
        u = _usageOf(attr.Name)
        if u.ImplementsType != "" and !symbols.ImplementsTransitively(decl, u.ImplementsType):
            报 E0457(attr, "a type implementing `" + u.ImplementsType + "`", decl)
        if u.DerivesFromType != "" and !symbols.DerivesFromTransitively(decl, u.DerivesFromType):
            报 E0457(attr, "a type deriving from `" + u.DerivesFromType + "`", decl)
```

- **待定清单**由语法相 pass 顺带收集（它已经在遍历，重复遍历是浪费），避免走两遍 AST。
- **相位**：与 E0913（`[ShouldThrow<E>]` 判继承链）同一相位，两者可共用继承链查询工具函数。
- **传递语义**：`implements: typeof(IHandler)` 命中"直接实现"和"经基类/父接口间接实现"。
  直接实现的语法判定 [`DeclEnforcer._baseHasSimpleName`](../../../../src/compiler/z42c.semantics/src/DeclEnforcer.z42#L86)
  已经有了（E0444/0445/0447 在用），传递闭包要走符号表——这正是它归语义相的原因。
- **跨包基类**：`decl` 的基类若来自其他包，继承链经 `ImportedSymbolLoader` 建好的 `ClassType` 走，
  与 `[Deprecated]` 的跨包 use-site 告警同一套数据。**注意这与 §7 不矛盾**：这里查的是**被贴类型**的
  继承链（本包声明、符号表里有），不是跨包读 attribute 的 usage。

## 6. 诊断：完整码表与消息模板

`E0451` 是当前最后一个占用码。

| Code | 名 | 触发 | 消息模板 |
|---|---|---|---|
| `E0452` | `AttributeTargetInvalid` | 贴在 usage 未声明的位置 | `` `[Test]` can only be applied to a free function or a `static` method (got: instance method `MathTests.test_add`) `` |
| `E0453` | `AttributeNotRepeatable` | `AllowMultiple=false` 却出现 ≥2 次 | `` `[Test]` cannot be applied more than once to the same declaration `` |
| `E0454` | `AttributeCompanionViolation` | `Needs` 未满足 / `Excludes` 冲突 | `` `[Skip]` requires one of `[Test]`, `[Benchmark]` on the same declaration `` / `` `[Test]` cannot be combined with `[Benchmark]` `` |
| `E0455` | `UnknownUsageTarget` | `[Usage(Target.Statik)]`；或 `Require.*` 与非可调用 Target 矛盾组合 | `` unknown usage target `Target.Statik`; expected one of: Class, Struct, … `` |
| `E0456` | `UnknownCompanionAttribute` | `needs`/`excludes` 名字无对应内建/类 | （随 §4.4 的 v1 简化**推迟**）|
| **`E0457`** | **`AttributeRequirementViolated`** | **签名形状不满足 `Require.*`** | 见下 |

E0457 的四条消息（**每条谓词各报一次**）：

```
error[E0457]: `[Test]` requires a `void` return type (got: `int`)
error[E0457]: `[Test]` requires no parameters (got: 1 parameter)
error[E0457]: `[Test]` cannot be applied to a generic method
error[E0457]: `[Test]` requires a method body
```

**消息必须带允许集**——只说"不允许"而不说"允许什么"，用户还得去翻文档。E0452 的 `(got: ...)` 要带上
**被贴声明的形态 + 名字**，因为一个方法上贴多个 attribute 时光看 span 分不清是哪个位置违规。

### 6.1 E0911 / E0912 / E0915 被 E0457 取代

§1.2 那三个丢失的码本来分别是"`[Test]` 签名错 / `[Benchmark]` 签名错 / `[Setup]`·`[Teardown]` 签名错"
——**同一条规则按 attribute 名劈成三份**。签名约束进声明（§2.2b）之后，规则只有一条、
落点只有 `AttrUsage.Requires` 一个字段，三个码就没有存在理由了：

- **E0911 / E0912 / E0915 → 标记为 superseded by E0457**，号段**保留不复用**（历史可追溯）。
- **E0913 / E0914 / E0917 保留**——它们是**实参语义**（`[ShouldThrow<E>]` 的 E 须继承 `Exception`、
  `[Skip]` 的 reason 非空、`[Timeout]` 的毫秒数 >0），不是签名形状，也不是纯语法（E0913 需符号表）。
  归 §2.1 第 5 层，随 P2。

> ⚠️ **发码手法（硬约束）**：沿用 E0449/E0450/E0451 已踩过三次的坑——语义层**先用字面量 `"E0452"` 发码**，
> **不**引 `DiagnosticCodes.*` 新常量，避免 `z42c.core → z42c.semantics` 新增跨成员符号撞
> **F2 冷启动 stale-cache**；待新码随 nightly 载入后再切常量引用。常量仍要在 DiagnosticCodes.z42 里
> 定义 + 注释说明"语义层暂用字面量"。

## 7. 持久化：**不写进 zpkg**（决策 D7，2026-09-10 修订）

usage 是**纯编译期契约，零 zpkg 持久化**——与 `[Suppress]` 完全同档：

| | `[Usage]` | 对照 |
|---|---|---|
| `KindOf` | `Directive` | 同 `[Native]`/`[Suppress]`/`[Record]`/`[Deprecated]` |
| `AttributeSynth` 合成反射工厂 | ❌ 不合成 | 同上四者 |
| 写 store-meta blob | ❌ | 同上 |
| `ClassDescBuilder` 追加 attr-ref | ❌ **一个字节都不写** | 同 `[Suppress]`（异于 `[Deprecated]` 的 `$Deprecated` 哨兵）|
| 运行期反射可见 | ❌ `GetCustomAttributes()` 里没有 | 同 `[Suppress]` |
| zbc 格式影响 | **零**（不 bump、不建字段、golden 字节不动）| —— |

**收益**：整个变更**没有任何格式面**。P1–P3 全程不碰 zbc、不碰 `TsigReconcile`、不碰
`ImportedSymbolLoader`、不需要哨兵编码/版本位/前向兼容策略——早期草案里的 `$Usage` 哨兵、
`IrAttrUsage`、`UsageMask` 全部**删除**。

### 7.1 代价：跨包的用户 attribute 不校验

包 A 定义 `[Usage(Target.Class)] class RouteAttribute`，包 B 写 `[Route] void f()` —— **B 编译通过**。
B 的编译器看不到 A 的 usage（A 的 zbc 里没有），按 §2.3 ① "未登记 → 跳过"处理。

**这是明确接受的代价**，理由三条：

1. **需要强制的那批不受影响**。`[Test]`/`[Native]`/`[Record]`/`[Deprecated]` 全是**内建**——
   规则在编译器的 `UsageOf` 表里，**处处可查、跨包一致**。而它们正是"违规会在运行期炸"的那批
   （§1.1）。用户自定义 attribute 是被动元数据，贴错位置的后果是"没人读它"，不是崩溃。
2. **同包仍然校验**。attribute 定义与使用在同一个包里（最常见的形态：应用内部的标注约定）照常拦。
3. **真需要跨包强制时有正牌出口**：**写一个 `Analyzer`**。analyzer zpkg 会被载进编译器 VM、能看到
   消费方的 AST、能报诊断、severity 可经 `[lints]` 调、可 `#suppress`——这正是 §2.1 第 5 层的分工。
   attribute 库作者想跨包管住用法，就随库发一个 analyzer；这比让每个 attribute 都往 zbc 里塞元数据更合理。

> **可逆性**：将来若跨包强制成为真需求，加回哨兵是**纯增量**（`ClassDescBuilder` 追加一条 attr-ref +
> 两处邻位读取，见 git 历史里的 `$Deprecated` 路径），不需要推翻本设计的任何其他部分。
> 早期草案的哨兵方案存档在 §8.2 T1 一档。

## 8. 编译期求值：三档，以及为什么 usage 走 T0/T1

### 8.1 横向调研

| 语言 | 机制 | 无存储 | **跨编译单元** | **与继承** |
|---|---|---|---|---|
| **D** | `enum x = f();` manifest constant + CTFE | ✅ 无地址 | 值落进 `.di` / object 元数据 | **是成员 → 随继承可见** |
| **Zig** | `comptime` / `const` decl 天然编译期 | ✅ | 全源编译 | 无继承 |
| **Nim** | 编译器**内嵌 VM**：`const x = proc()` / `static:` / macro | ✅ | 全源编译 | —— |
| **Jai** | `#run` 任意编译期执行 | ✅ | 全源编译 | —— |
| **Rust** | `const`（用点内联）/ `const fn` / MIR 解释器 | ✅ | 值落进 rmeta | trait 关联 const **可被 impl 覆盖** |
| **C++** | `constexpr` / `consteval` | ⚠️ | 头文件 = 源可见 | static 成员随继承 |
| **C#** | `const` | ✅ | **值写进 assembly metadata**（改 public const 是破坏性变更） | 随继承可见 |
| **Java** | `static final` + `@Target` 元注解 | ❌ | class 文件 | 随继承可见 |
| **Swift** | 宏角色写在**宏声明修饰位**：`@attached(member) macro Foo()` | —— | swiftmodule | —— |

**规律 ①：凡「编译期求值 + 分离编译」的语言，值最终都得落进某种元数据。**
唯一豁免的是**全源编译**语言（Zig/Nim/Jai——消费方直接看得见定义方的源）。z42 是分包编译 + zbc 产物，
属于前者：**「跨包可见 + 一字节不写 zpkg」在信息论上不成立**，除非编译器能把定义包的代码跑起来（T2）。

**规律 ②：把 usage 做成"成员"，就自动继承了继承语义。**
D 的 manifest `enum`、C# 的 `const`、Rust 的关联 const、Java 的 `static final`——**全是成员、全随继承可见**，
Rust 的关联 const 甚至明确设计成可被 impl 覆盖。而 usage 的既定决策是**无隐式继承**（§2.3 ②）。

**并且：没有任何一门主流语言把 attribute 的 target 做成类的成员。** C#/Java/Kotlin 一律贴在声明头上，
Swift 写在 `macro` 声明的修饰位。理由同一个——**usage 描述的是"这个声明本身"，不是"这个类型的内容"**。

### 8.2 z42 的底牌：T2 已经是生产能力

| 档 | 读法 | 同包 | 跨包 | 持久化 | 代价 |
|---|---|---|---|---|---|
| **T0 语法读** | 读 `Attr.Args` AST | ✅ | ❌ | 无 | ~0 |
| **T1 元数据读** | `$Usage` 哨兵（**早期草案，已弃**，见 §8.3）| ✅ | ✅ | ~20 字节串，不 bump/不建字段/不进反射 | ~0 |
| **T2 编译期执行** | 把定义包载进编译器 VM，**真跑代码取值** | ❌ 自身包不行 | ✅ | 无（代码本就在 zpkg） | 大，见下 |

**T2 在 z42 不是设想** —— [AnalyzerLoader.z42:29-45](../../../../src/compiler/z42c.semantics/src/AnalyzerLoader.z42#L29)
已在生产里这么干：`__load_module(path)` 把任意 zpkg 的函数/类型**并入编译器 live VM**（Path-B），
`AssemblyLoadContext.Default().Load` 枚举类型（Path-A），再 `Type.GetType` + `Activator.CreateInstance`
+ **虚调用**。这是一台完整的编译期执行引擎。

根因：**z42c 本身就是跑在 z42 VM 上的 z42 程序**——与 Nim（编译器内嵌 VM）、Jai（`#run`）同类。
C#/Java **做不到**：Roslyn analyzer 是 .NET 程序，读的是 C# 的**元数据**，无法执行被编译语言的任意代码取值。

**增量正确性的前置也已具备**：
[`BuildPaths._handlerFingerprint`](../../../../src/compiler/z42c.driver/src/BuildPaths.z42#L81)
把 handler zpkg 的**内容指纹**揉进源 hash（换 generator → 全量重编）——编译期执行最容易翻的那个车
（"代码变了缓存没失效"）已经解决过一遍。

**T2 的代价不在技术，在策略**：

1. **供应链面**：今天只有 z42.toml `[analyzers]` **显式声明**的包进编译器 VM。usage 走 T2 意味着
   **任何被 `using` 的依赖**都要载入 → "任意依赖都能在你的编译期跑任意代码"。
2. **载入即有副作用**：`__load_module` 会跑 static initializer
   （[module_load.rs](../../../../src/runtime/src/corelib/reflection/module_load.rs) 的 `init_static_fields`）。
3. **自身包不适用**：编译包 A 时 A 还没编出来 → 同包只能 T0。**T2 省不掉 T0，只能叠加。**
4. **开销**：为读一个位掩码而载入并初始化整个依赖包。

### 8.3 决策：**只用 T0**

早期草案是 T0（同包）+ T1（跨包哨兵）。2026-09-10 修订后**只用 T0**：

| 档 | 用不用 | 理由 |
|---|---|---|
| **T0 语法读** | ✅ **唯一采用** | 同包够用；零持久化、零格式面 |
| **T1 元数据（哨兵）** | ❌ 不用 | 跨包的用户 attribute 交 analyzer（§7.1 ③）；省掉整个格式面 |
| **T2 编译期执行** | ❌ 不用 | 供应链面 + 载入副作用 + 自身包不适用（§8.2）；且 T2 省不掉 T0 |

**结果**：本变更**没有跨编译单元的信息传递问题**，§8.1 规律 ① 那道信息论的坎根本不用过——
因为我们不跨编译单元传。内建那批靠编译器自带的表跨包一致，用户那批只管同包。

**handler 侧（`*Generator`/`*Analyzer`）另论**：它们的 zpkg 会被载进编译器 VM（T2 天然可用），
但 `[Usage]` 不持久化 → **反射也读不到**。所以 handler 侧要跨包声明 usage，只能走**契约方法**
（`Targets()` / `Requires()`，编译器把它真调起来）——那正是 §8.4 说"in-body 在 handler 侧合理"的场景。
这条留 Deferred（§12），不在本变更范围。

### 8.4 被否决的 in-body 形态（存档理由）

| 形态 | 零 parser 改动 | 同包可读 | 跨包 | 运行期代价 | 依赖新语言能力 |
|---|---|---|---|---|---|
| **类头 `[Usage(...)]`**（采纳） | ✅ | ✅ | ✅ 哨兵 | ✅ 无 | ✅ 无 |
| 类体 `const int Usage = Target.A \| Target.B` | ✅ | ❌ **跨类 const 墙** | ❌ 仍需哨兵 | ✅ const 无存储 | ❌ 需扩 const env |
| 类体 `static Target[] Usage = [...]`（读初始化器 AST） | ✅ | ✅ | ❌ 仍需哨兵 | ❌ 真字段：存储 + static-init + 反射噪声 | ✅ |
| 类体 `override Target[] Usage()` | ✅ | ⚠️ 读**方法体** AST，极脆 | ❌ 仍需哨兵 | ❌ 真方法 | ✅ |
| 类头新子句 `class X : Attribute usage(...)` | ❌ parser+AST+golden 全动 | ✅ | ❌ TYPE 段新字段 → **bump** | ✅ | ✅ |

三条否决理由，按份量排：

**① 继承语义（§8.1 规律 ②）** —— 成员形态**自带**错误的语义：`class AdminRouteAttribute : RouteAttribute`
会继承父类 usage，而设计要求不继承；要压掉就得在名字查找里给魔法名开特例。**这不是能打补丁的问题，
是范畴选错了。**

**② `const` 形态今天编译不过** —— [`MemberCollector.z42:74`](../../../../src/compiler/z42c.semantics/src/MemberCollector.z42#L74)
的 `classConstEnv` **每个类新建、只累积本类已声明的 const**（`:309-311` 只 Put 本类的裸名 + `Class.名`）。
引用别类的 `Target.Class` → `ConstEval` 返 null → 报 `ConstExprBadRef`
*「const expression can only reference already-defined const values」*。enum 成员同理（`ConstEval` 的 env
是 `StrMap`，**不查 `syms.EnumConsts`**）。存量实证：全仓 `const` 字段共 11 处，
[const_basic:7](../../../../src/tests/const/const_basic/source.z42#L7) 注释写死适用范围*「引用**同类**已定义 const」*。
（注意：`|` 本身**是支持的**——`ConstEval._evalBinary` 覆盖 `& | ^`。挡路的是跨类引用，不是位或。）

**③ 跨包照样要哨兵** —— `FieldSymbol.ConstVal` 是**纯内存**的：`ClassExtractor` / `TsigReconcile` /
`ImportedSymbolLoader` 全链路搜不到 `ConstVal`，**zbc 不带 const 值**（`src/tests/cross-zpkg/` 下亦无
`const_cross_pkg` 用例）。static 字段的值更不在 zbc（由运行期 `__static_init__` 算）。
**in-body 的净收益 = 观感；净成本 = 一个语言级前置 + 同样的哨兵。**

---

## 9. 配套独立变更：`add-comptime-eval`（草案，不阻塞本变更）

若要的是**通用编译期求值能力**（而非 usage 这一格），它该独立立项，形状建议：

```z42
comptime {                                   // 编译期块：编译器执行，不产码
    ...
}
comptime int MaxSlots = computeSlots();      // 编译期绑定
```

三条设计要点，正好绕开 §8.1 两条规律的坑：

1. **`comptime` 是绑定，不是成员** —— 不进 `Members`、不参与名字查找、**子类看不见、不参与继承**。
   这是它与 `const` 的根本区别，也是"usage 不该继承"那个顾虑的正解。
   （对标 D 的 manifest `enum` 时**故意偏离**：D 的是成员，所以随继承可见。）
2. **求值器分两级**：纯表达式走已有 `ConstEval`（`& | ^`、一元二元、命名 const 均已支持）；
   要调函数走 T2 —— 复用 `AnalyzerLoader` 的 Path-A/Path-B 范式 + `_handlerFingerprint` 的增量模型。
   - **顺带补的前置缺口**：`classConstEnv` 每类新建 → 跨类 const / enum 成员引用今天不支持（§8.4 ②）。
     这是个 C# 有而 z42 缺的独立语言洞，本身值得补。
3. **跨包默认不可见** —— 这才是"不写入 zpkg"的诚实版本。需要跨包时**显式**选一个持久化通道
   （哨兵串 / 新字段）由使用方 opt-in，而**不是**让 `comptime` 隐式变成 ABI 的一部分。

**与本变更解耦**：`[Usage(...)]` 的实参本来就是常量表达式，`add-comptime-eval` 落地后若想改成 comptime
拼写，是**换容器不换语义**。两件事可按序做，现在不必赌。

---

## 10. 分期

| 期 | 内容 | 改语法 | 改格式 | 交付 |
|---|---|---|---|---|
| **P1** | §3 内建表（**Target + Require 一起**）+ §5 强制 pass + E0452/0453/0455/**0457** + T0 审计 + fixture 重冻结 | ✗ | ✗ | **`[Test]` 贴实例方法 → 编译错**；**`[Test] int f(int x)` → 编译错**；`[Record]` 贴方法、重复贴 全部拦下 |
| **P2** | 实参语义检查：E0913（`[ShouldThrow<E>]` 的 E 存在且继承 `Exception`）+ E0914（`[Skip]` reason）+ E0917（`[Timeout]` 值）+ E0454 伴随约束 | ✗ | ✗ | 兑现 error-codes.md 剩余声称；修文档假陈述 |
| **P3** | §4 `[Usage]` directive + `Std.Target` / `Std.Require` enum + 同包强制 | ✗ | ✗ | 用户 attribute 可自声明位置与签名 |

**没有 P4** —— 跨包持久化按 §7 取消。

**与早期草案的差别**：签名约束从 P2 上移进 **P1**（它进了声明式的表，和位置规则同一个 `AttrUsage.Of` 调用），
P2 缩到只剩需要符号表 / 实参语义的三个码。**E0911/E0912/E0915 不再实现**（被 E0457 取代，§6.1）。

P1 独立可交付，且是触发本变更那个洞的**完整**修复——位置和签名一起。

**Deferred（不在本变更）**：handler 侧（`*Generator`/`*Analyzer`）的 usage 声明。它们不持久化 → 反射读不到 →
只能走契约方法（编译器载入 zpkg 后真调），是另一套读法，见 §8.3。

## 11. 测试策略

- **T0 审计先于表落地**（tasks.md T0）：全仓枚举每个内建名的实际贴附位置；**表必须先覆盖存量**。
- **Negative golden**，每码 ≥2 例：
  - E0452：`[Test]` 实例方法 / `[Test]` 贴类 / `[Record]` 贴方法
  - **E0457：`[Test] int f()`（有返回值）/ `[Test] void f(int x)`（有参数）/ `[Test] void f<T>()`（泛型）/
    `[Test] void f();`（无体）**
  - E0453：`[Test][Test]`
  - E0454：`[Skip]` 无 `[Test]` / `[Test][Benchmark]`
  - E0455：`[Usage(Target.Statik)]` / `[Usage(Target.Class, Require.NoParams)]`（矛盾组合）
  - `[Usage]` 贴非 Attribute 类
  断言**码 + 消息含允许集**。
- **Positive 回归**：全仓 3807 处合法 `[Test]`、66 处 `[Benchmark]`（**含 form-2 `Bencher` 参数**）、
  262+59+27 处 `[Native]`、78 处 `[Record]` 一处不能报错 —— `./xtask test` 全绿 + golden 不动点。
  （⚠️ 别跑整套 `cargo test`，会挂在 signal helper 上；门禁走 `./xtask test`。）
- **fixture 重冻结**：`with-tidx` 改 `static` 后按
  [zbc-format/README.md](../../../../src/tests/zbc-format/README.md) 流程 regen，
  `git diff` 应**只有** `param_count` / `param_types` / `is_static` 三处 + 随之的字节偏移，逐行看懂才能合。
- **自举**：本 pass 会作用于**编译器自身源码**。P1 合入前须确认 `src/compiler/**` + `src/libraries/**` 零违规
  （T0 矩阵已初步覆盖），否则自举第一轮就红。
  （踩坑提示：worktree 供种的第一轮红先怀疑种子旧。）
- **相位回归**：专门一个 golden 锁住 §5.1 —— form-2 `[Benchmark] void f(Bencher b)` **必须编译通过**
  （防止将来有人把 pass 上移到 BenchmarkDesugar 之前）。

---

## 12. 明确不做 / Deferred

- **`Target.A | Target.B` 位或**：用逗号变长位置参（§4.3）。
- **attribute 继承 / `Inherited` 语义**：无隐式继承（§2.3 ②）。
- **运行期反射读 usage**：纯编译期契约。跨包只走哨兵（编译器读），**不给活实例**、不进
  `GetCustomAttributes()`——同 `[Deprecated]`（持久化但不反射成实例）。
- **enum 成员上的 attribute**：`EnumMember` 无 `Attrs` 字段，parser 不收 → 无 `EnumMember` target。
  要支持须先扩 parser + AST，属独立语言变更。
- **局部变量 attribute**：`StmtParser` 不解析（上游 design 已记）。
- **partial 碎片间的多重性合并**：v1 逐碎片校验（§5.4）。
- **未知 attribute 名的专用诊断**：`[Nonexistent]` 今天报的是合成工厂里的通用 typecheck 错——
  属另案 `attribute-future-dedicated-diagnostics`，与本设计正交。
- **用户 attribute 的签名约束**：交 `Analyzer`（§2.1 第 4 层）。
- **`ImplDecl` 自身作为 target**：v1 无此 target；`impl` 块内的方法照常校验。
- **跨包用户 attribute 的强制**：usage 不持久化（§7.1），跨包一律不校验；真需要时写 `Analyzer`。
- **handler 侧（`*Generator`/`*Analyzer`）的 usage 声明**：不持久化 → 反射也读不到 → 只能走**契约方法**
  （编译器载入 zpkg 后真调），是另一套读法，见 §8.3。留待独立变更。
- **闭集之外的谓词**：按 §2.2c 准入四条标准把关；兜不住的交 `Analyzer`。

## 13. 文档同步（随对应期）

- [attributes.md](../../../design/language/attributes.md)：`attribute-future-attributeusage` 移出 Deferred，
  写清 directive 落法 + 静态/实例分家的理由。
- [error-codes.md](../../../design/compiler/error-codes.md)：加 E0452–E0456；**修 E0911 段的假陈述**
  （当前写"已启用"、指向已删除的 C# 文件）。
- [testing.md](../../../design/testing/testing.md)：§编译期校验 指向新 pass 而非已删的 C# validator。
- [attribute-handler-registry design](../attribute-handler-registry/design.md)：§位置限制 `[Targets]`
  标注"由 enforce-test-attr-placement 取代/细化"。

## 14. 决策记录

| # | 决策 | 理由 |
|---|---|---|
| D1 | Target **区分静态/实例方法** | §1.1 的失败模式就在这个轴上；C#/Java/Kotlin 不分是因为它们没有零接收者调用路径 |
| D2 | 属性**不**分静态/实例 | 无已知消费者、无对应失败模式；只在有真实失败模式的轴上分 |
| D3 | `[Usage]` 是 **directive**，非 `: Attribute` 类 | 消除 C# 元属性自循环；免 backing 类；`AttributeSynth` 天然跳过 |
| D4 | 变长位置参，**不用** `\|` | 保持语法相 pass 纯语法（`ConstEval` 支持 `\|`，但不值得为此降级 pass） |
| D5 | 缺省 = 不校验 | opt-in 加严，零存量破坏 |
| D6 | 无隐式继承 | 沿用上游；也是否决 in-body 成员形态的根据（§8.1 规律 ②）|
| **D7'** | **usage 完全不写进 zpkg**（2026-09-10 修订，取代早期的 `$Usage` 哨兵方案）| 零格式面；需强制的那批是内建（编译器表，跨包一致）；跨包用户 attribute 交 `Analyzer`（§7.1）。可逆：将来要加哨兵是纯增量 |
| **D8'** | **签名约束进声明**（`Require.*`，取代早期"签名硬编码"）| 一个 attribute 的应用约束应在一处可读完；四条谓词全是纯语法，进声明不降级 pass（§2.1）|
| D9 | v1 用户侧不开 `needs`/`excludes` | 唯一真实用户是内建 test 家族；避免 stringly-typed |
| D10 | 通用编译期求值独立立项 | §9；与本变更解耦，`[Usage]` 将来可换容器不换语义 |
| **D11** | **谓词按「适用域 × 相位」两轴组织**，语法相/语义相分两个 pass | 位置与签名违规要尽早报（零依赖）；`implements:` 要等继承链。整体后移会让用户在连锁错误里找病灶（§2.2c 轴二）|
| **D12** | **谓词准入四条标准**（纯声明性 / 判定一两行 / 有真实需求 / 后果明确）| 防止闭集长成迷你 DSL；兜不住的交 `Analyzer`（§2.2c）|
| **D13** | 无参谓词走位置参、带参谓词走命名参 | `Attr.Args` 本来就按 `AssignExpr` 分流；`typeof(T)` 是既有合法实参形态（§2.2c）|
| **D14** | E0911/E0912/E0915 **不实现**，标记 superseded by E0457 | 它们是同一条规则按 attribute 名劈成三份；规则进表后落点只有一个字段（§6.1）|

---

## 附录 A. 用法示例（用户视角）

> 每个示例标注它属于哪一期。**P1 落地后**内建部分（A.1/A.2）即生效——**位置和签名一起**；
> `[Usage]` 自声明（A.3–A.7）随 P3。跨包（A.8）**不做**，原因见 §7.1。

### A.0 速查：内建 attribute 能贴哪里（P1 起强制）

| attribute | 自由函数 | static 方法 | 实例方法 | 构造器 | 类 | struct | 字段 | 属性 | 参数 | 可重复 |
|---|:-:|:-:|:-:|:-:|:-:|:-:|:-:|:-:|:-:|:-:|
| `[Test]` `[Benchmark]` `[Setup]` `[Teardown]` ² | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| `[Skip]` `[Ignore]` `[ShouldThrow<E>]` `[Timeout]` | ✅¹ | ✅¹ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| `[Native("...")]` | ❌ | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ | ✅ | ❌ | ❌ |
| `[Record]` | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| `[Deprecated]` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ | ❌ |
| `[Suppress("Id")]` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| 用户 attribute（未写 `[Usage]`） | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |

¹ 且**必须**与同声明上的 `[Test]` 或 `[Benchmark]` 同贴（否则 E0454）。
² 除位置外还有**签名约束**：`void` 返回 + 无参数 + 非泛型 + 有方法体（否则 E0457）。
  `[Benchmark] void f(Bencher b)`（form-2）**是合法的**——它在检查前已被脱糖成零参 wrapper。

### A.1 内建：`[Test]` 的三种合法写法（P1）

```z42
namespace MyLib.Tests;
using Std.Test;

// ① 顶层自由函数 —— 最常用，全仓 3807 处都是这个形态
[Test]
void test_add() { Assert.Equal(3, 1 + 2); }

// ② 类内 static 方法 —— 想用类做分组时
class MathTests {
    [Test]
    public static void test_sub() { Assert.Equal(5, 10 - 5); }
}

// ③ 组合修饰
[Test]
[Timeout(milliseconds: 5000)]
[ShouldThrow<TestFailure>]
void test_slow_throwing() { Assert.Fail("boom"); }
```

**为什么必须零接收者**：runner 按 TIDX 里的全限定名走 `ModuleLoader.Invoke(fqn)` 无参调用
（[Runner.z42:6-8](../../../../src/libraries/z42.test/src/Runner.z42#L6)），实例方法的第一个参数是
receiver，对不上。

### A.2 内建：五种会被拦下的写法（P1）

```z42
class MathTests {
    [Test]
    void test_add() { }              // ❌ 实例方法
}
```
```
error[E0452]: `[Test]` can only be applied to a free function or a `static` method
              (got: instance method `MathTests.test_add`)
  --> src/tests/zbc-format/with-tidx/source.z42:5:5
   = help: add `static`, or move it to a top-level function
```

```z42
[Test] class MathTests { }           // ❌ 贴在类上
```
```
error[E0452]: `[Test]` can only be applied to a free function or a `static` method
              (got: class `MathTests`)
```

```z42
[Test] [Test] void test_dup() { }    // ❌ 重复
```
```
error[E0453]: `[Test]` cannot be applied more than once to the same declaration
```

```z42
[Skip(reason: "flaky")]              // ❌ 没有 [Test]
void test_orphan() { }
```
```
error[E0454]: `[Skip]` requires one of `[Test]`, `[Benchmark]` on the same declaration
```
> 这条**不是洁癖**：今天这么写会让 TIDX 凭空多出一条 `Kind=Test` 的 skipped entry，
> 测试报告里出现一个你没写过的"测试"。

```z42
[Test] [Benchmark] void test_both() { }   // ❌ 互斥
```
```
error[E0454]: `[Test]` cannot be combined with `[Benchmark]`
```

### A.2b 内建：签名不对也拦（P1，E0457）

```z42
[Test] int  test_ret() { return 1; }      // ❌ 有返回值
[Test] void test_arg(int x) { }           // ❌ 有参数
[Test] void test_gen<T>() { }             // ❌ 泛型
[Test] void test_nobody();                // ❌ 无方法体
```
```
error[E0457]: `[Test]` requires a `void` return type (got: `int`)
error[E0457]: `[Test]` requires no parameters (got: 1 parameter)
error[E0457]: `[Test]` cannot be applied to a generic method
error[E0457]: `[Test]` requires a method body
```

**这四条以前是 E0911/E0912/E0915 在管**——那套校验随 C# 编译器一起被删、没移植（§1.2），
现在它们不再是硬编码，而是内建表里的一行：

```z42
if (name == "Test") {
    return AttrUsage.Of(Target.ZeroReceiver,
                        Require.VoidReturn | Require.NoParams | Require.NotGeneric | Require.HasBody,
                        false, "", "Benchmark,Setup,Teardown");
}
```

**`[Test]` 的全部应用约束，一行读完。**

### A.3 用户 attribute：**不写 `[Usage]` = 今天的行为，一字不改**（P3）

```z42
public class DocAttribute : Attribute {
    public string Text;
    public DocAttribute(string text) { this.Text = text; }
}

[Doc("a type")]   class Foo { }        // ✅
[Doc("a method")] void bar() { }       // ✅
[Doc("a field")]  int baz;             // ✅
[Doc("x")] [Doc("y")] void qux() { }   // ✅ 缺省 allowMultiple = true
```

**未登记 → 不校验。** `[Usage]` 是 opt-in 的加严，升级编译器不会让你现有的 attribute 报错。

### A.4 用户 attribute：写 `[Usage]` 收紧（P3）

```z42
using Std;

// 这个 attribute 只允许贴在类和 static 方法上，且同一处只能贴一次
[Usage(Target.Class, Target.StaticMethod, allowMultiple: false)]
public class RouteAttribute : Attribute {
    public string Path;
    public string Method;
    public RouteAttribute(string path, string method = "GET") {
        this.Path = path; this.Method = method;
    }
}
```

用对：
```z42
[Route("/users")]                              // ✅ Target.Class
public class UsersController {
    [Route("/users/list", method: "POST")]     // ✅ Target.StaticMethod
    public static void List() { }
}
```

用错：
```z42
public class UsersController {
    [Route("/users/detail")]
    public void Detail() { }                   // ❌ 实例方法
}
```
```
error[E0452]: `[Route]` can only be applied to a class or a `static` method
              (got: instance method `UsersController.Detail`)
```

```z42
[Route("/a")] [Route("/b")] class Dup { }      // ❌ allowMultiple: false
```
```
error[E0453]: `[Route]` cannot be applied more than once to the same declaration
```

**注意用名与类名的对应**：类名 `RouteAttribute`（D8 强制后缀），贴的时候写 `[Route]`（剥后缀）。
`[Usage]` 贴在**带后缀的类**上，约束的是**剥后缀的用名**。

### A.4b 用户 attribute：约束签名与类特征（P3）

**「只能贴在无返回值、无参数的静态函数上」**——与内建 `[Test]` 完全同款，用户可自己写出来：

```z42
[Usage(Target.ZeroReceiver,
       Require.VoidReturn, Require.NoParams, Require.NotGeneric, Require.HasBody,
       allowMultiple: false)]
public class SmokeTestAttribute : Attribute { }
```
```z42
[SmokeTest] void check_boot() { }                    // ✅
[SmokeTest] static void check_db() { }               // ✅（类内 static 同理）

[SmokeTest] int  check_ret() { return 0; }           // ❌ E0457: requires a `void` return type
[SmokeTest] void check_arg(int retries) { }          // ❌ E0457: requires no parameters
class Probe { [SmokeTest] void run() { } }           // ❌ E0452: 实例方法
```

**「只能贴在具有某些特征的类上」**——类特征谓词（§2.2c）：

```z42
[Usage(Target.Class,
       Require.Sealed,            // 必须 sealed
       Require.Instantiable,      // 必须有可访问的无参构造器
       implements: typeof(IHandler))]    // 必须（传递地）实现 IHandler
public class HandlerAttribute : Attribute { }
```
```z42
[Handler] public sealed class PingHandler : IHandler {     // ✅
    public PingHandler() { }
    public void Handle() { }
}

[Handler] public class OpenHandler : IHandler { ... }      // ❌ 非 sealed
[Handler] public sealed class Orphan { ... }               // ❌ 没实现 IHandler
[Handler] public sealed class NeedsArgs : IHandler {       // ❌ 无无参构造器
    public NeedsArgs(int n) { ... }
}
```
```
error[E0457]: `[Handler]` requires a `sealed` type (got: class `OpenHandler`)
error[E0457]: `[Handler]` requires a type implementing `IHandler` (got: class `Orphan`)
error[E0457]: `[Handler]` requires an instantiable type — a public parameterless
              constructor (got: class `NeedsArgs`)
```

其余可用的类特征谓词：`Require.NotAbstract`、`Require.NoInstanceState`（无实例字段）、
`Require.AllFieldsPublic`、`Require.NotGeneric`，以及带参的 `derivesFrom: typeof(T)`。
字段/属性域有 `Require.Readonly`、`Require.Public`。

> **无参谓词写位置参、带参谓词写命名参**（`implements:` / `derivesFrom:`）——因为后者要带一个 `typeof(T)`。
> **报错时机不同**：无参谓词是**语法相**，和位置违规一起在第一趟就报；`implements:` 是**语义相**
> （要等继承链建好），在稍后一个 pass 报（§5.5）。对用户是无感的，同一个 E0457。

> **兜不住的怎么办**：闭集有准入标准（§2.2c：纯声明性 / 判定一两行 / 有真实需求 / 后果明确）。
> 想要"字段名必须以 `_` 开头"这类，**写一个 `Analyzer`**——它能跑任意判定、severity 可经 `[lints]` 调、
> 可 `#suppress`。这是刻意的分工，不是能力缺失。

### A.5 可写的 Target 成员（P3）

```z42
// 单个
[Usage(Target.Class)]
// 多个 —— 逗号，不是 `|`
[Usage(Target.Field, Target.StaticField, Target.Property)]
// 别名
[Usage(Target.Type)]           // = Class | Struct | Interface | Enum | Delegate
[Usage(Target.ZeroReceiver)]   // = Function | StaticMethod（test 家族用的就是它）
[Usage(Target.AnyMethod)]      // = Function | StaticMethod | Method
[Usage(Target.AnyField)]       // = Field | StaticField
[Usage(Target.Any)]            // 全部（= 不写 [Usage] 的效果，但显式声明"我确实到处都能贴"）
// 加命名参
[Usage(Target.Method, allowMultiple: true)]

// Require.*（§2.2c）—— 与 Target.* 混在同一个位置参列表
[Usage(Target.ZeroReceiver, Require.VoidReturn, Require.NoParams)]
[Usage(Target.Class, Require.Sealed, Require.NoInstanceState)]
[Usage(Target.Field, Require.Readonly)]
// 带参谓词 → 命名参
[Usage(Target.Class, implements: typeof(IDisposable))]
[Usage(Target.Class, derivesFrom: typeof(Controller))]
```

拼错立刻报错，不会静默失效：
```z42
[Usage(Target.Statik)] public class FooAttribute : Attribute { }
```
```
error[E0455]: unknown usage target `Target.Statik`; expected one of:
              Class, Struct, Interface, Enum, Delegate, Function, StaticMethod,
              Method, Ctor, Field, StaticField, Property, Param,
              Type, ZeroReceiver, AnyMethod, AnyField, Any
   = note: signature predicates are written as `Require.*`, e.g. `Require.VoidReturn`
```

**适用域矛盾也当场报**（不等到用点）：
```z42
[Usage(Target.Class, Require.NoParams)] public class BarAttribute : Attribute { }
```
```
error[E0455]: `Require.NoParams` applies to callable declarations, but `[Bar]`'s
              targets are `Class` — this requirement can never be satisfied
```

### A.6 `[Usage]` 自己用错（P3）

```z42
[Usage(Target.Class)]
public class NotAnAttribute { }        // ❌ 基类不是 Attribute
```
```
error[E0452]: `[Usage]` can only be applied to a class deriving from `Attribute`
              (got: class `NotAnAttribute`)
```

```z42
[Usage(Target.Class)] [Usage(Target.Method)]
public class FooAttribute : Attribute { }    // ❌ 贴两次
```
```
error[E0453]: `[Usage]` cannot be applied more than once to the same declaration
```

### A.7 继承**不**传递（P3）

```z42
[Usage(Target.Class)]
public class RouteAttribute : Attribute { ... }

public class AdminRouteAttribute : RouteAttribute { ... }   // 没写 [Usage]

[AdminRoute] void handler() { }    // ✅ 通过 —— AdminRoute 是"未登记"，不校验
```

`AdminRouteAttribute` **不继承**父类的 `Target.Class` 限制。要一样的约束就自己再写一遍
`[Usage(Target.Class)]`。

> 这是**刻意**的（决策 D6）。C#/D/Rust/Java 里编译期常量做成成员就自动随继承走；z42 把 usage 放在
> 声明头上而非成员位，正是为了让"不继承"成为默认。

### A.8 跨包：**不校验**（设计取舍，§7.1）

```z42
// ── 包 web.core ──────────────────────────
[Usage(Target.Class)]
public class RouteAttribute : Attribute { ... }
```
```z42
// ── 包 myapp（依赖 web.core）──────────────
using Web.Core;

class Api {
    [Route("/api")] public void Get() { }   // ⚠️ 编译通过 —— 跨包不校验
}
```

usage **完全不写进 zpkg**（纯编译期，同 `[Suppress]`），所以 `myapp` 的编译器看不到 `web.core` 的
`[Usage]` 声明，按"未登记 → 跳过"处理。

**为什么可以接受**：

1. **需要强制的那批不受影响** —— `[Test]`/`[Native]`/`[Record]`/`[Deprecated]` 是**内建**，
   规则在编译器自带的表里，**跨包一致**。而它们才是"违规会在运行期炸"的那批。
   用户 attribute 是被动元数据，贴错位置的后果是"没人读它"，不是崩溃。
2. **同包照常校验** —— attribute 定义与使用在同一个包（最常见形态）不受影响。
3. **真要跨包强制，写个 `Analyzer`** —— analyzer zpkg 被载进编译器 VM、看得到消费方 AST、
   能报诊断、severity 可调、可 `#suppress`。attribute 库作者想管住下游用法，就随库发一个 analyzer。

```toml
# myapp 的 z42.toml
[analyzers]
web-core-lints = { version = "1.0" }     # web.core 随库发的用法检查
```

> 将来若跨包强制成为真需求，加回持久化是**纯增量**（`ClassDescBuilder` 追加一条 attr-ref +
> 两处邻位读取，路径与 `[Deprecated]` 的 `$Deprecated` 哨兵完全相同），不推翻本设计任何其他部分。

### A.9 "配置"：能不能调 severity / 局部关掉？

**不能。** usage 违规是编译器硬错（E04xx），不是 analyzer 诊断：

```z42
#suppress E0452 "我就要这么写"     // ⚠️ 无效
[Test] void Foo.bar() { }
```

- `z42.toml` 的 `[lints]` 段只覆盖 **analyzer 规则**（`Z9xxx`），不覆盖 E 码。
- `#suppress` / `[Suppress]` 只过滤 `AnalyzerDriver` 的诊断汇报路径，E 码不经过那里。

**要放宽只有一条路：改 `[Usage]` 声明本身**（加一个 Target，或干脆删掉 `[Usage]` 回到"不校验"）。
内建 attribute 的位置（`[Test]` 等）**不可放宽**——它们是运行期真的会崩，不是风格问题。

### A.10 升级影响：现有代码要改什么

**几乎不用改。** 全仓审计（§1.4）：3807 处 `[Test]` + 66 处 `[Benchmark]` + 348 处 `[Native]` +
78 处 `[Record]` 全部合规，**唯一违规是 1 个 fixture 的 2 个方法**：

```z42
// src/tests/zbc-format/with-tidx/source.z42
class MathTests {
-   [Test] void test_add() { Assert.Equal(3, 1 + 2); }
+   [Test] public static void test_add() { Assert.Equal(3, 1 + 2); }
-   [Test] void test_sub() { Assert.Equal(5, 10 - 5); }
+   [Test] public static void test_sub() { Assert.Equal(5, 10 - 5); }
}
```

改完按 [zbc-format/README.md](../../../../src/tests/zbc-format/README.md) 重新冻结 `source.zbc` +
`expected.json`（`param_count` 1→0、`param_types` 置空、`is_static` false→true）。

**用户代码零影响**：不写 `[Usage]` 就没有新约束；内建 attribute 只要你原本**能跑**就一定合规——
位置和签名两类约束刻画的都是"runner 真的调得动"，跑不通的那些本来就是 bug（§1.1 的 arity 崩、
§1.2 那套丢失的校验本来就该拦下它们）。

**格式零影响**：usage 不写进 zpkg（§7），所以 `.zbc` 字节、zpkg 版本、反射行为**全部不变**——
唯一的字节变化来自上面那个 fixture 的源码改动本身。
