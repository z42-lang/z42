# Spec: readonly 字段 + readonly-load 优化

## ADDED Requirements

### Requirement: readonly 字段语法与不变性强制

#### Scenario: ctor 内赋值合法
- **WHEN** `class C { readonly int x; C(int a) { this.x = a; } }`
- **THEN** 编译通过，无诊断

#### Scenario: 字段初始化器合法
- **WHEN** `class C { readonly int x = 5; }`
- **THEN** 编译通过（初始化器等价 ctor 内赋值）

#### Scenario: 非 ctor 方法内赋值报错
- **WHEN** `class C { readonly int x; void set(int a) { this.x = a; } }`
- **THEN** 报 `E0415`（readonly 字段不可在构造函数外赋值），错误计入编译错误数

#### Scenario: 跨对象赋值报错
- **WHEN** ctor 内 `other.x = 1`（other 非 this，即便同类型）
- **THEN** 报 `E0415`（readonly 只允许 `this.<field>`）

#### Scenario: 解析不脱轨
- **WHEN** 一个方法内 readonly 违规赋值
- **THEN** 同类后续成员仍正常绑定（诊断精准、不级联撕毁）

### Requirement: readonly 字段读的块内 CSE

#### Scenario: 同接收者重复读消重
- **WHEN** `class C { readonly int x; int f() { return this.x + this.x; } }`，用 `Opt.ReadonlyLoad`
- **THEN** IR 只剩 **一条** `field_get %0.x`（第二次读 remap 到第一次的 Dst）

#### Scenario: 关闭优化时保留两次读
- **WHEN** 同源码用 `Opt.None`
- **THEN** IR 有 **两条** `field_get %0.x`

#### Scenario: 非 readonly 字段不消重
- **WHEN** 字段非 readonly，`return this.x + this.x`，用 `Opt.ReadonlyLoad`
- **THEN** IR 仍有两条 `field_get`（保守正确，字段可能被其他线程/别名改）

#### Scenario: ctor 内 FieldSet 使值号失效（不误合并）
- **WHEN** ctor 体 `this.x = 1; int r = this.x; this.x = 2; int s = this.x;`
- **THEN** `r` 与 `s` 的读**不**被 CSE 合并（中间 field_set 失效值号）

### Requirement: this 接收者 readonly 字段读的 LICM 外提

#### Scenario: 循环内 this.readonly 字段提到 pre-header
- **WHEN** `class C { readonly int x; int sum(int n){ int s=0; for(int i=0;i<n;i=i+1){ s=s+this.x; } return s; } }`，用 `Opt.ReadonlyLoad`
- **THEN** `field_get %0.x` 出现在循环 pre-header（只算一次），循环体内引用其 Dst；`Opt.None` 时 `field_get` 在循环体内（每迭代）

#### Scenario: 非 this 接收者不外提（v1 保守）
- **WHEN** 循环内读 `param.x`（param 是方法形参，非 this），readonly
- **THEN** v1 **不**外提（保留循环体内；避免 param 可空导致 NPE 时机漂移）

#### Scenario: 循环体内有该字段 FieldSet 时不外提
- **WHEN** 循环体内既读 `this.x` 又（在 ctor 语境不可能，但防御性）写 `this.x`
- **THEN** 不外提（值可能在循环内变）

### Requirement: 无 zbc/zpkg 格式变更

#### Scenario: field_get 编码不变
- **WHEN** 编译任意含 readonly 字段读的代码
- **THEN** 产出的 zbc 中 `field_get` 指令编码与优化前**逐字节相同**（Readonly 是内存标志，序列化前已被优化消费、不写入）；zbc/zpkg version 不 bump

## MODIFIED Requirements

### Requirement: IsPure 白名单（不变）
**Before:** `FieldGet` 不在 `IsPure` 白名单，CSE/LICM 全跳过。
**After:** `IsPure` **仍不放行** `FieldGet`（readonly 只保证值不变，不保证 obj 非空 → DCE/通用 LICM
仍不能删/提）。readonly-load 优化走**独立分支**（CseKey 的 readonly 分支 + IrLicm 的 this-非空分支），
不经 `IsPure`。

## IR Mapping
- `readonly` 字段读 → `FieldGetInstr`（`Readonly=true` 内存标志）→ 优化后可能被 CSE remap 消除或
  LICM 移到 pre-header 块；**序列化到 zbc 的仍是普通 `field_get`**（无新 opcode、无新字段）。

## Pipeline Steps
- [x] Lexer（readonly token）
- [x] Parser / AST（进 FieldDecl.Mods）
- [x] TypeChecker（FieldSymbol.IsReadonly + ctor 上下文 + E0415 + emit 填 FieldGetInstr.Readonly）
- [x] IR Codegen（ExprEmitter）
- [x] IR Opt（CSE + LICM readonly 分支，OptSet 位）
- [x] VM interp（无需改——优化在编译期，VM 执行普通 field_get）
