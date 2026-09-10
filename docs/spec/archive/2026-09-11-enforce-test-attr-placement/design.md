# Design: 编译期约束 `[Test]` 家族的应用位置与签名

> **范围**：只做一件事——让 `[Test]` / `[Benchmark]` / `[Setup]` / `[Teardown]` **必须是零接收者、
> 无返回值、无参数、非泛型、有体的函数**，违规在**编译期**报错。
> **不做**通用 attribute usage 框架（`[Usage]` / `Target` enum / 注册表 / 跨包）——存档在
> [design-full-usage-framework.md](design-full-usage-framework.md)，等出现第二个用例再取用。
> **不新增诊断码**、**不改语法**、**不改格式**。

---

## 1. 要修的东西

### 1.1 `[Test]` 贴实例方法，编译期全绿、运行期炸

```
① MemberParser.z42:98        任何成员的前置 `[X]` 一律包成 AttributedDecl，不看被贴的是什么
② TestIndexBuilder.z42:48    扫类成员只判 `md.HasBody && _hasTestAttr(ad)` —— 不判 static，照写 TIDX entry
③ Runner.z42:6-8             不变量（只写在注释里）：test 函数是 zero-arg FREE function，
                             按 TIDX 全限定名调用 —— "not via instance + MethodInfo"
④ invoke.rs:234              __invoke_static → invoke_arity_check(qualified, param_count, 0)
                             实例方法 param_count 含 receiver → 1 ≠ 0
⑤ 运行期                      MethodInfo.Invoke: `X` expects 1 argument(s) (incl. receiver), got 0
```

**同一失败模式两年内第二次**：[BenchmarkDesugar.z42](../../../../src/compiler/z42c.semantics/src/BenchmarkDesugar.z42)
文件头记着——`[Benchmark] void f(Bencher b)` 的脱糖 pass 随 C# 编译器一起被删（f8ff73d5）、没移植到 z42c，
form-2 benchmark 从此在运行期挂，报的是**一模一样**的 `expects 1 argument, got 0`，2026-07-20 才补回。

### 1.2 这套校验本来有，在自举迁移中整套丢了

[error-codes.md:139](../../../design/compiler/error-codes.md#L139) 至今写着
「E0911/E0912/E0914/E0915（R4.A **已启用**，2026-04-30）」，实施位置指向
`src/compiler/z42.Semantics/TestAttributeValidator.cs` —— 属**已退休的 C# 编译器**。

```
$ grep -rn "E0911|E0915|TestSignatureInvalid" --include=*.z42 src | grep -v DiagnosticCodes.z42
（无输出）
```

六个码（E0911/0912/0913/0914/0915/0917）在
[DiagnosticCodes.z42](../../../../src/libraries/z42c.core/src/DiagnosticCodes.z42) **定义齐全、零引用**。
**本变更 = 把丢的那部分补回来**，不是发明新机制。

### 1.3 存量违规：1 处，且被冻进了 golden

全仓审计（脚本见 [tasks.md](tasks.md) T0）：`[Test]` 3807 处合法 / **2 处贴在实例方法上**，
`[Benchmark]` 66 处、`[Setup]`/`[Teardown]` 各 1 处全合法。唯一违规：

[src/tests/zbc-format/with-tidx/source.z42](../../../../src/tests/zbc-format/with-tidx/source.z42)
```z42
class MathTests {
    [Test]  void test_add() { Assert.Equal(3, 1 + 2); }   // ← 无 static
    [Test]  void test_sub() { Assert.Equal(5, 10 - 5); }
}
```
其冻结产物 `expected.json` 把 bug 固化了：`"param_count": 1, "param_types": ["MathTests"],
"is_static": false`、`"test_index_size": 2` —— 编译器确实给两个实例方法写了 TIDX entry。
这是字节级 golden、**从不执行**，所以没人发现。

---

## 2. 规则

一个声明只要带 `[Test]` / `[Benchmark]` / `[Setup]` / `[Teardown]` 之一，必须全部满足：

| # | 规则 | 判定式（**全是纯语法**） | 为什么 |
|---|---|---|---|
| R1 | **零接收者**：顶层自由函数，或 `static` 方法 | `md.IsFree \|\| IrGenFacts._hasWord(md.Mods,"static")` | runner 按 FQN 无参调用，实例方法的 receiver 对不上（§1.1）|
| R2 | **返回 `void`** | `md.RetType is NamedType && (…).Name == "void"` | 返回值无处可去；`__invoke_static` 的结果被丢弃 |
| R3 | **无参数** | `md.ParamCount == 0` | 同 R1：调用点不传实参 |
| R4 | **非泛型** | `md.TypeParams.Count == 0` | 无处提供类型实参 |
| R5 | **有方法体** | `md.HasBody` | 抽象/接口/extern 声明无体可跑 |

R2 的判定式与 [MemberParser.z42:372](../../../../src/libraries/z42c.syntax/src/MemberParser.z42#L372)
既有的 void 判定同款；R4 读 [`TypeParamList.Count`](../../../../src/libraries/z42c.syntax/src/TypeExpr.z42#L71)。
**五条都不依赖符号表**——与 E0444/E0445/E0447 三个后缀 pass 同级。

### 2.1 修饰类 attribute 不单独管

`[Skip]` / `[Ignore]` / `[ShouldThrow<E>]` / `[Timeout]` 骑在同一个声明上，只要该声明带
`[Test]`/`[Benchmark]` 就已被覆盖。**孤儿修饰**（`[Skip]` 单独出现）留作已知缺口——
它今天会让 TIDX 凭空多一条 skipped entry，但不崩，归 §6 Deferred。

---

## 3. 实现

### 3.1 处理伪代码

写成贴近 z42c 自举子集的形式（`while` 索引循环、`is`/`as`、无闭包、无 `foreach`），可直接照抄。

#### (a) `HandlerRegistry.z42` —— 加一个 4 名子集

既有 `IsTestHandlerAttr` 是 **8 名触发集**（含 `Skip`/`Ignore`/`ShouldThrow`/`Timeout`）。
本 pass 要的是其中的 **4 个 kind attr**（它们才决定诊断码）。按 PR1b 的原则
*「逻辑不变，识别改注册表」*，名字知识留在注册表，不散到 `DeclEnforcer`：

```z42
// 与 IsTestHandlerAttr（8 名触发集）并列的 kind 子集（4 名）。修饰类 attribute 不在其中——
// 它们骑在同一声明上，由 kind attr 的检查覆盖（design §2.1）。
public static bool IsTestKindAttr(string name) {
    return name == "Test" || name == "Benchmark" || name == "Setup" || name == "Teardown";
}

// 既有 8 名集顺手改成复用子集（纯重构，集合不变）：
public static bool IsTestHandlerAttr(string name) {
    if (HandlerRegistry.IsTestKindAttr(name)) { return true; }
    return name == "Ignore" || name == "Skip" || name == "ShouldThrow" || name == "Timeout";
}
```

#### (b) `DeclEnforcer.z42` —— 主体

```z42
// fix-test-attr-placement：`[Test]`/`[Benchmark]`/`[Setup]`/`[Teardown]` 的位置 + 签名强制。
// 补回自举迁移中丢失的 TestAttributeValidator（E0911/E0912/E0915，design §1.2）。
// 纯语法：不查符号表、不依赖类型解析 —— 与 E0444/E0445/E0447 同级。
//
// ⚠️⚠️ 相位：本 pass **必须**在 HandlerRegistry.RunAst 之后跑（三个挂载点天然满足）。
//     RunAst 里的 BenchmarkDesugar 把**合法**的 form-2 `[Benchmark] void f(Bencher b)` 改写成
//     `f$impl(Bencher b)` + 零参 wrapper `[Benchmark] void f()`；在它之前查 R3 会让全仓
//     66 处 benchmark 全红。改动本 pass 位置前先读 design §3.2。
internal void _passTestAttrEnforce(CompilationUnit cu) {
    int i = 0;
    while (i < cu.DeclCount) {
        this._teWalk(cu.Decls[i], "");
        i = i + 1;
    }
}

// 递归下降：顶层 → 类成员（含嵌套类型）→ impl 块方法。
// owner = 外层类型名（"" = 顶层），只用于诊断消息里的限定名。
private void _teWalk(Decl raw, string owner) {
    // ① 本节点若是"带 attribute 的方法"，先查它。
    if (raw is AttributedDecl) {
        AttributedDecl ad = raw as AttributedDecl;
        if (ad.Inner is MethodDecl) { this._teCheck(ad, ad.Inner as MethodDecl, owner); }
    }
    // ② 再下沉到容器（AttributedDecl 也可能包着 ClassDecl，故先 _unwrap）。
    Decl d = this._sc._unwrap(raw);
    if (d is ClassDecl) {
        ClassDecl c = d as ClassDecl;
        int j = 0;
        while (j < c.MemberCount) { this._teWalk(c.Members[j], c.Name); j = j + 1; }
    } else if (d is ImplDecl) {
        ImplDecl im = d as ImplDecl;
        string tn = (im.TargetType is NamedType) ? (im.TargetType as NamedType).Name : "";
        int j = 0;
        while (j < im.MethodCount) { this._teWalk(im.Methods[j], tn); j = j + 1; }
    }
}

// 五条规则（design §2）。诊断锚在 **attribute 的 span**（指着 `[Test]`），不是方法 span——
// 本 parser 的 decl span 只覆盖起始 token，且一个方法可能贴多个 attribute。
private void _teCheck(AttributedDecl ad, MethodDecl md, string owner) {
    Attr kind = this._teKindAttr(ad);
    if (kind == null) { return; }                       // 不带 kind attr → 与本 pass 无关
    string code = this._teCode(kind.Name);
    string use  = "`[" + kind.Name + "]`";

    // R1 零接收者。位置错了就**只报这一条并返回** —— 贴错地方时再说"还得返回 void"只会刷屏。
    if (!md.IsFree && !this._sc._hasWord(md.Mods, "static")) {
        this._sc.Diags.Error(code,
            use + " must be applied to a free function or a `static` method (got: "
                + this._teShape(md, owner)
                + "); add `static`, or move it to a top-level function",
            kind.Span);
        return;
    }
    // R2..R5 各报一次 —— 一个方法可能同时"有返回值"且"有参数"，都说出来比只说第一条有用。
    if (!(md.RetType is NamedType) || (md.RetType as NamedType).Name != "void") {
        this._sc.Diags.Error(code,
            use + " must return `void` (got: `" + md.RetType.Dump() + "`)", kind.Span);
    }
    if (md.ParamCount != 0) {
        this._sc.Diags.Error(code,
            use + " must take no parameters (got: " + md.ParamCount.ToString() + ")", kind.Span);
    }
    if (md.TypeParams.Count != 0) {
        this._sc.Diags.Error(code, use + " cannot be applied to a generic method", kind.Span);
    }
    if (!md.HasBody) {
        this._sc.Diags.Error(code, use + " must have a method body", kind.Span);
    }
}

// 首个命中的 kind attr（它决定诊断码）；无 → null。
private Attr _teKindAttr(AttributedDecl ad) {
    int a = 0;
    while (a < ad.AttrCount) {
        if (HandlerRegistry.IsTestKindAttr(ad.Attrs[a].Name)) { return ad.Attrs[a]; }
        a = a + 1;
    }
    return null;
}

// 码映射（design §3.3）：三个常量早已存在于 z42c.core，直接引用，无 F2 顾虑。
private string _teCode(string name) {
    if (name == "Benchmark") { return DiagnosticCodes.BenchmarkSignatureInvalid; }        // E0912
    if (name == "Setup" || name == "Teardown") {
        return DiagnosticCodes.SetupTeardownSignatureInvalid;                             // E0915
    }
    return DiagnosticCodes.TestSignatureInvalid;                                          // E0911
}

// 诊断里的"你贴到了什么"：instance method `MathTests.test_add` / constructor `Foo`
private string _teShape(MethodDecl md, string owner) {
    string q = (owner == "") ? md.Name : (owner + "." + md.Name);
    if (md.IsCtor) { return "constructor `" + q + "`"; }
    return "instance method `" + q + "`";
}
```

#### (c) `SymbolCollector.z42` —— 三行挂载

```z42
this._enforce._passTestAttrEnforce(cu);   // fix-test-attr-placement：[Test] 家族位置+签名强制
```

紧邻既有三个后缀 pass，三处：
`CollectWithImports`（[:60-62](../../../../src/compiler/z42c.semantics/src/SymbolCollector.z42#L60)）、
`CollectAll`（`:173-177`）、`Collect`（`:198-200`）。
**这三个是互斥的公开入口**（一次编译只走其一），故不会重复报诊断——与既有三个后缀 pass 同构。

#### (d) 走查覆盖表（照着写测试）

| 声明形态 | 走到吗 | R1 结果 |
|---|---|---|
| 顶层 `[Test] void f()` | ✅ ① | `IsFree` → 通过 |
| 类内 `[Test] public static void f()` | ✅ ②→① | `static` → 通过 |
| 类内 `[Test] void f()` | ✅ ②→① | **E0911** |
| 嵌套类内的 `[Test] void f()` | ✅ ②→②→① | **E0911** |
| `impl T for X { [Test] void f() }` | ✅ ②(Impl)→① | **E0911** |
| `[Test]` 贴构造器 | ✅ | **E0911**（`constructor \`Foo\``）|
| `[Test]` 贴类 / 字段 / 属性 / 参数 | ①不命中（`Inner` 非 `MethodDecl`）→ **不报** | 已知缺口，见 §6 |
| `[Skip]` 单独贴（无 kind attr） | `_teKindAttr` 返回 null → **不报** | 已知缺口，见 §6 |
| form-2 `[Benchmark] void f(Bencher b)` | 检查时已被脱糖成零参 wrapper | 通过（§3.2）|

> 表里两处"不报"是**刻意**的最小范围：`[Test]` 贴类今天只是静默无视（不崩），`[Skip]` 孤儿会多一条
> 假 entry（不崩）。它们在 §6 列为已知缺口 + 跟进小项，不在本批。

### 3.2 相位约束（唯一的雷）

**本 pass 必须在 `HandlerRegistry.RunAst` 之后。**
RunAst 里的 [`BenchmarkDesugar`](../../../../src/compiler/z42c.semantics/src/BenchmarkDesugar.z42) 会把
**合法**的 form-2 `[Benchmark] void f(Bencher b)` 改写成 `f$impl(Bencher b)`（attribute 剥掉）+
零参 wrapper `[Benchmark] void f()`。在它之前查 R3，**全仓 66 处 benchmark 立刻全红**。

SymbolCollector 天然在 RunAst 之后（IncrementalDriver:52 先 RunAst 再 collect），**挂载点已满足**——
但必须①写进代码注释、②配一个相位回归 golden（form-2 benchmark 必须编译通过），否则将来有人上移 pass 就踩雷。

### 3.3 诊断：复用已有的死码，**不新增**

| Code | 名（已存在于 DiagnosticCodes.z42） | 用于 |
|---|---|---|
| `E0911` | `TestSignatureInvalid` | `[Test]` |
| `E0912` | `BenchmarkSignatureInvalid` | `[Benchmark]` |
| `E0915` | `SetupTeardownSignatureInvalid` | `[Setup]` / `[Teardown]` |

**这正是这三个码当初的定义**（error-codes.md 的原始描述：*「`[Test]` 函数签名错误：必须 `fn() -> void`、
不能泛型」*），本变更让文档重新变成真的。

> **F2 注**：这三个常量**早已存在**于 z42c.core（非本次新增），可直接引用常量、不必走
> E0449/E0450/E0451 那套"先用字面量"的规避。若种子过旧仍可退回字面量。

消息模板（**必须说清允许什么**）：
```
error[E0911]: `[Test]` must be a free function or a `static` method
              (got: instance method `MathTests.test_add`)
   = help: add `static`, or move it to a top-level function

error[E0911]: `[Test]` must return `void` (got: `int`)
error[E0911]: `[Test]` must take no parameters (got: 1)
error[E0911]: `[Test]` cannot be generic
error[E0911]: `[Test]` must have a body
```

---

## 4. 为什么这批放编译器，而不是放 z42.test / 写成 Analyzer

问得对，先说结论：**analyzer 做得到，但不该由它做这条规则**。

### 4.1 致命点：analyzer 是纯 opt-in

[Main.z42:306](../../../../src/compiler/z42c.driver/src/Main.z42#L306) 是 `if (pm.AnalyzerCount > 0)`
——只有在 z42.toml 显式写了 `[analyzers]` 的包才加载。**没有内建/默认开启的 analyzer 集合。**

实测：
```
带 tests 目录的包：      28
现有 [analyzers] 声明：   0
```

要靠 analyzer 强制，得**改 28 个 manifest**，而且**新建的包默认漏网**——恰恰是新人最容易写错
`[Test] void foo()` 在类里的时候。而这条规则的违规后果是**运行期崩**，不是风格问题。

### 4.2 次要点：可关、可抑制

analyzer 诊断走 `[lints]` 段（severity 逐规则可覆盖、可 `none`）+ `#suppress Z9xxx` 局部抑制。
把"这么写一定崩"做成可关的 lint，语义上就不对。E09xx 是编译器硬错，关不掉。

### 4.3 还有：得新建一个 zpkg

analyzer 要独立编译成 zpkg、进构建/发布链路，且
[`BuildPaths._handlerFingerprint`](../../../../src/compiler/z42c.driver/src/BuildPaths.z42#L81)
会把它的内容指纹揉进源 hash —— **改一行 analyzer = 全量重编**。为一条 60 行的检查付这个代价不划算。

### 4.4 那 analyzer 适合什么

适合**可选的、可调的、非崩溃性**的用法约束——命名规范、代码风格、将来"用户自定义 attribute 该贴在哪"。
那类规则本来就该让每个包自己决定开不开。**内建测试框架的硬规则不属于这一类。**

---

### 4.5 「谁的规则谁处理」——原则对，但今天 `[Test]` 不是 z42.test 的规则

这是本设计被质疑得最有道理的一点，值得把事实摆全。

#### 事实：编译器**已经**深度拥有 `[Test]`

按文件统计写死 test 家族名字 / 语义的位置：

```
19  z42c.semantics/TestIndexBuilder.z42      发现 [Test] + 构建 TIDX 表
13  z42.ir/BinaryFormat/ZbcReader.z42        TIDX section 解码
11  z42c.semantics/HandlerRegistry.z42       8 个名字的识别 + kind 归类
10  z42.ir/BinaryFormat/ZbcWriter.z42        TIDX section 编码
 9  z42c.semantics/IrGen.z42                 TIDX 接线
 5  z42.ir/IrModule.z42                      IrTestEntry / Kind 常量
 4  z42c.driver/IncrementalDriver.z42        增量路径
 2  z42.ir/ZpkgWriter.z42 · BenchmarkDesugar.z42
 1  z42.ir/BinaryFormat/ZbcFormat.z42
```

**12 个文件 + 一个专属 zbc section（TIDX，有自己的 v1→v2 版本史）+ 运行时 decoder
（`runtime/src/metadata/test_index.rs`）。** z42.test **只是 runner**——它 `ModuleLoader.Load(path)`
拿到编译器已经算好的 `TestEntry[]`，然后按 FQN 调用。它今天既不发现测试、也不定义什么算测试。

所以「`[Test]` 是 z42.test 的规则」是**将来的事实，不是现在的事实**。

#### 只搬校验 = 把一条规则劈给两个主人

若把发现（TestIndexBuilder）留在编译器、把校验搬到 z42.test：**编译器决定什么算一个测试，
z42.test 决定它合不合法**。这两个判据一旦漂移就是 bug——**而本变更修的正是这种漂移**：
TestIndexBuilder 认了实例方法（`md.HasBody && _hasTestAttr(ad)`），runner 调不动它（§1.1）。
再劈一次只会让同类漂移更容易发生。

#### 而且本批不是"往编译器塞新概念"

TestIndexBuilder **已经**在判「什么算一个测试」，只是判据不全：

```z42
if (md.HasBody && this._hasTestAttr(ad)) { ... 写 TIDX entry ... }   // 现状：漏判 static/void/arity
```

本批做的是**把这条已经存在的判据补完整并让它出声**，不是引入新机制。没有 `[Usage]`、没有 `Target`
词汇、没有注册表——**60 行 + 复用三个已经躺在 DiagnosticCodes.z42 里的死码**。

> 落点选 `DeclEnforcer` 而不是直接改 `TestIndexBuilder`，是因为前者跑得更早（纯语法、first-pass）、
> 与既有三个 `*SuffixEnforce` 同构，且能覆盖 `impl` 块和嵌套类型——TestIndexBuilder 的扫描面
> 只有顶层 + 类成员，够不着那两处（§3.1(d) 覆盖表）。

### 4.6 你指的方向是对的——正解分两步，都该独立立项

**终态已经记录在案**（[HandlerRegistry.z42:15](../../../../src/compiler/z42c.semantics/src/HandlerRegistry.z42#L15)
原话）：*「design 记 TestIndexBuilder 的**终态**是 store-meta + 反射发现、TIDX 退休——那是独立后续变更」*。

| 步 | 变更 | 解决什么 | 现状 |
|---|---|---|---|
| **① analyzer 随库携带** | 让**依赖**能贡献 analyzer，消费方无需在 z42.toml 声明 | 破掉 §4.1 的 opt-in 死结。**z42.test 已经是每个测试包的依赖**——一旦支持，它自带的检查就 **28/28 自动生效、零 manifest 改动** | 今天 [`_parseAnalyzers`](../../../../src/libraries/z42.project/src/ManifestLoader.z42#L94) 是**扁平显式列表**，无传递/贡献概念 |
| **② TIDX 退休** | test 家族从内建 handler 变普通 store-meta attribute + 反射发现 | 把上面 12 个文件的知识**撤出编译器**，`[Test]` 真正归 z42.test | 已记录为"独立后续变更"，未启动 |

两步做完，验证自然就落在 z42.test 里，而且**是自动生效的**——那才是「谁的规则谁处理」的完整形态。
对标 **Roslyn analyzer 随 NuGet 包分发**（包里放 `analyzers/dotnet/cs/*.dll`，消费方零配置自动生效）
——步骤 ① 就是这个机制的 z42 版。

**本批 60 行是通往那里的桥，不是路障**：它没有引入任何要在 ①② 里拆解的耦合，TIDX 退休时它是
**整体删除的一小块**（连同 TestIndexBuilder 一起走）。而在 ①② 落地之前——那是两个独立变更、
不知多久——**今天写 `[Test] void foo()` 在类里的人仍然会在运行期撞那个看不懂的 arity 错。**

## 5. 验证

- **Positive 回归**（一处不能报错）：3807 处 `[Test]` + 66 处 `[Benchmark]`（**含 form-2 `Bencher` 参数**）
  + `[Setup]`/`[Teardown]` 各 1 处。`./xtask test` 全绿 + golden 不动点。
  （⚠️ 别跑整套 `cargo test`——会挂在 signal helper 上；门禁走 `./xtask test`。）
- **Negative golden**：R1–R5 各 ≥1 例，断言**码 + 消息含允许集**。
- **相位回归 golden**（§3.2）：form-2 `[Benchmark] void f(Bencher b)` 必须编译通过。
- **fixture 重冻结**：`with-tidx` 两个方法加 `static`，按
  [zbc-format/README.md](../../../../src/tests/zbc-format/README.md) regen；
  `git diff` 应**只有** `param_count` 1→0、`param_types` 置空、`is_static` false→true + 随之的字节偏移。
- **自举**：本 pass 作用于编译器自身源码。审计已确认 `src/compiler/**` + `src/libraries/**` 零违规。

---

## 6. 明确不做

| 不做 | 理由 |
|---|---|
| `[Usage]` / `Target` enum / `Require` 谓词 / usage 注册表 | 通用框架为一个用例建太重；存档在 [design-full-usage-framework.md](design-full-usage-framework.md)，出现**第二个**用例再取 |
| 把检查搬进 z42.test | 今天 `[Test]` 是编译器的规则（12 个文件 + TIDX section），不是 z42.test 的；只搬校验会把一条规则劈给两个主人。正解是 §4.6 的两步独立变更 |
| 用户自定义 attribute 的位置约束 | 同上；真有需求时写 `Analyzer`（§4.4 正是它的场景）|
| 跨包 | 不涉及——test 家族是内建，规则在编译器里，跨包天然一致 |
| 其余内建 attribute（`[Native]`/`[Record]`/`[Deprecated]`）的位置约束 | 它们贴错不崩、只是静默无视；审计确认存量全合规。**留作已知缺口** |
| 孤儿修饰（`[Skip]` 无 `[Test]`）→ E0914 | 会让 TIDX 多一条 skipped entry，但不崩；可作为跟进小项 |
| `[ShouldThrow<E>]` 的 E 须继承 `Exception` → E0913 | 需符号表判继承链，不是纯语法，另一个相位；跟进小项 |
| `[Timeout]` 值域 → E0917 | 同上，跟进小项 |
| 新诊断码 | 复用已有的 E0911/E0912/E0915（§3.3）|

## 7. 文档同步

- [error-codes.md](../../../design/compiler/error-codes.md#L139)：**修假陈述**——把 E0911/E0912/E0915 的
  实施位置从已删除的 `src/compiler/z42.Semantics/TestAttributeValidator.cs` 改为
  `z42c.semantics/src/DeclEnforcer.z42`；E0913/E0914/E0917 标注"**未实现**，跟进项"（当前写"已启用"是假的）。
- [testing.md](../../../design/testing/testing.md)：§编译期校验 指向新 pass。
- [attributes.md](../../../design/language/attributes.md)：`attribute-future-attributeusage` 保留在 Deferred，
  加一行指向 [design-full-usage-framework.md](design-full-usage-framework.md)。
