# Spec: 测试 attribute 的位置与签名约束

> Capability：`test-attribute-placement`。
> 适用 attribute：`[Test]` / `[Benchmark]` / `[Setup]` / `[Teardown]`（下称 **kind attr**）。
> 修饰类 attribute（`[Skip]` / `[Ignore]` / `[ShouldThrow<E>]` / `[Timeout]`）不单独触发本约束——
> 它们骑在同一声明上，由该声明的 kind attr 覆盖。

## ADDED Requirements

### Requirement R1: kind attr 只能贴在零接收者函数上

被贴的方法必须是**顶层自由函数**或 **`static` 方法**。

#### Scenario: 顶层自由函数（合法）
- **WHEN** 源码为 `[Test] void test_add() { }`（命名空间级）
- **THEN** 编译通过，无诊断

#### Scenario: 类内 static 方法（合法）
- **WHEN** 源码为 `class T { [Test] public static void t() { } }`
- **THEN** 编译通过，无诊断

#### Scenario: 类内实例方法（违规）
- **WHEN** 源码为 `class MathTests { [Test] void test_add() { } }`
- **THEN** 报 `E0911`，消息含 ``must be applied to a free function or a `static` method``
  与 ``instance method `MathTests.test_add` ``，并含修复提示 ``add `static` ``
- **AND** 不再对该 attribute 报 R2–R5 的任何诊断（位置错了就只报一条）

#### Scenario: 嵌套类内的实例方法（违规）
- **WHEN** `[Test]` 贴在 `class Outer { class Inner { ... } }` 的 `Inner` 实例方法上
- **THEN** 报 `E0911`，消息中的限定名为 ``Inner.<method>``

#### Scenario: impl 块内的实例方法（违规）
- **WHEN** `[Test]` 贴在 `impl Trait for Target { ... }` 块内的实例方法上
- **THEN** 报 `E0911`，消息中的限定名为 ``Target.<method>``

#### Scenario: 构造器（违规）
- **WHEN** `[Test]` 贴在构造器上
- **THEN** 报 `E0911`，消息含 ``constructor `<Class>` ``

### Requirement R2: kind attr 要求返回 `void`

#### Scenario: 非 void 返回（违规）
- **WHEN** 源码为 `[Test] int test_ret() { return 1; }`
- **THEN** 报 `E0911`，消息含 ``must return `void` `` 与实际返回类型 ``（got: `int`）``

### Requirement R3: kind attr 要求无参数

#### Scenario: 有参数（违规）
- **WHEN** 源码为 `[Test] void test_arg(int x) { }`
- **THEN** 报 `E0911`，消息含 `must take no parameters` 与参数个数

#### Scenario: form-2 benchmark 的 Bencher 参数（**合法**）
- **WHEN** 源码为 `[Benchmark] void b(Bencher bb) { }`
- **THEN** 编译通过，无诊断
- **BECAUSE** `BenchmarkDesugar`（在本 pass 之前运行）已把它改写成 `b$impl(Bencher)`（attribute 剥离）
  加零参 wrapper `[Benchmark] void b()`；本 pass 只看得到零参 wrapper

### Requirement R4: kind attr 不能贴在泛型方法上

#### Scenario: 泛型方法（违规）
- **WHEN** 源码为 `[Test] void test_gen<T>() { }`
- **THEN** 报 `E0911`，消息含 `cannot be applied to a generic method`

### Requirement R5: kind attr 要求有方法体

#### Scenario: 无体声明（违规）
- **WHEN** 源码为 `[Test] void test_nobody();`
- **THEN** 报 `E0911`，消息含 `must have a method body`

### Requirement R6: 诊断码按 kind attr 分派

#### Scenario: `[Benchmark]` 违规
- **WHEN** 任一 R1–R5 违规发生在 `[Benchmark]` 上
- **THEN** 诊断码为 `E0912`（`BenchmarkSignatureInvalid`）

#### Scenario: `[Setup]` / `[Teardown]` 违规
- **WHEN** 任一 R1–R5 违规发生在 `[Setup]` 或 `[Teardown]` 上
- **THEN** 诊断码为 `E0915`（`SetupTeardownSignatureInvalid`）

#### Scenario: 多个 kind attr 同贴
- **WHEN** 一个声明同时带 `[Test]` 和 `[Benchmark]` 且违规
- **THEN** 按**声明中首个出现**的 kind attr 定码，只报一次

### Requirement R7: 非 kind attr 与非方法声明不受影响

#### Scenario: 修饰类 attribute 单独出现
- **WHEN** 源码为 `[Skip(reason: "x")] void f() { }`（无 kind attr）
- **THEN** 本 capability **不报任何诊断**（孤儿修饰归 Out of Scope，见 proposal.md）

#### Scenario: kind attr 贴在非方法声明上
- **WHEN** 源码为 `[Test] class Foo { }` 或 `[Test] int field;`
- **THEN** 本 capability **不报任何诊断**（`AttributedDecl.Inner` 非 `MethodDecl`，不进检查；
  归 Out of Scope——贴错不崩、只是静默无视）

#### Scenario: R2–R5 可并发报出
- **WHEN** 源码为 `[Test] int f(int x) { return x; }`（R2 与 R3 同时违规、R1 合法）
- **THEN** 报**两条** `E0911`（一条 `must return void`、一条 `must take no parameters`）

## MODIFIED Requirements

### Requirement: E0911 / E0912 / E0915 的实施位置

**Before:** `docs/design/compiler/error-codes.md` 声称三码「R4.A 已启用（2026-04-30）」，实施位置
`src/compiler/z42.Semantics/TestAttributeValidator.cs` —— 该文件随 C# 编译器退休已删除，
z42c 中**无任何引用**，三码实际处于**未实现**状态。

**After:** 三码由 `z42c.semantics/src/DeclEnforcer.z42` 的 `_passTestAttrEnforce` 实施，
覆盖 R1–R6。文档同步更正。E0913 / E0914 / E0917 明确标注**未实现（跟进项）**。

## Pipeline Steps

受影响的 pipeline 阶段：

- [ ] Lexer
- [ ] Parser / AST
- [x] **TypeChecker 前的声明良构 pass**（`SymbolCollector` → `DeclEnforcer`）
- [ ] IR Codegen
- [ ] VM interp

### 相位约束（关键，必须满足）

本 pass **必须**在 `HandlerRegistry.RunAst` 之后运行。`RunAst` 内的 `BenchmarkDesugar` 会把合法的
form-2 `[Benchmark] void f(Bencher b)` 改写成零参 wrapper；在其之前检查 R3 会让**全仓 66 处
benchmark 全部误报**。三个挂载点均在 `SymbolCollector`（`RunAst` 之后），天然满足；
代码注释与相位回归用例共同锁住这条。

## IR Mapping

无。本变更纯诊断，不产生 IR、不改 zbc/zpkg 格式、不影响任何字节产物
（除 `with-tidx` fixture 因其**源码**被修正而重冻结）。
