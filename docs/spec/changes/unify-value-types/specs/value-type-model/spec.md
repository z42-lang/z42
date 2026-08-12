# Spec: 统一值类型模型（Phase 1 —— 消灭 Z42PrimType）

## ADDED Requirements

### Requirement: 基元关键字解析到 Std.* 值类型

#### Scenario: int 解析为 Std.Int32 Z42ClassType（Scalar）
- **WHEN** 类型解析器解析裸类型名 `int`（或别名，源码写法不变）
- **THEN** 返回一个 `Z42ClassType`，`IsStruct==true`、`Repr==Scalar`、指向 z42.core 已存在的 `Std.Int32`
  phantom struct（带其 Methods/接口），**不再返回 `Z42PrimType`**

#### Scenario: 每个基元关键字映射到对应 wrapper
- **WHEN** 解析 `long`/`bool`/`char`/`float`/`double` 及别名（byte/short/uint/...）
- **THEN** 分别解析到 `Std.Int64`/`Std.Boolean`/`Std.Char`/`Std.Single`/`Std.Double`（别名归到对应
  canonical wrapper），全部 `Repr==Scalar`

### Requirement: Repr 区分 Scalar 与 Blob

#### Scenario: 六个基元 wrapper 是 Scalar
- **WHEN** 查询 `Std.Int32`/`Int64`/`Boolean`/`Char`/`Double`/`Single` 的 Repr
- **THEN** 返回 `Scalar`（运行时载体为裸 `Value` 变体，不进字节 arena）

#### Scenario: 多字段用户 struct 是 Blob
- **WHEN** 查询 `Point{int x; int y}` 的 Repr
- **THEN** 返回 `Blob`（现 `IsBlobStruct` 语义，走 StructLayout 字节 arena）

### Requirement: 算术热路径与 codegen 输出不变（纯重构不变式）

#### Scenario: int 算术仍直发原生指令
- **WHEN** 编译 `int a; int b; a + b`
- **THEN** 仍发 `AddInstr` 直加 `Value::I64`（Scalar 经 `ToIrType`→`PrimTag(i32)`），`_emitBinary` 逻辑不变

#### Scenario: self-host 逐字节不动点
- **WHEN** 用改后 z42c 编译 z42c 自身源码（gen1）再自编（gen2）
- **THEN** gen1 与 gen2 产物 **byte-identical**；全量 golden 输出逐条不变（Phase 1 是编译器内部类型模型
  的纯重构，emit 的 IR/zbc 逐字节相同）

### Requirement: 装箱按 Repr 路由，特例保留

#### Scenario: 整数擦除到 object 走 __box_prim
- **WHEN** `object o = someInt`（Scalar 整数擦除到 object/接口）
- **THEN** 编译器插 box，codegen 发 `__box_prim`（runtime `Value::Boxed` 表示不变）

#### Scenario: bool/char/double 不装箱
- **WHEN** `object o = someBool`（或 char/double/float/string）
- **THEN** **不插 box**（有独立 `Value::Bool/Char/F64/Str` 变体自带类型身份），行为与现状一致

#### Scenario: Blob struct 擦除走 __box_struct
- **WHEN** `object o = somePoint`（Blob struct 擦除到 object）
- **THEN** codegen 发 `__box_struct`（runtime `Value::BoxedStruct` 表示不变）

## REMOVED / MODIFIED Requirements

### Requirement: Z42PrimType 类删除

**Before:** 基元由 `Z42PrimType`（纯名字包装）承载；类型系统二元割裂；七张并行名字桥接映射。
**After:** `Z42PrimType` 类删除；基元由 `Std.*` `Z42ClassType(IsStruct, Repr=Scalar)` 承载；七表收敛成
单一 PrimModel 入口；`ImportedSymbolLoader` 不再产 `Z42PrimType` 降级哨兵。

## IR Mapping

无新 IR 指令、无 zbc/zpkg 格式变化。Scalar 值类型的 IR 类型标签由 `ToIrType`→`PrimTag` 产出，与现状逐字节相同。

## Pipeline Steps

- [ ] Lexer —— 不涉及
- [ ] Parser / AST —— 不涉及（`int` 源码写法不变）
- [x] TypeChecker —— 类型解析 / 可赋性 / 装箱 / 重载（核心触面）
- [x] IR Codegen —— `ToIrType` / `BoxIfNeeded` 路由（输出不变）
- [ ] VM interp —— 不涉及（Phase 1 纯编译器）
