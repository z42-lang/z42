# Spec: 测试 attribute 的实参语义约束

> Capability：`test-attribute-args`。承接 `test-attribute-placement`（位置 + 签名）。
> 本 capability 只管**实参**：`[Skip]` 的 reason、`[Ignore]`/`[Skip]` 的伴随、`[Timeout]` 的值域、
> `[ShouldThrow<E>]` 的类型实参。

## ADDED Requirements

### Requirement A1: `[Skip]` 必须给出非空 `reason`

#### Scenario: 缺 reason
- **WHEN** `[Test] [Skip] void t() { }`
- **THEN** 报 `E0914`，消息含 ``[Skip]`` 与 `` `reason` ``

#### Scenario: reason 为空串
- **WHEN** `[Test] [Skip(reason: "")] void t() { }`
- **THEN** 报 `E0914`（空理由等于没理由）

#### Scenario: 给了 reason（合法）
- **WHEN** `[Test] [Skip(reason: "flaky on CI")] void t() { }`
- **THEN** 无 `E0914`

### Requirement A2: `[Skip]` / `[Ignore]` 必须与 `[Test]` 或 `[Benchmark]` 同贴

#### Scenario: `[Skip]` 孤儿
- **WHEN** `[Skip(reason: "x")] void t() { }`（同一声明上无 kind attr）
- **THEN** 报 `E0914`，消息含 ``requires`` 与 ``[Test]``
- **BECAUSE** 今天这么写会让 TIDX 凭空多一条 `Kind=Test` 的 skipped entry

#### Scenario: `[Ignore]` 孤儿
- **WHEN** `[Ignore] void t() { }`
- **THEN** 报 `E0914`

#### Scenario: 与 `[Benchmark]` 同贴（合法）
- **WHEN** `[Benchmark] [Ignore] void b() { }`
- **THEN** 无 `E0914`

### Requirement A3: `[Timeout]` 的 `milliseconds` 必填且为正

#### Scenario: 缺 milliseconds
- **WHEN** `[Test] [Timeout] void t() { }`
- **THEN** 报 `E0917`

#### Scenario: 非正值
- **WHEN** `[Test] [Timeout(milliseconds: 0)] void t() { }`
- **THEN** 报 `E0917`，消息含实际值
- **AND** 负值同理

#### Scenario: 正值（合法）
- **WHEN** `[Test] [Timeout(milliseconds: 5000)] void t() { }`
- **THEN** 无 `E0917`

### Requirement A4: `[ShouldThrow<E>]` 必须带类型实参

#### Scenario: 裸 `[ShouldThrow]`
- **WHEN** `[Test] [ShouldThrow] void t() { }`
- **THEN** 报 `E0913`，消息含 `type argument`

### Requirement A5: `E` 必须（传递地）派生自 `Exception`

#### Scenario: 可解析且不派生 Exception
- **WHEN** 源码含 `class Plain { }` 且 `[Test] [ShouldThrow<Plain>] void t() { }`
- **THEN** 报 `E0913`，消息含 ``Exception``

#### Scenario: 直接派生（合法）
- **WHEN** 源码含 `class Exception { }` `class MyError : Exception { }` 且 `[ShouldThrow<MyError>]`
- **THEN** 无 `E0913`

#### Scenario: 传递派生（合法）
- **WHEN** `class Exception { }` → `class A : Exception { }` → `class B : A { }`，`[ShouldThrow<B>]`
- **THEN** 无 `E0913`

#### Scenario: `E` 就是 `Exception` 本身（合法）
- **WHEN** `[ShouldThrow<Exception>]`
- **THEN** 无 `E0913`

#### Scenario: **刻意保守** —— `E` 在符号表中解析不到
- **WHEN** `[ShouldThrow<Nonexistent>]` 且 `Nonexistent` 不在符号表
- **THEN** **不报** `E0913`
- **BECAUSE** 跨包 / 单 CU 收集等场景下符号表可能不含该类型，报错会误伤；
  宁可漏报也不误报（Out of Scope，已知缺口）

### Requirement A6: 只作用于测试 attribute，不越界

#### Scenario: 同名但非测试 attribute
- **WHEN** 用户自定义 attribute 恰好带 `reason` / `milliseconds` 命名参
- **THEN** 本 capability 不报任何诊断（只认 `HandlerRegistry` 的 8 名触发集）

#### Scenario: 位置违规时不叠加实参诊断
- **WHEN** `class C { [Test] [Skip] void t() { } }`（实例方法 + 缺 reason）
- **THEN** 只报位置违规（`E0911`），**不**叠加 `E0914`
- **BECAUSE** 贴错地方时再谈实参没有意义（沿用 `test-attribute-placement` R1 的短路约定）

## MODIFIED Requirements

### Requirement: E0913 / E0914 / E0917 的实现状态

**Before:** `docs/design/compiler/error-codes.md` 在 #564 后把三码标为「**未实现**（跟进项）」。

**After:** 三码由 `DeclEnforcer` 实施 —— E0914 / E0917 在纯语法 pass
`_passTestAttrEnforce`，E0913 在语义相 pass `_passTestAttrSemantic`（需符号表判继承链）。
文档同步。

## Pipeline Steps

- [x] **TypeChecker 前的声明良构 pass**（`SymbolCollector` → `DeclEnforcer`），两个相位
- [ ] 其余阶段不受影响

## IR Mapping

无。纯诊断，不产生 IR、不改 zbc/zpkg 格式、不影响任何字节产物。

> **注**：本变更**不改** `TestIndexBuilder` 的既有降级行为（`if (r > 0)` / `if (ms > 0)`）——
> 那些分支在校验通过后不再可达，留着无害；删它们属另一次清理。
