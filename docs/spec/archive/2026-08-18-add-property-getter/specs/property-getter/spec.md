# Spec: Computed Property Getter

## ADDED Requirements

### Requirement: 命名属性支持计算 getter 块体

#### Scenario: 计算 getter 返回派生值
- **WHEN** 声明 `public int Doubled { get { return this.n * 2; } }` 且 `n` 是字段，`o.n = 21`
- **THEN** `o.Doubled` 求值为 `42`（调用 getter 函数体，而非读取 backing field）

#### Scenario: 计算 getter 引用其它属性
- **WHEN** 计算 getter body 内访问同类另一个属性（`this.Other`）
- **THEN** 正确派发到 `get_Other`，返回其值

#### Scenario: 计算属性无 backing field
- **WHEN** 类含一个计算属性 `X { get { ... } }`
- **THEN** 该类的 TSIG own-fields 与运行时 layout 中**不含** `__prop_X`；`o.X` 编译为 `VCall get_X`，
  不产生 `FieldGet __prop_X`

#### Scenario: auto-property 不受影响
- **WHEN** 声明 `public int V { get; }`（`get;` 分号形式）
- **THEN** 仍按 auto-property 处理：合成 `__prop_V` backing field + `field.get` 空桩（行为与本变更前一致）

#### Scenario: extern 属性不受影响
- **WHEN** 声明 `[Native("__foo")] public bool F { get; }`
- **THEN** 仍走 extern builtin 桩，无 backing field（行为与本变更前一致）

### Requirement: get-only（计算 setter 不在本变更）

#### Scenario: 计算 getter + auto 无冲突
- **WHEN** 计算属性只写 `get { ... }`
- **THEN** 编译通过；不要求也不提供计算 `set { ... }`

## IR Mapping

无新 IR 指令。计算属性 getter → 一个普通实例方法 `get_<Name>()`（0 逻辑参数），body 经
`FunctionEmitter.EmitFunction` 正常 emit；读取点 `x.Name` → 既有 `VCallInstr get_<Name>`。

## Pipeline Steps

- [x] Lexer —— 无改动（复用 `{` `}` `get`）
- [x] Parser / AST —— `_parseProperty` 捕获 get-body；`PropertyDecl.HasGetBody/GetBody`
- [x] TypeChecker —— DeclBinder 绑 getter body（env=this+字段）；SymbolCollector 抑制 backing own-field
- [x] IR Codegen —— IrGen `HasGetBody` → EmitFunction；ClassDescBuilder 抑制 backing runtime field
- [x] VM interp —— 无改动（普通方法调用）
